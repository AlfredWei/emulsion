//! Integration test for the `emulsion-ai` helper and its supervisor (RFC-0026
//! §3.1): the real helper binary, spawned the way the app spawns it, **without
//! model files** (so it runs everywhere, CI included). The model path itself is
//! covered by the helper's `--ignored` real-weights test.

#[path = "../src/ai/mod.rs"]
#[allow(dead_code)]
mod ai;

use ai::protocol::{HelloResult, ModelPaths, Prompt, Request, PROTOCOL_VERSION};
use ai::supervisor::{AiError, AiHelper};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const T: Duration = Duration::from_secs(20);

fn helper_exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_emulsion-ai"))
}

fn models() -> ModelPaths {
    ModelPaths { encoder: "no-such-encoder.onnx".into(), decoder: "no-such-decoder.onnx".into() }
}

fn decode_request() -> Request {
    Request::SegmentDecode { models: models(), content_hash: "never-prepared".into(), prompts: vec![Prompt { x: 0.5, y: 0.5, positive: true }], refine: None }
}

fn pid_alive(pid: u32) -> bool {
    // `kill -0` on unix, `tasklist` on Windows: enough to tell "still there" from "gone".
    if cfg!(windows) {
        let out = std::process::Command::new("tasklist").args(["/FI", &format!("PID eq {pid}")]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).contains(&pid.to_string())
    } else {
        std::process::Command::new("kill").args(["-0", &pid.to_string()]).status().map(|s| s.success()).unwrap_or(false)
    }
}

fn hello_pid(h: &AiHelper) -> u32 {
    let v = h.request(Request::Hello, T).expect("hello");
    let hello: HelloResult = serde_json::from_value(v).unwrap();
    assert_eq!(hello.protocol, PROTOCOL_VERSION);
    hello.pid
}

fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if cond() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {what}");
}

#[test]
fn the_helper_is_not_started_until_the_first_request_and_answers_hello() {
    let h = AiHelper::new(helper_exe(), 0);
    assert!(!h.is_running());
    // Releasing must not spawn a helper just to free nothing.
    assert!(h.request_if_running(Request::Release, T).unwrap().is_none());
    assert!(!h.is_running());
    let pid = hello_pid(&h);
    assert!(h.is_running() && pid_alive(pid));
    h.shutdown();
    wait_until("the helper to exit after shutdown", || !pid_alive(pid));
}

#[test]
fn an_error_the_helper_reports_comes_back_with_its_code() {
    let h = AiHelper::new(helper_exe(), 0);
    match h.request(decode_request(), T) {
        Err(AiError::Remote { code, .. }) => assert_eq!(code, "not_prepared"),
        other => panic!("expected a not_prepared error, got {other:?}"),
    }
    // The helper stays up after an error answer and keeps serving.
    assert!(h.is_running());
    hello_pid(&h);
    h.shutdown();
}

#[test]
fn preparing_with_missing_model_files_or_image_fails_cleanly_without_killing_the_helper() {
    let h = AiHelper::new(helper_exe(), 0);
    let r = h.request(Request::SegmentPrepare { models: models(), content_hash: "x".into(), image_path: "no-such-image.png".into() }, T);
    match r {
        Err(AiError::Remote { code, .. }) => assert_eq!(code, "image"),
        other => panic!("expected an image error, got {other:?}"),
    }
    hello_pid(&h);
    h.shutdown();
}

#[test]
fn the_helper_exits_on_its_own_when_idle_and_the_next_request_respawns_it() {
    let h = AiHelper::new(helper_exe(), 1); // 1 s idle lifetime
    let first = hello_pid(&h);
    wait_until("the idle helper to exit", || !pid_alive(first));
    wait_until("the supervisor to notice", || !h.is_running());
    let second = hello_pid(&h);
    assert_ne!(first, second, "a fresh process after the idle exit");
    h.shutdown();
}

