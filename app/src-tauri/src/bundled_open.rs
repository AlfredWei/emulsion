//! Settings > About opens the app's own licence files in the OS's default app. That OS call can block for good: on
//! Windows with no app registered for `.md`, `ShellExecuteExW` shows "Pick an app" and later calls never return. A
//! blocked thread cannot be cancelled, so the bound comes from refusing to start another one. Each file has one slot.
//! While an open is pending, a second click starts nothing and the first click reports the outcome. Once an open has
//! outlived the timeout, the click that timed out reports it, and every later click reports it at once instead of
//! starting a thread that would be stuck too. A slot frees when its thread returns, so a stuck open that finishes
//! late (the user answers the dialog) makes the file openable again.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub const OPEN_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BundledFile {
    Notices,
    Libraries,
}

impl BundledFile {
    const COUNT: usize = 2;

    pub fn file_name(self) -> &'static str {
        match self {
            BundledFile::Notices => "THIRD_PARTY_NOTICES.md",
            BundledFile::Libraries => "THIRD_PARTY_LIBRARIES.md",
        }
    }

    fn slot(self) -> usize {
        self as usize
    }
}

/// When the open now holding each slot started, or `None` when the slot is free.
type Slots = [Option<Instant>; BundledFile::COUNT];

fn lock(slots: &Mutex<Slots>) -> MutexGuard<'_, Slots> {
    slots.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug)]
pub enum Begin {
    /// The slot was free. The open may run, and it holds the slot until the `Pending` drops.
    Started(Pending),
    /// An open of this file started less than the timeout ago. Its own click reports how it ends.
    InProgress,
    /// An open of this file started more than the timeout ago and has not returned.
    Stuck,
}

/// Holds one file's slot. It moves into the thread that calls the OS, so the slot frees when that call returns,
/// panics included.
#[derive(Debug)]
pub struct Pending {
    slots: Arc<Mutex<Slots>>,
    file: BundledFile,
}

impl Drop for Pending {
    fn drop(&mut self) {
        lock(&self.slots)[self.file.slot()] = None;
    }
}

pub struct OpenGuard {
    timeout: Duration,
    slots: Arc<Mutex<Slots>>,
}

impl Default for OpenGuard {
    fn default() -> Self {
        Self::new(OPEN_TIMEOUT)
    }
}

impl OpenGuard {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout, slots: Arc::new(Mutex::new([None; BundledFile::COUNT])) }
    }

    pub fn begin(&self, file: BundledFile) -> Begin {
        self.begin_at(file, Instant::now())
    }

    fn begin_at(&self, file: BundledFile, now: Instant) -> Begin {
        let mut slots = lock(&self.slots);
        match slots[file.slot()] {
            None => {
                slots[file.slot()] = Some(now);
                Begin::Started(Pending { slots: Arc::clone(&self.slots), file })
            }
            Some(started) if now.saturating_duration_since(started) < self.timeout => Begin::InProgress,
            Some(_) => Begin::Stuck,
        }
    }
}

fn manual_open_hint(path: &Path) -> String {
    format!("You can open the file yourself: {}", path.display())
}

fn timed_out_message(file: BundledFile, path: &Path, timeout: Duration) -> String {
    format!(
        "{} did not open within {} seconds. No app may be set for Markdown files on this system. {}",
        file.file_name(),
        timeout.as_secs(),
        manual_open_hint(path)
    )
}

fn stuck_message(file: BundledFile, path: &Path) -> String {
    format!(
        "{} is still waiting to open. If a window is asking which app to use, answer it or close it. {}",
        file.file_name(),
        manual_open_hint(path)
    )
}

