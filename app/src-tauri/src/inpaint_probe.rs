//! RFC-0032 bake-off, stage 3: the PatchMatch baseline on the cases `mask_training/inpaint/prepare_cases.py` made.
//! Ignored test, run by hand (reads every `<dir>/<case>/crop.png` + `mask.png`, writes `patchmatch.png` beside them and
//! prints the time per case):
//!   INPAINT_CASES=/path/to/out/cases cargo test --release --lib inpaint_probe -- --ignored --nocapture
//! Not product code; the RFC quotes its output.

use crate::inpaint_patchmatch::{inpaint, Params};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

#[test]
#[ignore]
fn inpaint_probe_patchmatch() {
    let dir = PathBuf::from(std::env::var("INPAINT_CASES").expect("set INPAINT_CASES"));
    let mut cases: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.join("crop.png").exists()).collect();
    cases.sort();
    let next = AtomicUsize::new(0);
    let times = Mutex::new(Vec::<(String, f64)>::new());
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get()).saturating_sub(2).clamp(1, 8);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(case) = cases.get(i) else { break };
                let img = image::open(case.join("crop.png")).unwrap().to_rgb8();
                let mask = image::open(case.join("mask.png")).unwrap().to_luma8();
                let t = std::time::Instant::now();
                let out = inpaint(&img, &mask, &Params::default());
                let secs = t.elapsed().as_secs_f64();
                out.save(case.join("patchmatch.png")).unwrap();
                times.lock().unwrap().push((case.file_name().unwrap().to_string_lossy().into_owned(), secs));
            });
        }
    });
    let mut t = times.into_inner().unwrap();
    t.sort_by(|a, b| a.0.cmp(&b.0));
    let mut secs: Vec<f64> = t.iter().map(|x| x.1).collect();
    secs.sort_by(|a, b| a.total_cmp(b));
    for (name, s) in &t {
        println!("{name} {s:.2}s");
    }
    println!("patchmatch: {} cases on {workers} threads, median {:.2}s, max {:.2}s", secs.len(), secs[secs.len() / 2], secs[secs.len() - 1]);
}
