//! Tauri commands for AI denoise (M6 slice 3b, RFC-0027 §3.4): the model file, the cache, and the job that
//! runs in the `emulsion-ai` helper. Thin wrappers, as `segment_commands.rs`; no UI calls these yet (slice 3c).
//!
//! A job decodes the source at full size (the same decode the export render uses, so a result lines up with
//! it pixel for pixel), hands it to the helper as a temporary PNG, and either lands the result in the cache
//! (the whole photo) or leaves a one-off crop file (the preview of a region). One job at a time.

use crate::ai::protocol::{DenoiseResult, Progress, Region, Request};
use crate::ai::supervisor::AiHelper;
use crate::denoise_cache;
use crate::denoise_models::{self, ModelStatus, MODEL_ID};
use crate::source_decode;
use crate::{resolve_denoise_dir, AppState};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

/// Longest the helper may stay silent mid-job: a tile takes about a second, a cold model load a few.
const SILENCE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// The request id of the running job, 0 when none: what `denoise_cancel` names, and the "one at a time" lock.
static RUNNING_JOB: AtomicU64 = AtomicU64::new(0);

fn models_dir(app: &AppHandle, state: &State<'_, AppState>) -> Result<PathBuf, String> {
    Ok(crate::resolve_cache_root(app, &state.catalog)?.join("denoise_models"))
}

#[derive(Clone, Serialize)]
struct JobProgress {
    done: u32,
    total: u32,
}

#[derive(Serialize)]
pub struct DenoiseJobResult {
    /// The finished PNG: the cache entry for a whole photo, a crop file for a region.
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub tiles: u32,
    pub ms: u64,
}

#[derive(Serialize)]
pub struct DenoiseCacheInfo {
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
}

/// Whether the denoise model file is installed and valid.
#[tauri::command]
pub async fn denoise_model_status(app: AppHandle, state: State<'_, AppState>) -> Result<ModelStatus, String> {
    let dir = models_dir(&app, &state)?;
    tauri::async_runtime::spawn_blocking(move || denoise_models::status(&dir)).await.map_err(|e| e.to_string())
}

#[derive(Clone, Serialize)]
struct ModelProgress {
    downloaded: u64,
    total: u64,
}

/// Set by `denoise_cancel_download`, reset when a download starts (one download at a time, as for SAM 2).
static DOWNLOAD_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Stops a running `denoise_download_model` (it then fails with "download cancelled"). A no-op otherwise.
#[tauri::command]
pub fn denoise_cancel_download() {
    DOWNLOAD_CANCEL.store(true, Ordering::Relaxed);
}

