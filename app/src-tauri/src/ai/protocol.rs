//! Wire protocol between the app and the `emulsion-ai` helper: one JSON
//! object per line on the helper's stdin (requests) and stdout (responses);
//! the helper logs to stderr. Requests carry a caller-chosen `id` that the
//! response echoes; the helper serves them strictly one at a time, in order
//! (the "one queue" of RFC-0026 §3.1), so an encoder pass delays the decode
//! behind it rather than running beside it.

// Shared by `#[path]` with the helper binary: each side uses a different half of these items.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const PROTOCOL_VERSION: u32 = 1;

/// One click, normalized to the full, uncropped image (0..1 on both axes).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Prompt {
    pub x: f32,
    pub y: f32,
    /// `true` = "this is part of it", `false` = "this is not".
    pub positive: bool,
}

/// One decoder candidate: the 256x256 logits as a quantised grayscale PNG,
/// the model's own IoU estimate, and the candidate's index in model order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub index: usize,
    pub iou: f32,
    /// Base64 of an 8-bit grayscale PNG: `0..255` <-> logit `-8..8`.
    pub logits_png: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelPaths {
    pub encoder: PathBuf,
    pub decoder: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Liveness + version handshake; answers `HelloResult`.
    Hello,
    /// Encode the image at `image_path` (the unedited Develop preview) unless
    /// its embedding is cached under `content_hash`; answers `PrepareResult`.
    SegmentPrepare { models: ModelPaths, content_hash: String, image_path: PathBuf },
    /// Decoder only; answers `Vec<Candidate>`, best predicted IoU first.
    SegmentDecode { models: ModelPaths, content_hash: String, prompts: Vec<Prompt>, refine: Option<String> },
    /// Drop the sessions and the cached embedding (the process stays alive).
    Release,
    /// Answer, then exit.
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub id: u64,
    #[serde(flatten)]
    pub request: Request,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    /// Stable machine-readable code: `not_prepared`, `inference`, `image`,
    /// `bad_request`, `internal`.
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<ErrorBody>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloResult {
    pub protocol: u32,
    pub pid: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareResult {
    /// Encoder time in ms; 0 when the embedding was already cached.
    pub encode_ms: u64,
    pub cached: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_frame_is_one_flat_json_object_with_the_op_tag() {
        let frame = Frame {
            id: 7,
            request: Request::SegmentDecode {
                models: ModelPaths { encoder: "e.onnx".into(), decoder: "d.onnx".into() },
                content_hash: "abc".into(),
                prompts: vec![Prompt { x: 0.5, y: 0.25, positive: true }],
                refine: None,
            },
        };
        let line = serde_json::to_string(&frame).unwrap();
        assert!(!line.contains('\n'));
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["id"], 7);
        assert_eq!(v["op"], "segment_decode");
        assert_eq!(v["prompts"][0]["positive"], true);
        let back: Frame = serde_json::from_str(&line).unwrap();
        assert!(matches!(back.request, Request::SegmentDecode { .. }));
    }

    #[test]
    fn unit_requests_and_error_responses_round_trip() {
        let hello: Frame = serde_json::from_str(r#"{"id":1,"op":"hello"}"#).unwrap();
        assert!(matches!(hello.request, Request::Hello));
        let err = Response { id: 2, ok: None, err: Some(ErrorBody { code: "not_prepared".into(), message: "x".into() }) };
        let line = serde_json::to_string(&err).unwrap();
        assert!(!line.contains("\"ok\""));
        let back: Response = serde_json::from_str(&line).unwrap();
        assert_eq!(back.err.unwrap().code, "not_prepared");
    }

    #[test]
    fn an_unknown_op_is_a_parse_error_not_a_panic() {
        assert!(serde_json::from_str::<Frame>(r#"{"id":1,"op":"mine_bitcoin"}"#).is_err());
    }
}
