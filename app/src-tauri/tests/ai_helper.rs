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

// ---- denoise (RFC-0027): the whole job path with a stand-in model ---------------------------------------
//
// An ONNX file with one `Identity` node, written by hand below, stands in for the denoiser: the real helper
// reads a real PNG, tiles it through a real ONNX Runtime session, streams progress, honours a cancel, and
// writes the result; with an identity model the result must equal the input exactly, which proves the tiling
// and blending end to end without any model weights (so CI runs it).

use ai::protocol::{DenoiseResult, Progress};

fn pb_varint(mut v: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return out;
        }
        out.push(b | 0x80);
    }
}
fn pb_field_varint(field: u32, v: u64) -> Vec<u8> {
    [pb_varint((field as u64) << 3), pb_varint(v)].concat()
}
fn pb_field_bytes(field: u32, bytes: &[u8]) -> Vec<u8> {
    [pb_varint(((field as u64) << 3) | 2), pb_varint(bytes.len() as u64), bytes.to_vec()].concat()
}

/// `out = Identity(image)`, both `1x3x512x512` float: the model contract of the denoisers, minus the denoising.
fn identity_model_bytes() -> Vec<u8> {
    let dims: Vec<u8> = [1u64, 3, 512, 512].iter().flat_map(|&d| pb_field_bytes(1, &pb_field_varint(1, d))).collect(); // TensorShapeProto.dim { dim_value }
    let tensor_type = [pb_field_varint(1, 1), pb_field_bytes(2, &dims)].concat(); // elem_type FLOAT, shape
    let type_proto = pb_field_bytes(1, &tensor_type);
    let value_info = |name: &str| [pb_field_bytes(1, name.as_bytes()), pb_field_bytes(2, &type_proto)].concat();
    let node = [pb_field_bytes(1, b"image"), pb_field_bytes(2, b"out"), pb_field_bytes(3, b"id"), pb_field_bytes(4, b"Identity")].concat();
    let graph = [pb_field_bytes(1, &node), pb_field_bytes(2, b"identity"), pb_field_bytes(11, &value_info("image")), pb_field_bytes(12, &value_info("out"))].concat();
    let opset = pb_field_varint(2, 13); // OperatorSetIdProto { domain "", version 13 }
    [pb_field_varint(1, 8), pb_field_bytes(8, &opset), pb_field_bytes(7, &graph)].concat() // ir_version 8, opset_import, graph
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("emulsion-ai-denoise-it-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A photo-ish test pattern (smooth gradient + a few hard edges) so a seam or an off-by-one would show.
fn test_image(w: u32, h: u32) -> image::RgbImage {
    image::RgbImage::from_fn(w, h, |x, y| {
        let edge = if (x / 97 + y / 61) % 2 == 0 { 40 } else { 0 };
        image::Rgb([((x * 255 / w) as u8).saturating_add(edge), (y * 255 / h) as u8, ((x + y) % 256) as u8])
    })
}

fn denoise_request(dir: &std::path::Path, name: &str, region: Option<ai::protocol::Region>) -> Request {
    Request::DenoiseRun { model: dir.join("identity.onnx"), image_path: dir.join("in.png"), out_path: dir.join(name), region }
}

fn denoise_fixture(name: &str, w: u32, h: u32) -> (PathBuf, image::RgbImage) {
    let dir = scratch(name);
    std::fs::write(dir.join("identity.onnx"), identity_model_bytes()).unwrap();
    let img = test_image(w, h);
    img.save(dir.join("in.png")).unwrap();
    (dir, img)
}

#[test]
fn a_denoise_job_streams_progress_and_an_identity_model_reproduces_the_image_exactly() {
    let (dir, img) = denoise_fixture("identity", 1100, 600); // 3 x 2 tiles of 512 with a 32 px overlap
    let h = AiHelper::new(helper_exe(), 0);
    let mut seen: Vec<Progress> = Vec::new();
    let v = h.request_with_progress(denoise_request(&dir, "out.png", None), T, |p| seen.push(p)).expect("denoise");
    let r: DenoiseResult = serde_json::from_value(v).unwrap();
    assert_eq!((r.width, r.height, r.tiles), (1100, 600, 6));
    assert_eq!(seen.len(), 6, "one progress line per tile: {seen:?}");
    assert!(seen.iter().enumerate().all(|(i, p)| p.done as usize == i + 1 && p.total == 6));
    let out = image::open(dir.join("out.png")).unwrap().to_rgb8();
    assert_eq!(out, img, "tiling an identity model must not change a single pixel");
    assert!(!dir.join("out.png.part").exists());
    h.shutdown();
}

#[test]
fn a_region_denoises_only_that_rectangle() {
    let (dir, img) = denoise_fixture("region", 700, 700);
    let h = AiHelper::new(helper_exe(), 0);
    let region = ai::protocol::Region { x: 100, y: 50, w: 600, h: 300 };
    let v = h.request(denoise_request(&dir, "crop.png", Some(region)), T).expect("denoise region");
    let r: DenoiseResult = serde_json::from_value(v).unwrap();
    assert_eq!((r.width, r.height), (600, 300));
    let out = image::open(dir.join("crop.png")).unwrap().to_rgb8();
    assert_eq!(out, image::imageops::crop_imm(&img, 100, 50, 600, 300).to_image());
    // A region outside the image is an image error, not a silent clamp.
    let bad = ai::protocol::Region { x: 650, y: 0, w: 100, h: 10 };
    match h.request(denoise_request(&dir, "bad.png", Some(bad)), T) {
        Err(AiError::Remote { code, .. }) => assert_eq!(code, "image"),
        other => panic!("expected an image error, got {other:?}"),
    }
    assert!(!dir.join("bad.png").exists());
    h.shutdown();
}

#[test]
fn a_cancel_stops_the_job_between_tiles_writes_nothing_and_the_helper_carries_on() {
    let (dir, _) = denoise_fixture("cancel", 2000, 1100); // 15 tiles
    let h = AiHelper::new(helper_exe(), 0);
    let id = h.reserve_id();
    let mut cancelled_at = None;
    let mut last = Progress { done: 0, total: 0 };
    let r = h.request_with_id(id, denoise_request(&dir, "out.png", None), T, |p| {
        last = p;
        if p.done == 1 && cancelled_at.is_none() {
            cancelled_at = Some(p.done);
            h.cancel(id).expect("cancel is answered at once");
        }
    });
    match r {
        Err(AiError::Remote { code, .. }) => assert_eq!(code, "cancelled"),
        other => panic!("expected a cancelled error, got {other:?}"),
    }
    assert!(last.done < last.total, "the job must stop before the end: {last:?}");
    assert!(!dir.join("out.png").exists() && !dir.join("out.png.part").exists(), "a cancelled job leaves no file");
    hello_pid(&h); // still alive and serving
    // A cancel that names nothing running is harmless.
    h.cancel(id).unwrap();
    h.shutdown();
}

#[test]
fn a_request_that_arrives_during_a_job_waits_its_turn() {
    let (dir, _) = denoise_fixture("queue", 2000, 1100);
    let h = std::sync::Arc::new(AiHelper::new(helper_exe(), 0));
    let started = std::sync::Arc::new(std::sync::Barrier::new(2));
    let job = {
        let (h, started, dir) = (h.clone(), started.clone(), dir.clone());
        let mut first = true;
        std::thread::spawn(move || {
            h.request_with_progress(denoise_request(&dir, "out.png", None), T, |_| {
                if first {
                    first = false;
                    started.wait();
                }
            })
        })
    };
    started.wait(); // the job is running (first tile done)
    hello_pid(&h); // served only after the job: so its result file already exists
    assert!(dir.join("out.png").exists(), "hello was answered before the running job finished");
    assert!(job.join().unwrap().is_ok());
    h.shutdown();
}

#[test]
fn a_missing_model_or_image_is_a_clean_denoise_error() {
    let dir = scratch("missing");
    let h = AiHelper::new(helper_exe(), 0);
    let r = h.request(Request::DenoiseRun { model: dir.join("none.onnx"), image_path: dir.join("none.png"), out_path: dir.join("o.png"), region: None }, T);
    match r {
        Err(AiError::Remote { code, .. }) => assert_eq!(code, "image"),
        other => panic!("expected an image error, got {other:?}"),
    }
    hello_pid(&h);
    h.shutdown();
}