/// Fetches the model file after the user consented (the UI's dialog, slice 3c). Emits
/// `"denoise-model-progress"` `{downloaded, total}` at most every ~250 ms. The file is verified against its
/// pinned SHA-256 before it is kept.
#[tauri::command]
pub async fn denoise_download_model(app: AppHandle, state: State<'_, AppState>) -> Result<ModelStatus, String> {
    let dir = models_dir(&app, &state)?;
    DOWNLOAD_CANCEL.store(false, Ordering::Relaxed);
    let mut last_emit = std::time::Instant::now() - std::time::Duration::from_secs(1);
    let progress_app = app.clone();
    denoise_models::ensure_model(&dir, &DOWNLOAD_CANCEL, move |downloaded, total| {
        if downloaded == total || last_emit.elapsed() >= std::time::Duration::from_millis(250) {
            last_emit = std::time::Instant::now();
            let _ = progress_app.emit("denoise-model-progress", ModelProgress { downloaded, total });
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || denoise_models::status(&dir)).await.map_err(|e| e.to_string())
}

/// The offline path: verify the user-chosen model file by name and checksum and copy it in.
#[tauri::command]
pub async fn denoise_import_model(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<ModelStatus, String> {
    let dir = models_dir(&app, &state)?;
    tauri::async_runtime::spawn_blocking(move || denoise_models::import_file(&dir, Path::new(&path)).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

/// The whole-photo result in the cache for this content hash, if there is one (any size: the caller compares
/// it with the photo's own, and a mismatch means "not denoised yet" exactly as the render treats it).
#[tauri::command]
pub async fn denoise_cache_info(app: AppHandle, state: State<'_, AppState>, content_hash: String) -> Result<Option<DenoiseCacheInfo>, String> {
    let dir = resolve_denoise_dir(&app, &state.catalog)?;
    tauri::async_runtime::spawn_blocking(move || {
        let path = denoise_cache::entry_path(&dir, &content_hash, MODEL_ID);
        let Ok((width, height)) = image::image_dimensions(&path) else { return None };
        Some(DenoiseCacheInfo { width, height, bytes: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) })
    })
    .await
    .map_err(|e| e.to_string())
}

/// Runs a denoise job and returns when it is done, stopped or failed. Emits `"denoise-progress"`
/// `{done, total}` after every tile. `region` (full-image pixels) denoises only that rectangle into a crop
/// file for the before/after preview; without it the whole photo is denoised into the cache.
///
/// Fails with `a denoise job is already running`, `the denoise model is not installed`, `denoise cancelled`
/// (after `denoise_cancel`; nothing is written), or the helper's own error.
#[tauri::command]
pub async fn denoise_run(
    app: AppHandle,
    state: State<'_, AppState>,
    helper: State<'_, Arc<AiHelper>>,
    path: String,
    content_hash: String,
    region: Option<Region>,
) -> Result<DenoiseJobResult, String> {
    let models = models_dir(&app, &state)?;
    let cache = resolve_denoise_dir(&app, &state.catalog)?;
    let helper = helper.inner().clone();
    let id = helper.reserve_id();
    if RUNNING_JOB.compare_exchange(0, id, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Err("a denoise job is already running".to_string());
    }
    let result = tauri::async_runtime::spawn_blocking(move || run_job(&app, &helper, id, &models, &cache, Path::new(&path), &content_hash, region)).await;
    RUNNING_JOB.store(0, Ordering::SeqCst);
    result.map_err(|e| e.to_string())?
}

#[allow(clippy::too_many_arguments)]
fn run_job(
    app: &AppHandle,
    helper: &Arc<AiHelper>,
    id: u64,
    models: &Path,
    cache: &Path,
    source: &Path,
    content_hash: &str,
    region: Option<Region>,
) -> Result<DenoiseJobResult, String> {
    let model = denoise_models::ready_path(models).ok_or_else(|| "the denoise model is not installed".to_string())?;
    std::fs::create_dir_all(cache).map_err(|e| format!("could not create {}: {e}", cache.display()))?;
    let decoded = source_decode::decode_preview(source).map_err(|e| e.to_string())?;
    let input = cache.join(format!("input-{id}.png"));
    let out = match region {
        None => denoise_cache::entry_path(cache, content_hash, MODEL_ID),
        Some(_) => cache.join(format!("crop-{content_hash}.png")),
    };
    let written = image::RgbImage::from_raw(decoded.width, decoded.height, decoded.rgb)
        .ok_or_else(|| "decoded image has the wrong size".to_string())
        .and_then(|img| write_input(&img, &input));
    let answer = written.and_then(|()| {
        helper
            .request_with_id(id, Request::DenoiseRun { model, image_path: input.clone(), out_path: out.clone(), region }, SILENCE_TIMEOUT, |p: Progress| {
                let _ = app.emit("denoise-progress", JobProgress { done: p.done, total: p.total });
            })
            .map_err(|e| match e {
                crate::ai::supervisor::AiError::Remote { code, .. } if code == "cancelled" => "denoise cancelled".to_string(),
                other => other.to_string(),
            })
    });
    let _ = std::fs::remove_file(&input);
    let value = answer?;
    let r: DenoiseResult = serde_json::from_value(value).map_err(|e| e.to_string())?;
    Ok(DenoiseJobResult { path: out.to_string_lossy().to_string(), width: r.width, height: r.height, tiles: r.tiles, ms: r.ms })
}

/// A fast PNG: this is a hand-off to the helper, deleted right after.
fn write_input(img: &image::RgbImage, path: &Path) -> Result<(), String> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ImageEncoder;
    let file = std::fs::File::create(path).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    PngEncoder::new_with_quality(std::io::BufWriter::new(file), CompressionType::Fast, FilterType::Sub)
        .write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgb8)
        .map_err(|e| e.to_string())
}

/// Stops the running job at its next tile (about a second). A no-op when none is running.
#[tauri::command]
pub async fn denoise_cancel(helper: State<'_, Arc<AiHelper>>) -> Result<(), String> {
    let id = RUNNING_JOB.load(Ordering::SeqCst);
    if id == 0 {
        return Ok(());
    }
    let helper = helper.inner().clone();
    tauri::async_runtime::spawn_blocking(move || helper.cancel(id).map_err(|e| e.to_string())).await.map_err(|e| e.to_string())?
}

/// Deletes this photo's cached results (any model) -- the "Remove denoised copy" action and the cleanup
/// when a photo is removed from the catalog.
#[tauri::command]
pub async fn denoise_remove_cache(app: AppHandle, state: State<'_, AppState>, content_hash: String) -> Result<(), String> {
    let dir = resolve_denoise_dir(&app, &state.catalog)?;
    tauri::async_runtime::spawn_blocking(move || denoise_cache::remove_photo(&dir, &content_hash)).await.map_err(|e| e.to_string())
}
