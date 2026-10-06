//! `emulsion-ai`: the long-lived AI helper process (RFC-0026 §3.1, revised
//! 2026-10-06). The app spawns it on first use of an AI feature and talks to it
//! over stdin/stdout (see `ai/protocol.rs`); it owns every ONNX Runtime session
//! and cached embedding, so the editor process never maps the ~1 GB of model
//! memory and a model crash cannot take the editor down.
//!
//! Lifetime: it exits when stdin closes (the app quit or died), on `Shutdown`,
//! or after `--idle-secs` (default 900, 0 = never) without a request, which is
//! what hands the memory back to the OS; the app respawns it on next use.
//! Requests are served strictly one at a time in arrival order.

#[path = "../../ai/protocol.rs"]
mod protocol;
mod segment;

use protocol::{Candidate, ErrorBody, Frame, HelloResult, ModelPaths, PrepareResult, Prompt, Request, Response, PROTOCOL_VERSION};
use segment::{SegmentError, Segmenter};
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const DEFAULT_IDLE_SECS: u64 = 900;

fn error_code(e: &SegmentError) -> &'static str {
    match e {
        SegmentError::NotPrepared(_) => "not_prepared",
        SegmentError::Ort(_) | SegmentError::BadOutput(_) => "inference",
        SegmentError::Image(_) => "image",
        SegmentError::Poisoned => "internal",
    }
}

fn fail(id: u64, code: &str, message: String) -> Response {
    Response { id, ok: None, err: Some(ErrorBody { code: code.to_string(), message }) }
}

fn ok<T: serde::Serialize>(id: u64, value: &T) -> Response {
    Response { id, ok: Some(serde_json::to_value(value).expect("serialisable result")), err: None }
}

fn segment_prepare(seg: &Segmenter, id: u64, models: &ModelPaths, content_hash: &str, image_path: &std::path::Path) -> Response {
    if seg.has_embedding(content_hash) {
        return ok(id, &PrepareResult { encode_ms: 0, cached: true });
    }
    let img = match image::open(image_path) {
        Ok(i) => i.to_rgb8(),
        Err(e) => return fail(id, "image", format!("could not read {}: {e}", image_path.display())),
    };
    match seg.prepare(models, content_hash, &img) {
        Ok(ms) => ok(id, &PrepareResult { encode_ms: ms as u64, cached: false }),
        Err(e) => fail(id, error_code(&e), e.to_string()),
    }
}

fn segment_decode(seg: &Segmenter, id: u64, models: &ModelPaths, content_hash: &str, prompts: &[Prompt], refine: Option<&str>) -> Response {
    match seg.decode(models, content_hash, prompts, refine) {
        Ok(c) => ok::<Vec<Candidate>>(id, &c),
        Err(e) => fail(id, error_code(&e), e.to_string()),
    }
}

fn main() {
    let mut idle_secs = DEFAULT_IDLE_SECS;
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--idle-secs") {
        idle_secs = args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_IDLE_SECS);
    }

    let started = Instant::now();
    let last_activity_ms = Arc::new(AtomicU64::new(0));
    let busy = Arc::new(AtomicBool::new(false));
    if idle_secs > 0 {
        let (last, busy) = (last_activity_ms.clone(), busy.clone());
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis((idle_secs * 1000 / 4).clamp(100, 5000)));
            let idle_ms = started.elapsed().as_millis() as u64 - last.load(Ordering::Relaxed);
            if !busy.load(Ordering::Relaxed) && idle_ms >= idle_secs * 1000 {
                eprintln!("emulsion-ai: idle for {idle_secs}s, exiting to free its memory");
                std::process::exit(0);
            }
        });
    }

    let seg = Segmenter::default();
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        busy.store(true, Ordering::Relaxed);
        let mut shutdown = false;
        let response = match serde_json::from_str::<Frame>(&line) {
            Err(e) => fail(0, "bad_request", format!("unparseable request: {e}")),
            Ok(Frame { id, request }) => match request {
                Request::Hello => ok(id, &HelloResult { protocol: PROTOCOL_VERSION, pid: std::process::id() }),
                Request::SegmentPrepare { models, content_hash, image_path } => segment_prepare(&seg, id, &models, &content_hash, &image_path),
                Request::SegmentDecode { models, content_hash, prompts, refine } => {
                    segment_decode(&seg, id, &models, &content_hash, &prompts, refine.as_deref())
                }
                Request::Release => {
                    seg.release();
                    ok(id, &serde_json::json!({}))
                }
                Request::Shutdown => {
                    shutdown = true;
                    ok(id, &serde_json::json!({}))
                }
            },
        };
        let mut out = stdout.lock();
        if writeln!(out, "{}", serde_json::to_string(&response).expect("serialisable response")).and_then(|_| out.flush()).is_err() {
            break; // the app is gone
        }
        drop(out);
        last_activity_ms.store(started.elapsed().as_millis() as u64, Ordering::Relaxed);
        busy.store(false, Ordering::Relaxed);
        if shutdown {
            break;
        }
    }
}