#[test]
fn a_crashed_helper_is_replaced_and_the_request_is_retried_once() {
    let h = AiHelper::new(helper_exe(), 0);
    let first = hello_pid(&h);
    if cfg!(windows) {
        std::process::Command::new("taskkill").args(["/F", "/PID", &first.to_string()]).output().unwrap();
    } else {
        std::process::Command::new("kill").args(["-9", &first.to_string()]).status().unwrap();
    }
    wait_until("the killed helper to be gone", || !pid_alive(first));
    // The supervisor may not have seen EOF yet; either way this request must succeed on a new process.
    let second = hello_pid(&h);
    assert_ne!(first, second);
    h.shutdown();
}

#[test]
fn a_helper_that_cannot_be_started_is_a_spawn_error() {
    let h = AiHelper::new(PathBuf::from("/definitely/not/emulsion-ai"), 0);
    assert!(matches!(h.request(Request::Hello, T), Err(AiError::Spawn(_))));
}

#[test]
fn the_helper_exits_when_the_app_side_closes_the_pipe() {
    let mut child = std::process::Command::new(helper_exe())
        .arg("--idle-secs")
        .arg("0")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdin.take()); // EOF on its stdin, as when the app quits
    wait_until("the helper to exit on EOF", || child.try_wait().unwrap().is_some());
}

fn rss_mb(pid: u32) -> u64 {
    let out = std::process::Command::new("ps").args(["-o", "rss=", "-p", &pid.to_string()]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().parse::<u64>().unwrap_or(0) / 1024
}

/// Real weights through the real helper: `SEGMENT_MODELS_DIR=<dir with the two
/// .onnx files> cargo test --release --test ai_helper -- --ignored --nocapture
/// real_model`. Shows what the process split buys: the *app* process stays small
/// while the helper holds the ~1 GB, and the memory is gone when the helper idles out.
#[test]
#[ignore]
fn real_model_memory_lives_in_the_helper_and_is_returned_when_it_exits() {
    let dir = PathBuf::from(std::env::var("SEGMENT_MODELS_DIR").expect("SEGMENT_MODELS_DIR"));
    let models = ModelPaths { encoder: dir.join("sam2_hiera_tiny.encoder.onnx"), decoder: dir.join("sam2_hiera_tiny.decoder.onnx") };
    let image = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test_image/Field-corn-Liechtenstein-landscape.jpg");
    let h = AiHelper::new(helper_exe(), 3); // exits after 3 idle seconds
    let app_before = rss_mb(std::process::id());
    let t = Instant::now();
    let v = h
        .request(Request::SegmentPrepare { models: models.clone(), content_hash: "corn".into(), image_path: image }, Duration::from_secs(120))
        .expect("prepare");
    let prep: ai::protocol::PrepareResult = serde_json::from_value(v).unwrap();
    println!("prepare via the helper: {} ms encoder, {} ms round trip", prep.encode_ms, t.elapsed().as_millis());
    let t = Instant::now();
    let v = h
        .request(Request::SegmentDecode { models, content_hash: "corn".into(), prompts: vec![Prompt { x: 0.5, y: 0.75, positive: true }], refine: None }, Duration::from_secs(30))
        .expect("decode");
    let cands: Vec<ai::protocol::Candidate> = serde_json::from_value(v).unwrap();
    println!("decode via the helper: {} candidates, best iou {:.3}, {} ms round trip (incl. {} KB of PNGs)", cands.len(), cands[0].iou, t.elapsed().as_millis(), cands.iter().map(|c| c.logits_png.len()).sum::<usize>() / 1024);
    assert_eq!(cands.len(), 3);
    assert!(cands.windows(2).all(|w| w[0].iou >= w[1].iou));
    let pid = hello_pid(&h);
    println!("RSS: app process {} MB before, {} MB after; helper process {} MB", app_before, rss_mb(std::process::id()), rss_mb(pid));
    assert!(rss_mb(std::process::id()) < 400, "the app process must not hold the model memory");
    wait_until("the helper to idle out", || !pid_alive(pid));
    println!("helper exited after idling; its {} MB went back to the OS", rss_mb(pid).max(0));
}
