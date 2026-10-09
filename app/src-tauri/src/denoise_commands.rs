//! Tauri commands for AI denoise (M6 slices 3b-3c, RFC-0027 §3.4): the model file, the cache, the job that
//! runs in the `emulsion-ai` helper, and the Develop preview blend. Thin wrappers, as `segment_commands.rs`.
//!
//! A job decodes the source at full size (the same decode the export render uses, so a result lines up with
//! it pixel for pixel), hands it to the helper as a temporary PNG, and either lands the result in the cache
//! (the whole photo) or leaves a one-off crop file (the preview of a region). One job at a time.

use crate::ai::protocol::{DenoiseResult, Progress, Region, Request};
use crate::ai::supervisor::AiHelper;
use crate::denoise_cache;
use crate::denoise_models::{self, ModelStatus, MODEL_ID};
use crate::denoise_preview;
use crate::preview_cache::{self, DevelopPreviewInfo};
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
    /// A region job only: the same rectangle of the undenoised photo (the "before" half of the preview), and
    /// the rectangle itself in full-image pixels.
    pub before_path: Option<String>,
    pub region: Option<Region>,
}

#[derive(Serialize)]
pub struct DenoiseCacheInfo {
    pub model: &'static str,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
}

/// Size of the crop preview (RFC-0027 §3.4): four 512 px tiles, a few seconds, enough texture to judge by.
const CROP_PREVIEW: (u32, u32) = (992, 736);

/// The rectangle a crop preview covers: `CROP_PREVIEW` (or the whole image when it is smaller) centred on
/// `centre` (0..1 of each axis) and slid back inside the image rather than clipped.
pub fn crop_region(width: u32, height: u32, centre: [f64; 2]) -> Region {
    let (w, h) = (CROP_PREVIEW.0.min(width), CROP_PREVIEW.1.min(height));
    let place = |len: u32, size: u32, c: f64| -> u32 {
        let c = if c.is_finite() { c.clamp(0.0, 1.0) } else { 0.5 };
        ((c * len as f64 - size as f64 / 2.0).round().max(0.0) as u32).min(len - size)
    };
    Region { x: place(width, w, centre[0]), y: place(height, h, centre[1]), w, h }
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
        Some(DenoiseCacheInfo { model: MODEL_ID, width, height, bytes: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) })
    })
    .await
    .map_err(|e| e.to_string())
}

/// Runs a denoise job and returns when it is done, stopped or failed. Emits `"denoise-progress"`
/// `{done, total}` after every tile. `crop_centre` (`[x, y]`, 0..1 of the photo) denoises only the rectangle
/// of `crop_region` around that point and writes it, with the same rectangle of the undenoised photo, as the
/// before/after preview files; without it the whole photo is denoised into the cache.
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
    crop_centre: Option<[f64; 2]>,
) -> Result<DenoiseJobResult, String> {
    let models = models_dir(&app, &state)?;
    let cache = resolve_denoise_dir(&app, &state.catalog)?;
    let previews = crate::resolve_previews_dir(&app, &state.catalog)?;
    let helper = helper.inner().clone();
    let id = helper.reserve_id();
    if RUNNING_JOB.compare_exchange(0, id, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Err("a denoise job is already running".to_string());
    }
    let result = tauri::async_runtime::spawn_blocking(move || run_job(&app, &helper, id, &models, &cache, &previews, Path::new(&path), &content_hash, crop_centre)).await;
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
    previews: &Path,
    source: &Path,
    content_hash: &str,
    crop_centre: Option<[f64; 2]>,
) -> Result<DenoiseJobResult, String> {
    let model = denoise_models::ready_path(models).ok_or_else(|| "the denoise model is not installed".to_string())?;
    std::fs::create_dir_all(cache).map_err(|e| format!("could not create {}: {e}", cache.display()))?;
    let decoded = source_decode::decode_preview(source).map_err(|e| e.to_string())?;
    let image = image::RgbImage::from_raw(decoded.width, decoded.height, decoded.rgb).ok_or_else(|| "decoded image has the wrong size".to_string())?;
    let input = cache.join(format!("input-{id}.png"));
    let region = crop_centre.map(|c| crop_region(image.width(), image.height(), c));
    let (out, before) = match region {
        None => (denoise_cache::entry_path(cache, content_hash, MODEL_ID), None),
        Some(r) => {
            // A new crop preview replaces the last; the before half is written now (it needs no model).
            // In the previews directory: the webview may load files from there, not from the denoise cache.
            std::fs::create_dir_all(previews).map_err(|e| format!("could not create {}: {e}", previews.display()))?;
            denoise_cache::remove_crops(previews, content_hash);
            let before = previews.join(format!("crop-{content_hash}-{id}.before.png"));
            let part = image::imageops::crop_imm(&image, r.x, r.y, r.w, r.h).to_image();
            write_png_fast(&part, &before)?;
            (previews.join(format!("crop-{content_hash}-{id}.after.png")), Some(before))
        }
    };
    let answer = write_png_fast(&image, &input).and_then(|()| {
        // The helper reads the whole decoded photo and crops it itself, so the region means the same there.
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
    let value = match answer {
        Ok(v) => v,
        Err(e) => {
            if let Some(b) = &before {
                let _ = std::fs::remove_file(b);
            }
            return Err(e);
        }
    };
    let r: DenoiseResult = serde_json::from_value(value).map_err(|e| e.to_string())?;
    Ok(DenoiseJobResult {
        path: out.to_string_lossy().to_string(),
        width: r.width,
        height: r.height,
        tiles: r.tiles,
        ms: r.ms,
        before_path: before.map(|b| b.to_string_lossy().to_string()),
        region,
    })
}

/// A fast-compressing PNG, for hand-offs, crop previews and preview blends: written once, read soon.
pub fn write_png_fast(img: &image::RgbImage, path: &Path) -> Result<(), String> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ImageEncoder;
    let file = std::fs::File::create(path).map_err(|e| format!("could not write {}: {e}", path.display()))?;
    PngEncoder::new_with_quality(std::io::BufWriter::new(file), CompressionType::Fast, FilterType::Sub)
        .write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgb8)
        .map_err(|e| e.to_string())
}