/// Runs `opener` on the blocking pool for `file`, at most one pending open per file, and gives up waiting after the
/// guard's timeout. Giving up frees the caller, not the thread: the thread keeps its slot until `opener` returns.
pub async fn open<F>(guard: &OpenGuard, file: BundledFile, path: Option<PathBuf>, opener: F) -> Result<(), String>
where
    F: FnOnce(&Path) -> Result<(), String> + Send + 'static,
{
    let path = path.ok_or_else(|| format!("{} was not found next to the app", file.file_name()))?;
    let pending = match guard.begin(file) {
        Begin::Started(pending) => pending,
        Begin::InProgress => return Ok(()),
        Begin::Stuck => return Err(stuck_message(file, &path)),
    };
    let thread_path = path.clone();
    let work = tokio::task::spawn_blocking(move || {
        let _pending = pending;
        opener(&thread_path)
    });
    match tokio::time::timeout(guard.timeout, work).await {
        Ok(joined) => joined.map_err(|e| e.to_string())?,
        Err(_) => Err(timed_out_message(file, &path, guard.timeout)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    const SHORT: Duration = Duration::from_millis(80);

    fn path() -> Option<PathBuf> {
        Some(PathBuf::from("licences").join("THIRD_PARTY_NOTICES.md"))
    }

    fn slot_is_free(guard: &OpenGuard, file: BundledFile) -> bool {
        lock(&guard.slots)[file.slot()].is_none()
    }

    async fn wait_until_free(guard: &OpenGuard, file: BundledFile) {
        for _ in 0..200 {
            if slot_is_free(guard, file) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the slot never freed");
    }

    /// An opener that signals when it starts and then blocks until released, as a stuck OS call does.
    fn blocking_opener(calls: &Arc<AtomicUsize>) -> (impl FnOnce(&Path) -> Result<(), String> + Send + 'static, mpsc::Receiver<()>, mpsc::Sender<()>) {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let calls = Arc::clone(calls);
        let opener = move |_: &Path| {
            calls.fetch_add(1, Ordering::SeqCst);
            started_tx.send(()).unwrap();
            let _ = release_rx.recv();
            Ok(())
        };
        (opener, started_rx, release_tx)
    }

    fn counting_opener(calls: &Arc<AtomicUsize>) -> impl FnOnce(&Path) -> Result<(), String> + Send + 'static {
        let calls = Arc::clone(calls);
        move |_: &Path| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn a_free_slot_starts_and_a_second_begin_on_it_is_in_progress() {
        let guard = OpenGuard::new(SHORT);
        let now = Instant::now();
        let first = guard.begin_at(BundledFile::Notices, now);
        assert!(matches!(first, Begin::Started(_)));
        assert!(matches!(guard.begin_at(BundledFile::Notices, now), Begin::InProgress));
    }

    #[test]
    fn the_two_files_have_separate_slots() {
        let guard = OpenGuard::new(SHORT);
        let now = Instant::now();
        let _notices = guard.begin_at(BundledFile::Notices, now);
        assert!(matches!(guard.begin_at(BundledFile::Libraries, now), Begin::Started(_)));
    }

    #[test]
    fn an_open_older_than_the_timeout_is_stuck() {
        let guard = OpenGuard::new(SHORT);
        let start = Instant::now();
        let _held = guard.begin_at(BundledFile::Notices, start);
        assert!(matches!(guard.begin_at(BundledFile::Notices, start + SHORT - Duration::from_millis(1)), Begin::InProgress));
        assert!(matches!(guard.begin_at(BundledFile::Notices, start + SHORT), Begin::Stuck));
        assert!(matches!(guard.begin_at(BundledFile::Notices, start + SHORT * 100), Begin::Stuck));
    }

    #[test]
    fn dropping_the_pending_frees_the_slot_even_after_it_was_stuck() {
        let guard = OpenGuard::new(SHORT);
        let start = Instant::now();
        let held = guard.begin_at(BundledFile::Notices, start);
        assert!(matches!(guard.begin_at(BundledFile::Notices, start + SHORT), Begin::Stuck));
        drop(held);
        assert!(matches!(guard.begin_at(BundledFile::Notices, start + SHORT), Begin::Started(_)));
    }

    #[test]
    fn a_poisoned_lock_does_not_block_opens() {
        let guard = OpenGuard::new(SHORT);
        let slots = Arc::clone(&guard.slots);
        let _ = std::thread::spawn(move || {
            let _held = slots.lock().unwrap();
            panic!("poison the lock");
        })
        .join();
        assert!(matches!(guard.begin(BundledFile::Notices), Begin::Started(_)));
    }

    #[tokio::test]
    async fn a_quick_open_succeeds_and_frees_the_slot() {
        let guard = OpenGuard::new(Duration::from_secs(5));
        let calls = Arc::new(AtomicUsize::new(0));
        open(&guard, BundledFile::Notices, path(), counting_opener(&calls)).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        wait_until_free(&guard, BundledFile::Notices).await;
        open(&guard, BundledFile::Notices, path(), counting_opener(&calls)).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_missing_file_is_reported_without_taking_the_slot() {
        let guard = OpenGuard::new(SHORT);
        let calls = Arc::new(AtomicUsize::new(0));
        let err = open(&guard, BundledFile::Libraries, None, counting_opener(&calls)).await.unwrap_err();
        assert_eq!(err, "THIRD_PARTY_LIBRARIES.md was not found next to the app");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(slot_is_free(&guard, BundledFile::Libraries));
    }

    #[tokio::test]
    async fn the_openers_own_error_is_passed_through_and_frees_the_slot() {
        let guard = OpenGuard::new(Duration::from_secs(5));
        let err = open(&guard, BundledFile::Notices, path(), |_| Err("no application is associated".to_string())).await.unwrap_err();
        assert_eq!(err, "no application is associated");
        wait_until_free(&guard, BundledFile::Notices).await;
    }

    #[tokio::test]
    async fn a_panicking_opener_is_an_error_and_frees_the_slot() {
        let guard = OpenGuard::new(Duration::from_secs(5));
        let err = open(&guard, BundledFile::Notices, path(), |_| panic!("shell exploded")).await.unwrap_err();
        assert!(!err.is_empty());
        wait_until_free(&guard, BundledFile::Notices).await;
    }

    #[tokio::test]
    async fn a_stuck_open_times_out_with_the_path_and_later_clicks_start_no_thread() {
        let guard = OpenGuard::new(SHORT);
        let calls = Arc::new(AtomicUsize::new(0));
        let (opener, started, release) = blocking_opener(&calls);

        let err = open(&guard, BundledFile::Notices, path(), opener).await.unwrap_err();
        started.recv().unwrap();
        assert!(err.contains("THIRD_PARTY_NOTICES.md did not open within"), "{err}");
        assert!(err.contains(&PathBuf::from("licences").join("THIRD_PARTY_NOTICES.md").display().to_string()), "{err}");

        for _ in 0..5 {
            let again = open(&guard, BundledFile::Notices, path(), counting_opener(&calls)).await.unwrap_err();
            assert!(again.contains("still waiting to open"), "{again}");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "no click after the first reached the OS");

        let other = open(&guard, BundledFile::Libraries, path(), counting_opener(&calls)).await;
        assert!(other.is_ok(), "the other file is not held by the stuck one");

        release.send(()).unwrap();
        wait_until_free(&guard, BundledFile::Notices).await;
        open(&guard, BundledFile::Notices, path(), counting_opener(&calls)).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 3, "the file opens again once the stuck call returned");
    }

    #[tokio::test]
    async fn a_second_click_while_the_first_is_pending_returns_at_once_and_the_first_reports() {
        let guard = Arc::new(OpenGuard::new(Duration::from_secs(30)));
        let calls = Arc::new(AtomicUsize::new(0));
        let (opener, started, release) = blocking_opener(&calls);

        let first = {
            let guard = Arc::clone(&guard);
            tokio::spawn(async move { open(&guard, BundledFile::Notices, path(), opener).await })
        };
        tokio::task::spawn_blocking(move || started.recv().unwrap()).await.unwrap();

        let second = tokio::time::timeout(
            Duration::from_secs(2),
            open(&guard, BundledFile::Notices, path(), counting_opener(&calls)),
        )
        .await
        .expect("the second click must not wait for the first");
        assert!(second.is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        release.send(()).unwrap();
        assert!(first.await.unwrap().is_ok());
    }
}
