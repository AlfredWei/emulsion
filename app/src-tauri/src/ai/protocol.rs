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
    /// Denoise the image at `image_path` (or just `region` of it) with the ONNX model at `model`, in overlapping
    /// tiles (RFC-0027 §3.4), and write the result as a PNG at `out_path` (full-image size, or region size).
    /// Streams `progress` lines under the same id; answers `DenoiseResult`, or the error `cancelled`.
    DenoiseRun { model: PathBuf, image_path: PathBuf, out_path: PathBuf, region: Option<Region> },
    /// Enlarge the image at `image_path` (or just `region` of it) with the 4x ONNX model at `model` (RFC-0029):
    /// in overlapping tiles, to exactly `out_width x out_height`, `strength` percent of the model over a plain
    /// Lanczos enlargement, written as a JPEG of `jpeg_quality` at `out_path` row by row (so the size of the result
    /// does not decide the memory). Streams `progress` lines; answers `UpscaleResult`, or the error `cancelled`.
    UpscaleRun {
        model: PathBuf,
        image_path: PathBuf,
        out_path: PathBuf,
        out_width: u32,
        out_height: u32,
        strength: u8,
        jpeg_quality: u8,
        region: Option<Region>,
    },
    /// Ask the running request `target` to stop at its next tile boundary; answers at once, whether or not
    /// `target` is still running (so a late cancel is harmless).
    Cancel { target: u64 },
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

/// A rectangle in full-image pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Interim line of a long request: `done` of `total` units finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub done: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    /// Stable machine-readable code: `not_prepared`, `inference`, `image`,
    /// `bad_request`, `cancelled`, `internal`.
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
    /// Present only on interim lines: the request is still running and its final answer follows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<Progress>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DenoiseResult {
    pub width: u32,
    pub height: u32,
    pub tiles: u32,
    pub ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpscaleResult {
    pub width: u32,
    pub height: u32,
    pub tiles: u32,
    pub ms: u64,
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
        let err = Response { id: 2, ok: None, err: Some(ErrorBody { code: "not_prepared".into(), message: "x".into() }), progress: None };
        let line = serde_json::to_string(&err).unwrap();
        assert!(!line.contains("\"ok\""));
        let back: Response = serde_json::from_str(&line).unwrap();
        assert_eq!(back.err.unwrap().code, "not_prepared");
    }

    #[test]
    fn denoise_requests_and_progress_lines_round_trip() {
        let frame = Frame {
            id: 9,
            request: Request::DenoiseRun { model: "m.onnx".into(), image_path: "in.png".into(), out_path: "out.png".into(), region: Some(Region { x: 1, y: 2, w: 3, h: 4 }) },
        };
        let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&frame).unwrap()).unwrap();
        assert_eq!(v["op"], "denoise_run");
        assert_eq!(v["region"]["w"], 3);
        let no_region: Frame = serde_json::from_str(r#"{"id":1,"op":"denoise_run","model":"m","image_path":"i","out_path":"o","region":null}"#).unwrap();
        assert!(matches!(no_region.request, Request::DenoiseRun { region: None, .. }));
        let cancel: Frame = serde_json::from_str(r#"{"id":2,"op":"cancel","target":9}"#).unwrap();
        assert!(matches!(cancel.request, Request::Cancel { target: 9 }));

        let line = serde_json::to_string(&Response { id: 9, ok: None, err: None, progress: Some(Progress { done: 3, total: 12 }) }).unwrap();
        let back: Response = serde_json::from_str(&line).unwrap();
        assert_eq!(back.progress, Some(Progress { done: 3, total: 12 }));
        // An old-style final answer has no progress field at all.
        let plain: Response = serde_json::from_str(r#"{"id":9,"ok":{}}"#).unwrap();
        assert!(plain.progress.is_none() && plain.ok.is_some());
    }

    #[test]
    fn upscale_requests_round_trip_with_and_without_a_region() {
        let frame = Frame {
            id: 4,
            request: Request::UpscaleRun {
                model: "m.onnx".into(),
                image_path: "in.png".into(),
                out_path: "out.jpg".into(),
                out_width: 4000,
                out_height: 3000,
                strength: 45,
                jpeg_quality: 95,
                region: Some(Region { x: 1, y: 2, w: 3, h: 4 }),
            },
        };
        let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&frame).unwrap()).unwrap();
        assert_eq!(v["op"], "upscale_run");
        assert_eq!((v["out_width"].as_u64(), v["strength"].as_u64(), v["region"]["h"].as_u64()), (Some(4000), Some(45), Some(4)));
        let plain: Frame = serde_json::from_str(r#"{"id":1,"op":"upscale_run","model":"m","image_path":"i","out_path":"o","out_width":8,"out_height":8,"strength":100,"jpeg_quality":90,"region":null}"#).unwrap();
        assert!(matches!(plain.request, Request::UpscaleRun { region: None, strength: 100, .. }));
        // Every field is required: a request that forgets the size must not default to something.
        assert!(serde_json::from_str::<Frame>(r#"{"id":1,"op":"upscale_run","model":"m","image_path":"i","out_path":"o","region":null}"#).is_err());
    }

    #[test]
    fn an_unknown_op_is_a_parse_error_not_a_panic() {
        assert!(serde_json::from_str::<Frame>(r#"{"id":1,"op":"mine_bitcoin"}"#).is_err());
    }
}
