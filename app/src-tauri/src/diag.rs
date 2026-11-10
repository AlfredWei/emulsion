//! Trace lines for diagnosing the macOS e2e hang (first `import_files` of a spec never returning, 90 s, see
//! PROGRESS.md). Compiled only into the `wdio-webdriver` build the e2e suite drives: a release or `make dev` build
//! prints nothing and runs no watchdog. The suite's wdio config forwards the app's stderr into the CI log
//! (`captureBackendLogs`), so these lines say whether the command reached the backend and which stage it sat in.

use std::sync::{Arc, Mutex};

use crate::catalog::Catalog;

/// One `[diag +<ms since the first line>ms] message` line on stderr.
#[cfg(feature = "wdio-webdriver")]
pub fn log(msg: impl AsRef<str>) {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    let ms = START.get_or_init(Instant::now).elapsed().as_millis();
    eprintln!("[diag +{ms}ms] {}", msg.as_ref());
}

#[cfg(not(feature = "wdio-webdriver"))]
pub fn log(_msg: impl AsRef<str>) {}

/// Runs `f`, logging when it began and how long it took.
pub fn timed<T>(label: &str, f: impl FnOnce() -> T) -> T {
    log(format!("{label}: begin"));
    #[cfg(feature = "wdio-webdriver")]
    let started = std::time::Instant::now();
    let out = f();
    #[cfg(feature = "wdio-webdriver")]
    log(format!("{label}: done in {} ms", started.elapsed().as_millis()));
    out
}

/// Every 2 s tries the catalog lock without waiting and logs how long it has been held, once it has been for 4 s or
/// more, so a stall behind that lock (the one every import step and every background pass takes) shows by name.
#[cfg(feature = "wdio-webdriver")]
pub fn watch_catalog_lock(catalog: Arc<Mutex<Catalog>>) {
    std::thread::spawn(move || {
        let mut held_since: Option<std::time::Instant> = None;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            match catalog.try_lock() {
                Ok(_) => {
                    if let Some(t) = held_since.take() {
                        log(format!("catalog lock free again after {} ms held", t.elapsed().as_millis()));
                    }
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    let t = *held_since.get_or_insert_with(std::time::Instant::now);
                    if t.elapsed().as_secs() >= 4 {
                        log(format!("catalog lock held for {} ms and counting", t.elapsed().as_millis()));
                    }
                }
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    log("catalog lock is poisoned");
                    return;
                }
            }
        }
    });
}

#[cfg(not(feature = "wdio-webdriver"))]
pub fn watch_catalog_lock(_catalog: Arc<Mutex<Catalog>>) {}
