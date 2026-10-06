//! Main-app side of the AI helper (RFC-0026 §3.1, revised 2026-10-06): spawns
//! `emulsion-ai` lazily on the first request, keeps the pipe open for the
//! life of the app, and respawns it when it is gone (it exits by itself after
//! an idle period, to return its memory; it can also crash).
//!
//! Failure policy: a request that finds the helper dead (EOF, broken pipe) is
//! retried **once** on a fresh process (every request is idempotent); a
//! request that times out kills the helper (it is hung) and fails; an error
//! the helper itself reports (`not_prepared`, `inference`, ...) is returned as
//! is. Concurrent callers are fine: each waits on its own channel and the
//! helper serves them in order.

use super::protocol::{Request, Response};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Default helper idle lifetime (seconds); `EMULSION_AI_IDLE_SECS` overrides it, 0 = never exit.
pub const DEFAULT_IDLE_SECS: u64 = 900;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiError {
    /// The helper binary could not be started.
    Spawn(String),
    /// The helper died or closed the pipe before answering (after one retry).
    HelperExited,
    /// No answer in time; the helper was killed.
    Timeout,
    /// The helper answered with an error.
    Remote { code: String, message: String },
    /// The helper sent something that is not a valid response.
    Protocol(String),
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiError::Spawn(m) => write!(f, "could not start the AI helper: {m}"),
            AiError::HelperExited => write!(f, "the AI helper stopped unexpectedly"),
            AiError::Timeout => write!(f, "the AI helper did not answer in time"),
            AiError::Remote { code, message } => write!(f, "{code}: {message}"),
            AiError::Protocol(m) => write!(f, "bad answer from the AI helper: {m}"),
        }
    }
}

impl std::error::Error for AiError {}

type Pending = Arc<Mutex<HashMap<u64, Sender<Response>>>>;

struct Conn {
    child: Arc<Mutex<Child>>,
    stdin: Mutex<ChildStdin>,
    pending: Pending,
    alive: Arc<AtomicBool>,
}

impl Conn {
    fn spawn(exe: &Path, idle_secs: u64) -> Result<Arc<Conn>, AiError> {
        let mut cmd = Command::new(exe);
        cmd.arg("--idle-secs").arg(idle_secs.to_string()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = cmd.spawn().map_err(|e| AiError::Spawn(format!("{}: {e}", exe.display())))?;
        let stdin = child.stdin.take().ok_or_else(|| AiError::Spawn("no stdin pipe".into()))?;
        let stdout = child.stdout.take().ok_or_else(|| AiError::Spawn("no stdout pipe".into()))?;
        let child = Arc::new(Mutex::new(child));
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));
        {
            let (pending, alive, child) = (pending.clone(), alive.clone(), child.clone());
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if let Ok(resp) = serde_json::from_str::<Response>(&line) {
                        if let Some(tx) = pending.lock().ok().and_then(|mut p| p.remove(&resp.id)) {
                            let _ = tx.send(resp);
                        }
                    }
                }
                // EOF: the helper is gone. Dropping the senders wakes every waiter with "disconnected".
                alive.store(false, Ordering::SeqCst);
                if let Ok(mut p) = pending.lock() {
                    p.clear();
                }
                // Reap it, so an idle exit or a crash does not leave a zombie until the app quits.
                if let Ok(mut c) = child.lock() {
                    let _ = c.wait();
                }
            });
        }
        Ok(Arc::new(Conn { child, stdin: Mutex::new(stdin), pending, alive }))
    }

    fn round_trip(&self, id: u64, req: &Request, timeout: Duration) -> Result<serde_json::Value, AiError> {
        let (tx, rx) = channel();
        self.pending.lock().map_err(|_| AiError::HelperExited)?.insert(id, tx);
        let line = serde_json::to_string(&super::protocol::Frame { id, request: req.clone() }).map_err(|e| AiError::Protocol(e.to_string()))?;
        let written = {
            let mut stdin = self.stdin.lock().map_err(|_| AiError::HelperExited)?;
            writeln!(stdin, "{line}").and_then(|_| stdin.flush())
        };
        if written.is_err() {
            self.forget(id);
            return Err(AiError::HelperExited);
        }
        match rx.recv_timeout(timeout) {
            Ok(resp) => match (resp.ok, resp.err) {
                (_, Some(e)) => Err(AiError::Remote { code: e.code, message: e.message }),
                (Some(v), None) => Ok(v),
                (None, None) => Err(AiError::Protocol("response has neither ok nor err".into())),
            },
            Err(RecvTimeoutError::Timeout) => {
                self.forget(id);
                Err(AiError::Timeout)
            }
            Err(RecvTimeoutError::Disconnected) => Err(AiError::HelperExited),
        }
    }

    fn forget(&self, id: u64) {
        if let Ok(mut p) = self.pending.lock() {
            p.remove(&id);
        }
    }

    fn kill(&self) {
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.alive.store(false, Ordering::SeqCst);
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        self.kill();
    }
}

