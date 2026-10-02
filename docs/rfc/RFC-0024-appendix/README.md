# RFC-0024 appendix

- `develop-open-trace.log` — raw timeline from the instrumented **debug** dev build (Rust unoptimised, webview real), 3 rounds of: enter Develop (module remount) → switch image inside Develop → back to Library. Each `== openDevelop` line resets the clock; times are ms since the start of that `openDevelop`. The instrumentation (a `perfMark()` helper and an auto-run in `+page.svelte`) was temporary and is not in the repo.
- Rust-side numbers come from two `#[ignore]` timing tests that *are* in the repo, run in release:
  `EMULSION_TEST_RAW_SAMPLE=<raw> cargo test --release --lib develop_open_timing_report -- --ignored --nocapture` (`preview_cache.rs`) and
  `cargo test --release --lib lens_db_load_timing_report -- --ignored --nocapture` (`lens_profile.rs`).
