//! Tauri commands for click-to-select (M6 slice 2a, RFC-0026 §3.1/§3.5). Thin
//! wrappers: model files in `segment_models`, inference in `segment`. No UI
//! calls these yet (slice 2c).

use crate::segment::{Candidate, Prompt, Segmenter};
use crate::segment_models::{self, ModelStatus};
use crate::{preview_cache, resolve_cache_root, AppState};
use serde::Serialize;
use std::path::PathBuf;
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

#[derive(Serialize)]
pub struct PrepareInfo {
    /// Encoder time in ms; 0 when the embedding was already cached.
    pub encode_ms: u64,
    pub cached: bool,
}

/// Whether the model files are installed and valid, and what a download costs.
#[tauri::command]
pub async fn segment_model_status(app: AppHandle, state: State<'_, AppState>) -> Result<ModelStatus, String> {
    let dir = models_dir(&app, &state)?;
    tauri::async_runtime::spawn_blocking(move || segment_models::status(&dir)).await.map_err(|e| e.to_string())
}

/// Fetches the model files after the user consented (the UI's dialog, slice 2c).
/// Emits `"segment-model-progress"` `{downloaded, total}` at most every ~250 ms.
#[tauri::command]
pub async fn segment_download_models(app: AppHandle, state: State<'_, AppState>) -> Result<ModelStatus, String> {
    let dir = models_dir(&app, &state)?;
    let mut last_emit = std::time::Instant::now() - std::time::Duration::from_secs(1);
    let progress_app = app.clone();
    segment_models::ensure_models(&dir, move |downloaded, total| {
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

/// Runs the encoder for this image (≈ 1-6 s) unless its embedding is cached.
/// Uses the same unedited, uncropped Develop preview the canvas shows, so the
/// result's normalized coordinates are in the frame every mask uses.
#[tauri::command]
pub async fn segment_prepare(
    app: AppHandle,
    state: State<'_, AppState>,
    segmenter: State<'_, Arc<Segmenter>>,
    path: String,
    content_hash: String,
) -> Result<PrepareInfo, String> {
    let dir = models_dir(&app, &state)?;
    let previews_dir = resolve_cache_root(&app, &state.catalog)?.join("previews");
    let segmenter = segmenter.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let paths = segment_models::ready_paths(&dir).ok_or_else(|| crate::segment::SegmentError::ModelsMissing.to_string())?;
        if segmenter.has_embedding(&content_hash) {
            segmenter.prepare(&paths, &content_hash, &image::RgbImage::new(1, 1)).map_err(|e| e.to_string())?;
            return Ok(PrepareInfo { encode_ms: 0, cached: true });
        }
        let preview = preview_cache::ensure_develop_preview(std::path::Path::new(&path), &previews_dir, Some(&content_hash))
            .map_err(|e| e.user_message())?;
        let img = image::open(&preview.path).map_err(|e| e.to_string())?.to_rgb8();
        let ms = segmenter.prepare(&paths, &content_hash, &img).map_err(|e| e.to_string())?;
        Ok(PrepareInfo { encode_ms: ms as u64, cached: false })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Decoder only (≈ 20-80 ms): the candidates for these prompts, best predicted
/// IoU first. `refine` is the previously chosen candidate's `logits_png`.
#[tauri::command]
pub async fn segment_decode(
    app: AppHandle,
    state: State<'_, AppState>,
    segmenter: State<'_, Arc<Segmenter>>,
    content_hash: String,
    prompts: Vec<Prompt>,
    refine: Option<String>,
) -> Result<Vec<Candidate>, String> {
    let dir = models_dir(&app, &state)?;
    let segmenter = segmenter.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let paths = segment_models::ready_paths(&dir).ok_or_else(|| crate::segment::SegmentError::ModelsMissing.to_string())?;
        segmenter.decode(&paths, &content_hash, &prompts, refine.as_deref()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Frees the sessions and the cached embedding (the tool was released / another image opened).
#[tauri::command]
pub fn segment_release(segmenter: State<'_, Arc<Segmenter>>) {
    segmenter.release();
}
