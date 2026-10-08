//! The AI helper process (RFC-0026 §3.1, revised 2026-10-06): the protocol the
//! main app and `emulsion-ai` speak (`protocol`, shared by `#[path]` with the
//! helper binary) and the main-app side that spawns and talks to it
//! (`supervisor`). Kept free of Tauri and catalog types so the integration test
//! can include this directory on its own.

pub mod protocol;
pub mod supervisor;
pub mod tiling;
