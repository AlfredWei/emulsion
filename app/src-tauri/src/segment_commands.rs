//! Tauri commands for click-to-select (M6 slice 2a, RFC-0026 §3.1/§3.5). Thin
//! wrappers: model files in `segment_models`, inference in the `emulsion-ai`
//! helper process (reached through `ai::supervisor`). No UI calls these yet
//! (slice 2c).

use crate::ai::protocol::{Candidate, ModelPaths, PrepareResult, Prompt, Request};
use crate::ai::supervisor::AiHelper;
use crate::segment_models::{self, ModelStatus};
use crate::{preview_cache, resolve_cache_root, AppState};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

fn models_dir(app: &AppHandle, state: &State<'_, AppState>) -> Result<PathBuf, String> {
    Ok(resolve_cache_root(app, &state.catalog)?.join("segment_models"))
}

#[derive(Clone, Serialize)]
struct ModelProgress {
    downloaded: u64,
    total: u64,
}

/// A cold encode on a slow machine can take several seconds; hung means far longer.
const PREPARE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);
const DECODE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const RELEASE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Whether the model files are installed and valid, and what a download costs.
#[tauri::command]
pub async fn segment_model_status(app: AppHandle, state: State<'_, AppState>) -> Result<ModelStatus, String> {
    let dir = models_dir(&app, &state)?;
    tauri::async_runtime::spawn_blocking(move || {
        // Models shipped in the installer are installed on first use (a no-op in builds without them).
        crate::bundled_models::seed_from_app(&app, &dir, &crate::bundled_models::SEGMENT);
        segment_models::status(&dir)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Set by `segment_cancel_download`, reset when a download starts. One download at a time is all the
/// dialog allows, so one flag is enough (same assumption as the face-detection cancel flag).
static DOWNLOAD_CANCEL: AtomicBool = AtomicBool::new(false);

/// Stops a running `segment_download_models` (it then fails with "download cancelled"). A no-op
/// when nothing is downloading.
#[tauri::command]
pub fn segment_cancel_download() {
    DOWNLOAD_CANCEL.store(true, Ordering::Relaxed);
}

/// Fetches the model files after the user consented (the UI's dialog, slice 2c).
/// Emits `"segment-model-progress"` `{downloaded, total}` at most every ~250 ms.
#[tauri::command]
pub async fn segment_download_models(app: AppHandle, state: State<'_, AppState>) -> Result<ModelStatus, String> {
    let dir = models_dir(&app, &state)?;
    DOWNLOAD_CANCEL.store(false, Ordering::Relaxed);
    let mut last_emit = std::time::Instant::now() - std::time::Duration::from_secs(1);
    let progress_app = app.clone();
    segment_models::ensure_models(&dir, &DOWNLOAD_CANCEL, move |downloaded, total| {
        if downloaded == total || last_emit.elapsed() >= std::time::Duration::from_millis(250) {
            last_emit = std::time::Instant::now();
            let _ = progress_app.emit("segment-model-progress", ModelProgress { downloaded, total });
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    let dir2 = dir.clone();
    tauri::async_runtime::spawn_blocking(move || segment_models::status(&dir2)).await.map_err(|e| e.to_string())
}

/// The offline path: verify user-chosen model files by name and checksum and copy them in.
#[tauri::command]
pub async fn segment_import_model_files(app: AppHandle, state: State<'_, AppState>, paths: Vec<String>) -> Result<ModelStatus, String> {
    let dir = models_dir(&app, &state)?;
    let files: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    tauri::async_runtime::spawn_blocking(move || segment_models::import_files(&dir, &files).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

/// Runs the encoder for this image (≈ 1-6 s) in the AI helper unless its
/// embedding is cached there. Uses the same unedited, uncropped Develop preview
/// the canvas shows, so the result's normalized coordinates are in the frame
/// every mask uses. The helper is spawned here on first use.
#[tauri::command]
pub async fn segment_prepare(
    app: AppHandle,
    state: State<'_, AppState>,
    helper: State<'_, Arc<AiHelper>>,
    path: String,
    content_hash: String,
) -> Result<PrepareResult, String> {
    let dir = models_dir(&app, &state)?;
    let previews_dir = resolve_cache_root(&app, &state.catalog)?.join("previews");
    let helper = helper.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let paths = segment_models::ready_paths(&dir).ok_or_else(|| "the click-select model is not installed".to_string())?;
        // By hash, not by re-reading the source: a cache hit is an exists() check (a RAW would otherwise be re-hashed).
        let preview = preview_cache::ensure_develop_preview_for_hash(std::path::Path::new(&path), &content_hash, &previews_dir)
            .map_err(|e| e.user_message())?;
        let v = helper
            .request(
                Request::SegmentPrepare { models: ModelPaths { encoder: paths.encoder, decoder: paths.decoder }, content_hash, image_path: PathBuf::from(preview.path) },
                PREPARE_TIMEOUT,
            )
            .map_err(|e| e.to_string())?;
        serde_json::from_value(v).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Decoder only (≈ 20-80 ms): the candidates for these prompts, best predicted
/// IoU first. `refine` is the previously chosen candidate's `logits_png`.
/// Fails with `not_prepared: ...` if the helper restarted since `segment_prepare`
/// (the UI then prepares again).
#[tauri::command]
pub async fn segment_decode(
    app: AppHandle,
    state: State<'_, AppState>,
    helper: State<'_, Arc<AiHelper>>,
    content_hash: String,
    prompts: Vec<Prompt>,
    refine: Option<String>,
) -> Result<Vec<Candidate>, String> {
    let dir = models_dir(&app, &state)?;
    let helper = helper.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let paths = segment_models::ready_paths(&dir).ok_or_else(|| "the click-select model is not installed".to_string())?;
        let v = helper
            .request(
                Request::SegmentDecode { models: ModelPaths { encoder: paths.encoder, decoder: paths.decoder }, content_hash, prompts, refine },
                DECODE_TIMEOUT,
            )
            .map_err(|e| e.to_string())?;
        serde_json::from_value(v).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Frees the helper's sessions and cached embedding (the tool was released /
/// another image opened). Never starts the helper just to release it.
#[tauri::command]
pub async fn segment_release(helper: State<'_, Arc<AiHelper>>) -> Result<(), String> {
    let helper = helper.inner().clone();
    tauri::async_runtime::spawn_blocking(move || helper.request_if_running(Request::Release, RELEASE_TIMEOUT).map(|_| ()).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub struct AiHelperInfo {
    pub path: String,
    /// Whether the helper binary is where the app looks for it (next to the app executable).
    pub exists: bool,
    pub running: bool,
}

/// Diagnostics: where the AI helper is expected, and whether it is currently alive.
/// Never starts it.
#[tauri::command]
pub fn ai_helper_info(helper: State<'_, Arc<AiHelper>>) -> AiHelperInfo {
    let path = AiHelper::default_exe();
    AiHelperInfo { exists: path.is_file(), path: path.to_string_lossy().to_string(), running: helper.is_running() }
}