pub struct AiHelper {
    exe: PathBuf,
    idle_secs: u64,
    next_id: AtomicU64,
    conn: Mutex<Option<Arc<Conn>>>,
}

impl AiHelper {
    pub fn new(exe: PathBuf, idle_secs: u64) -> Self {
        AiHelper { exe, idle_secs, next_id: AtomicU64::new(1), conn: Mutex::new(None) }
    }

    /// The helper binary sits next to the app's own executable (`cargo build`
    /// puts both in `target/<profile>/`; a bundle places the sidecar beside the app binary).
    pub fn default_exe() -> PathBuf {
        let name = if cfg!(windows) { "emulsion-ai.exe" } else { "emulsion-ai" };
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(name))).unwrap_or_else(|| PathBuf::from(name))
    }

    /// Idle lifetime from `EMULSION_AI_IDLE_SECS`, else the default.
    pub fn idle_secs_from_env() -> u64 {
        std::env::var("EMULSION_AI_IDLE_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_IDLE_SECS)
    }

    fn connection(&self) -> Result<Arc<Conn>, AiError> {
        let mut slot = self.conn.lock().map_err(|_| AiError::HelperExited)?;
        if let Some(c) = slot.as_ref() {
            if c.alive.load(Ordering::SeqCst) {
                return Ok(c.clone());
            }
        }
        let c = Conn::spawn(&self.exe, self.idle_secs)?;
        *slot = Some(c.clone());
        Ok(c)
    }

    fn discard(&self, conn: &Arc<Conn>) {
        if let Ok(mut slot) = self.conn.lock() {
            if slot.as_ref().is_some_and(|c| Arc::ptr_eq(c, conn)) {
                *slot = None;
            }
        }
    }

    /// Sends `req`, spawning the helper if needed. See the module doc for the failure policy.
    pub fn request(&self, req: Request, timeout: Duration) -> Result<serde_json::Value, AiError> {
        let mut last = AiError::HelperExited;
        for _attempt in 0..2 {
            let conn = self.connection()?;
            let id = self.next_id.fetch_add(1, Ordering::SeqCst);
            match conn.round_trip(id, &req, timeout) {
                Err(AiError::HelperExited) => {
                    self.discard(&conn);
                    last = AiError::HelperExited;
                }
                Err(AiError::Timeout) => {
                    conn.kill();
                    self.discard(&conn);
                    return Err(AiError::Timeout);
                }
                other => return other,
            }
        }
        Err(last)
    }

    /// Like `request`, but never spawns: `Ok(None)` when no helper is running
    /// (used for "release", which must not start a 1 GB process just to free it).
    pub fn request_if_running(&self, req: Request, timeout: Duration) -> Result<Option<serde_json::Value>, AiError> {
        let running = self.conn.lock().map(|s| s.as_ref().is_some_and(|c| c.alive.load(Ordering::SeqCst))).unwrap_or(false);
        if !running {
            return Ok(None);
        }
        self.request(req, timeout).map(Some)
    }

    /// Whether a helper process is currently alive (diagnostics and tests).
    pub fn is_running(&self) -> bool {
        self.conn.lock().map(|s| s.as_ref().is_some_and(|c| c.alive.load(Ordering::SeqCst))).unwrap_or(false)
    }

    #[allow(dead_code)] // the integration test and a future "AI memory" setting
    /// Asks the helper to exit and forgets it. Best effort; the process also
    /// exits on its own when this process closes the pipe.
    pub fn shutdown(&self) {
        let _ = self.request_if_running(Request::Shutdown, Duration::from_secs(2));
        if let Ok(mut slot) = self.conn.lock() {
            *slot = None;
        }
    }
}