/// The Develop canvas's source image with this photo's cached denoised copy mixed in at `amount` (1..100),
/// as a PNG the canvas loads in place of the plain preview (`full` picks the 1:1 tier). `null` means "use the
/// plain preview": amount 0, nothing cached, or the cached copy is not of this picture. Never runs the model.
#[tauri::command]
pub async fn get_denoised_develop_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    content_hash: String,
    amount: u32,
    full: bool,
) -> Result<Option<DevelopPreviewInfo>, String> {
    let previews = crate::resolve_previews_dir(&app, &state.catalog)?;
    let cache = resolve_denoise_dir(&app, &state.catalog)?;
    tauri::async_runtime::spawn_blocking(move || {
        if amount == 0 || !denoise_cache::entry_path(&cache, &content_hash, MODEL_ID).exists() {
            return Ok(None);
        }
        let source = Path::new(&path);
        let base = if full {
            preview_cache::ensure_develop_full_preview(source, &previews, Some(&content_hash))
        } else {
            preview_cache::ensure_develop_preview(source, &previews, Some(&content_hash))
        }
        .map_err(|e| e.user_message())?;
        denoise_preview::ensure_blended(&base, &content_hash, &cache, &previews, full, amount)
    })
    .await
    .map_err(|e| e.to_string())?
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
    let previews = crate::resolve_previews_dir(&app, &state.catalog)?;
    tauri::async_runtime::spawn_blocking(move || {
        denoise_cache::remove_photo(&dir, &content_hash);
        // The crop previews and preview blends are derived files in the previews directory.
        denoise_cache::remove_derived(&previews, &content_hash);
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crop_preview_is_centred_clamped_inside_the_photo_and_whole_for_a_small_one() {
        // Centred on a big photo.
        assert_eq!(crop_region(6000, 4000, [0.5, 0.5]), Region { x: 3000 - 496, y: 2000 - 368, w: 992, h: 736 });
        // Pushed against an edge it slides back inside rather than shrinking.
        assert_eq!(crop_region(6000, 4000, [0.0, 1.0]), Region { x: 0, y: 4000 - 736, w: 992, h: 736 });
        assert_eq!(crop_region(6000, 4000, [1.0, 0.0]), Region { x: 6000 - 992, y: 0, w: 992, h: 736 });
        // Out-of-range and NaN centres fall back to the nearest valid place / the middle.
        assert_eq!(crop_region(6000, 4000, [7.0, -2.0]), crop_region(6000, 4000, [1.0, 0.0]));
        assert_eq!(crop_region(6000, 4000, [f64::NAN, 0.5]), crop_region(6000, 4000, [0.5, 0.5]));
        // A photo smaller than the preview is previewed whole.
        assert_eq!(crop_region(300, 200, [0.3, 0.9]), Region { x: 0, y: 0, w: 300, h: 200 });
        assert_eq!(crop_region(2000, 500, [0.5, 0.5]), Region { x: 504, y: 0, w: 992, h: 500 });
    }
}
