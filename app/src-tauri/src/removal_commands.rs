//! Tauri command and job for object removal (RFC-0032 §12, ADR-0010): rasterise the user's marks on the photograph's full
//! decode, give the `emulsion-ai` helper just the window of it the network will see, store the patch it answers with in
//! the catalog, and return the patch's id for the `remove` op the frontend writes. One job at a time.

use crate::ai::protocol::{InpaintResult, Request};
use crate::ai::inpaint_pipeline::plan_window;
use crate::ai::supervisor::AiHelper;
use crate::ai::protocol::Region;
use crate::catalog::{StoredMask, StoredPatch};
use crate::inpaint_models::{self, ENGINE};
use crate::preview_cache::{self, DevelopPreviewInfo};
use crate::removal::{mask_clusters, rasterise_mask, MaskShape};
use crate::removal_preview;
use crate::{resolve_cache_root, source_decode, AppState};
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, State};

/// Longest the helper may stay silent (a cold model load and one network pass take seconds).
const SILENCE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);
/// How far past a drawn mark the mask is grown by default (full-decode pixels): the halo and soft shadow of an object.
pub const DEFAULT_GROW_PX: u32 = 6;

/// The request id of the running job, 0 when none: what `removal_cancel` names, and the "one at a time" lock.
static RUNNING_JOB: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Serialize, PartialEq)]
pub struct RemoveResult {
    pub patch_id: i64,
    pub engine: &'static str,
    /// The patch's place in the photograph's full decode (`src_w` x `src_h`).
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub src_w: u32,
    pub src_h: u32,
    /// The network saw a reduced window, so the fill is softer than its surroundings (RFC-0032 §11).
    pub reduced: bool,
    pub ms: u64,
}

/// What one run produced, before the catalog gives the patch an id.
#[derive(Debug)]
pub struct Removed {
    pub patch: StoredPatch,
    /// The hole this patch filled, kept with it so the removal can be redone.
    pub hole: StoredMask,
    pub reduced: bool,
    pub ms: u64,
}

/// Set by `removal_cancel`, looked at between the separate things of one job (the helper stops the one it is running).
static CANCELLED: AtomicBool = AtomicBool::new(false);
/// More separate things than this in one job is a sign the marks were meant as several jobs.
pub const MAX_THINGS: usize = 12;

/// The photograph at `source`, the marks, and the helper: one patch for each separate thing the marks ask to remove
/// (`mask_clusters`), each filled from a window sized for it. `work` holds the two small PNGs the helper reads and the
/// one it writes; they are removed whatever happens.
pub fn run_removal(
    helper: &AiHelper,
    id: u64,
    model: &Path,
    source: &Path,
    shapes: &[MaskShape],
    grow_px: u32,
    work: &Path,
) -> Result<Vec<Removed>, String> {
    let decoded = source_decode::decode_preview(source).map_err(|e| e.to_string())?;
    let (src_w, src_h) = (decoded.width, decoded.height);
    let image = image::RgbImage::from_raw(src_w, src_h, decoded.rgb).ok_or_else(|| "decoded image has the wrong size".to_string())?;
    let mask = rasterise_mask(src_w, src_h, shapes, grow_px);
    let things = mask_clusters(&mask);
    if things.is_empty() {
        return Err("nothing is marked to remove".to_string());
    }
    if things.len() > MAX_THINGS {
        return Err(format!("{} separate things are marked; remove at most {MAX_THINGS} at a time", things.len()));
    }
    std::fs::create_dir_all(work).map_err(|e| format!("could not create {}: {e}", work.display()))?;
    let mut out = Vec::with_capacity(things.len());
    for (n, bbox) in things.into_iter().enumerate() {
        if CANCELLED.load(Ordering::SeqCst) {
            return Err("removal cancelled".to_string());
        }
        // The helper plans its own window from the picture it is given; giving it exactly the window makes that plan the
        // whole picture, and the 24 MP decode never has to be written out.
        let window = plan_window(bbox, src_w, src_h);
        let crop_mask = image::imageops::crop_imm(&mask, window.x, window.y, window.w, window.h).to_image();
        // The hole is kept as the thing's own bounding box: the window holds no other marked thing (`mask_clusters`
        // merges any that fall inside each other's window), so the box is all a redo needs to rebuild the window's mask.
        let hole = image::imageops::crop_imm(&mask, bbox.x, bbox.y, bbox.w, bbox.h).to_image();
        out.push(fill_window(helper, id, model, &image, window, crop_mask, hole_png(&hole, bbox)?, n, work)?);
    }
    Ok(out)
}

/// The stored form of a hole: the bounding-box crop of the mask as a PNG, with the box's corner.
fn hole_png(hole: &image::GrayImage, bbox: Region) -> Result<StoredMask, String> {
    let mut png = Vec::new();
    hole.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| format!("could not encode the hole: {e}"))?;
    Ok(StoredMask { x: bbox.x, y: bbox.y, png })
}

/// Fills one window: `crop_mask` (255 = remove) over the same window of `image`, through the helper. `n` only names the
/// work files, which are removed whatever happens.
#[allow(clippy::too_many_arguments)]
fn fill_window(
    helper: &AiHelper,
    id: u64,
    model: &Path,
    image: &image::RgbImage,
    window: Region,
    crop_mask: image::GrayImage,
    hole: StoredMask,
    n: usize,
    work: &Path,
) -> Result<Removed, String> {
    let (src_w, src_h) = image.dimensions();
    let crop = image::imageops::crop_imm(image, window.x, window.y, window.w, window.h).to_image();
    let (image_path, mask_path, out_path) = (work.join(format!("removal-{id}-{n}-image.png")), work.join(format!("removal-{id}-{n}-mask.png")), work.join(format!("removal-{id}-{n}-patch.png")));
    let result = (|| {
        crop.save(&image_path).map_err(|e| format!("could not write {}: {e}", image_path.display()))?;
        crop_mask.save(&mask_path).map_err(|e| format!("could not write {}: {e}", mask_path.display()))?;
        let v = helper
            .request_with_id(
                id,
                Request::InpaintRun { model: model.to_path_buf(), image_path: image_path.clone(), mask_path: mask_path.clone(), out_path: out_path.clone() },
                SILENCE_TIMEOUT,
                |_| {},
            )
            .map_err(|e| e.to_string())?;
        let r: InpaintResult = serde_json::from_value(v).map_err(|e| e.to_string())?;
        let png = std::fs::read(&out_path).map_err(|e| format!("could not read the patch: {e}"))?;
        Ok::<_, String>(Removed {
            // The helper saw the window as its whole picture, so its coordinates are relative to the window.
            patch: StoredPatch { id: 0, x: window.x + r.x, y: window.y + r.y, w: r.width, h: r.height, src_w, src_h, engine: ENGINE.to_string(), png },
            hole,
            reduced: r.reduced,
            ms: r.ms,
        })
    })();
    for p in [&image_path, &mask_path, &out_path] {
        let _ = std::fs::remove_file(p);
    }
    result
}

/// Runs a stored hole again on the photograph at `source`: the window the first run used (planned from the hole's box),
/// the hole placed in it, one pass of the model. Fails when the photograph no longer has the size the hole was made on.
pub fn redo_removal(helper: &AiHelper, id: u64, model: &Path, source: &Path, hole: &StoredMask, src_size: (u32, u32), work: &Path) -> Result<Removed, String> {
    let stored = image::load_from_memory_with_format(&hole.png, image::ImageFormat::Png).map_err(|e| format!("the stored hole cannot be read: {e}"))?.to_luma8();
    let decoded = source_decode::decode_preview(source).map_err(|e| e.to_string())?;
    if (decoded.width, decoded.height) != src_size {
        return Err(format!(
            "the photograph is {}x{} now and the removal was made on {}x{}: it no longer fits",
            decoded.width, decoded.height, src_size.0, src_size.1
        ));
    }
    let image = image::RgbImage::from_raw(decoded.width, decoded.height, decoded.rgb).ok_or_else(|| "decoded image has the wrong size".to_string())?;
    let bbox = Region { x: hole.x, y: hole.y, w: stored.width(), h: stored.height() };
    let window = plan_window(bbox, decoded.width, decoded.height);
    let mut crop_mask = image::GrayImage::new(window.w, window.h);
    image::imageops::replace(&mut crop_mask, &stored, i64::from(hole.x) - i64::from(window.x), i64::from(hole.y) - i64::from(window.y));
    std::fs::create_dir_all(work).map_err(|e| format!("could not create {}: {e}", work.display()))?;
    fill_window(helper, id, model, &image, window, crop_mask, hole.clone(), 0, work)
}

/// Removes what `shapes` mark (normalised to the photograph) from the photograph of `version_id`: runs the model once
/// for each separate thing marked, stores the patches, returns each one's place and id, top to bottom. The caller adds
/// them to the `remove` op.
///
/// Fails with `a removal is already running`, `the removal model is not installed`, `nothing is marked to remove`,
/// `removal cancelled` (after `removal_cancel`; nothing is stored), or the helper's own error.
#[tauri::command]
pub async fn remove_object(
    app: AppHandle,
    state: State<'_, AppState>,
    helper: State<'_, Arc<AiHelper>>,
    version_id: i64,
    shapes: Vec<MaskShape>,
    grow_px: Option<u32>,
) -> Result<Vec<RemoveResult>, String> {
    let root = resolve_cache_root(&app, &state.catalog)?;
    let models = root.join("inpaint_models");
    let work = root.join("removal_work");
    let source = state.catalog.lock().map_err(|e| e.to_string())?.get_version_source(version_id).map_err(|e| e.to_string())?.path;
    let catalog = state.catalog.clone();
    let helper = helper.inner().clone();
    let id = helper.reserve_id();
    if RUNNING_JOB.compare_exchange(0, id, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Err("a removal is already running".to_string());
    }
    CANCELLED.store(false, Ordering::SeqCst);
    let result = tauri::async_runtime::spawn_blocking(move || {
        // Shipped in the installer; make sure it is installed (a no-op when it already is).
        crate::bundled_models::seed_from_app(&app, &models, &crate::bundled_models::INPAINT);
        let model = inpaint_models::ready_path(&models).ok_or_else(|| "the removal model is not installed".to_string())?;
        let removed = run_removal(&helper, id, &model, Path::new(&source), &shapes, grow_px.unwrap_or(DEFAULT_GROW_PX), &work)?;
        // Stored together, only once every thing has been filled: a cancel or a failure part-way leaves nothing behind.
        let catalog = catalog.lock().map_err(|e| e.to_string())?;
        removed
            .into_iter()
            .map(|r| {
                let patch_id = catalog.add_removal_patch_with_mask(version_id, &r.patch, Some(&r.hole)).map_err(|e| e.to_string())?;
                let p = r.patch;
                Ok(RemoveResult { patch_id, engine: ENGINE, x: p.x, y: p.y, w: p.w, h: p.h, src_w: p.src_w, src_h: p.src_h, reduced: r.reduced, ms: r.ms })
            })
            .collect()
    })
    .await;
    RUNNING_JOB.store(0, Ordering::SeqCst);
    result.map_err(|e| e.to_string())?
}

/// Runs removal `patch_id` of `version_id` again from the hole stored with it, and stores the result as a **new** patch
/// (the old row stays: History can bring the old result back). The caller swaps the item in the `remove` op.
///
/// Fails with `a removal is already running`, `the removal model is not installed`, `this removal has no stored hole`
/// (made before holes were kept), `... no longer fits` (the photograph changed size), `removal cancelled`, or the helper's
/// own error.
#[tauri::command]
pub async fn regenerate_removal(
    app: AppHandle,
    state: State<'_, AppState>,
    helper: State<'_, Arc<AiHelper>>,
    version_id: i64,
    patch_id: i64,
) -> Result<RemoveResult, String> {
    let root = resolve_cache_root(&app, &state.catalog)?;
    let models = root.join("inpaint_models");
    let work = root.join("removal_work");
    let (source, hole, src_size) = {
        let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
        let hole = catalog.get_removal_mask(version_id, patch_id).map_err(|e| e.to_string())?.ok_or_else(|| "this removal has no stored hole".to_string())?;
        let old = catalog.get_removal_patches(version_id, &[patch_id]).map_err(|e| e.to_string())?.into_iter().next().ok_or_else(|| "this removal is unknown".to_string())?;
        (catalog.get_version_source(version_id).map_err(|e| e.to_string())?.path, hole, (old.src_w, old.src_h))
    };
    let catalog = state.catalog.clone();
    let helper = helper.inner().clone();
    let id = helper.reserve_id();
    if RUNNING_JOB.compare_exchange(0, id, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Err("a removal is already running".to_string());
    }
    CANCELLED.store(false, Ordering::SeqCst);
    let result = tauri::async_runtime::spawn_blocking(move || {
        crate::bundled_models::seed_from_app(&app, &models, &crate::bundled_models::INPAINT);
        let model = inpaint_models::ready_path(&models).ok_or_else(|| "the removal model is not installed".to_string())?;
        let r = redo_removal(&helper, id, &model, Path::new(&source), &hole, src_size, &work)?;
        let catalog = catalog.lock().map_err(|e| e.to_string())?;
        let patch_id = catalog.add_removal_patch_with_mask(version_id, &r.patch, Some(&r.hole)).map_err(|e| e.to_string())?;
        let p = r.patch;
        Ok(RemoveResult { patch_id, engine: ENGINE, x: p.x, y: p.y, w: p.w, h: p.h, src_w: p.src_w, src_h: p.src_h, reduced: r.reduced, ms: r.ms })
    })
    .await;
    RUNNING_JOB.store(0, Ordering::SeqCst);
    result.map_err(|e| e.to_string())?
}

/// One removal as the strip needs to judge it: the decode it was made on and whether it has a stored hole.
#[derive(Debug, Serialize, PartialEq)]
pub struct RemovalInfo {
    pub patch_id: i64,
    pub src_w: u32,
    pub src_h: u32,
    pub has_hole: bool,
}

/// What is known of each of `patch_ids` (removals of `version_id`); an id with no row is left out (the removal is missing).
#[tauri::command]
pub async fn removal_info(state: State<'_, AppState>, version_id: i64, patch_ids: Vec<i64>) -> Result<Vec<RemovalInfo>, String> {
    let rows = state.catalog.lock().map_err(|e| e.to_string())?.removals_info(version_id, &patch_ids).map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(|r| RemovalInfo { patch_id: r.id, src_w: r.src_w, src_h: r.src_h, has_hole: r.has_hole }).collect())
}

/// A stored hole as the frontend needs it to plan a Heal: its place in the photograph's full decode (`src_w` x `src_h`)
/// and the hole itself, a gray PNG of just its box (255 inside).
#[derive(Debug, Serialize, PartialEq)]
pub struct RemovalHole {
    pub x: u32,
    pub y: u32,
    pub src_w: u32,
    pub src_h: u32,
    pub png: String,
}

/// The hole stored with removal `patch_id` of `version_id`; None when it has none (made before holes were kept).
#[tauri::command]
pub async fn removal_hole(state: State<'_, AppState>, version_id: i64, patch_id: i64) -> Result<Option<RemovalHole>, String> {
    use base64::Engine;
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    let Some(hole) = catalog.get_removal_mask(version_id, patch_id).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let patch = catalog.get_removal_patches(version_id, &[patch_id]).map_err(|e| e.to_string())?.into_iter().next().ok_or_else(|| "this removal is unknown".to_string())?;
    Ok(Some(RemovalHole { x: hole.x, y: hole.y, src_w: patch.src_w, src_h: patch.src_h, png: base64::engine::general_purpose::STANDARD.encode(&hole.png) }))
}

/// Stops the running removal at its next step boundary (a single network pass cannot be interrupted, so the one running
/// finishes or stops at the helper's next checkpoint, and the rest are not started); nothing is stored.
#[tauri::command]
pub async fn removal_cancel(helper: State<'_, Arc<AiHelper>>) -> Result<(), String> {
    let id = RUNNING_JOB.load(Ordering::SeqCst);
    if id == 0 {
        return Ok(());
    }
    CANCELLED.store(true, Ordering::SeqCst);
    let helper = helper.inner().clone();
    tauri::async_runtime::spawn_blocking(move || helper.cancel(id).map_err(|e| e.to_string())).await.map_err(|e| e.to_string())?
}

/// Whether the removal model is installed (the installer carries it; a build or install without it has no Remove, and
/// the tool says so). Cheap: the check is a size and a hash of one file, done once per call.
#[tauri::command]
pub async fn removal_available(app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    let models = resolve_cache_root(&app, &state.catalog)?.join("inpaint_models");
    tauri::async_runtime::spawn_blocking(move || inpaint_models::ready_path(&models).is_some()).await.map_err(|e| e.to_string())
}

/// The id of the installed removal engine (a removal made with another one can be tried again with this one).
#[tauri::command]
pub async fn removal_engine() -> &'static str {
    ENGINE
}

/// The Develop canvas's source picture: the plain preview (`full` picks the 1:1 tier) with this photo's kept denoised
/// copy mixed in at `amount` (0 = none) and the removal patches `patch_ids` laid over it, as a PNG the canvas loads in
/// place of the plain preview (RFC-0032 §12.1, RFC-0027 §3.3). `null` means "use the plain preview": nothing to mix
/// and nothing to lay (no amount or no copy kept, no patch of this picture). Never runs a model.
#[tauri::command]
pub async fn get_develop_source_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    content_hash: String,
    amount: u32,
    full: bool,
    version_id: Option<i64>,
    patch_ids: Option<Vec<i64>>,
) -> Result<Option<DevelopPreviewInfo>, String> {
    let previews = crate::resolve_previews_dir(&app, &state.catalog)?;
    let denoise_dir = crate::resolve_denoise_dir(&app, &state.catalog)?;
    let catalog = state.catalog.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let patch_ids = patch_ids.unwrap_or_default();
        let patches = match version_id {
            Some(v) if !patch_ids.is_empty() => catalog.lock().map_err(|e| e.to_string())?.get_removal_patches(v, &patch_ids).map_err(|e| e.to_string())?,
            _ => Vec::new(),
        };
        let mixes = amount > 0 && crate::denoise_cache::entry_path(&denoise_dir, &content_hash, crate::denoise_models::MODEL_ID).exists();
        if !mixes && patches.is_empty() {
            return Ok(None);
        }
        let source = Path::new(&path);
        let base = if full {
            preview_cache::ensure_develop_full_preview(source, &previews, Some(&content_hash))
        } else {
            preview_cache::ensure_develop_preview(source, &previews, Some(&content_hash))
        }
        .map_err(|e| e.user_message())?;
        removal_preview::ensure_baked(&base, &content_hash, &denoise_dir, &previews, full, if mixes { amount } else { 0 }, &patches)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Needs the real model and the built helper: `INPAINT_MODEL=<lama_fp32.onnx> INPAINT_IMAGE=<photo> EMULSION_AI_EXE=<target/release/emulsion-ai>
    /// cargo test --release --lib removal_commands -- --ignored --nocapture`. Marks a stroke and a lasso on the photograph,
    /// runs the whole job (decode, mask, window, helper, patch), lays the patch in and saves before / after next to the photo's name.
    #[test]
    #[ignore]
    fn a_marked_object_is_removed_through_the_real_helper() {
        let model = PathBuf::from(std::env::var("INPAINT_MODEL").expect("INPAINT_MODEL"));
        let photo = PathBuf::from(std::env::var("INPAINT_IMAGE").expect("INPAINT_IMAGE"));
        let helper = AiHelper::new(PathBuf::from(std::env::var("EMULSION_AI_EXE").expect("EMULSION_AI_EXE")), 0);
        let work = std::env::temp_dir().join(format!("emulsion-removal-job-{}", std::process::id()));
        let shapes = vec![
            MaskShape::Stroke { points: vec![[0.30, 0.55], [0.38, 0.60]], radius: 0.012 },
            MaskShape::Polygon { points: vec![[0.60, 0.55], [0.68, 0.55], [0.68, 0.70], [0.60, 0.70]] },
        ];
        let removed = run_removal(&helper, helper.reserve_id(), &model, &photo, &shapes, DEFAULT_GROW_PX, &work).unwrap();
        for r in &removed {
            eprintln!("patch {}x{} at ({}, {}) in {}x{}, reduced {}, {} ms", r.patch.w, r.patch.h, r.patch.x, r.patch.y, r.patch.src_w, r.patch.src_h, r.reduced, r.ms);
        }
        assert_eq!(removed.len(), 2, "two separate things, two windows");
        assert!(std::fs::read_dir(&work).unwrap().next().is_none(), "the work files are removed");
        let decoded = source_decode::decode_preview(&photo).unwrap();
        let before = image::RgbImage::from_raw(decoded.width, decoded.height, decoded.rgb).unwrap();
        let mut after = before.clone();
        // A redo from the stored hole is the same job: the same patch, byte for byte, on this machine (RFC-0032 §10 measured
        // the engine; this is the path that reuses a stored hole).
        let again = removed
            .iter()
            .map(|r| redo_removal(&helper, helper.reserve_id(), &model, &photo, &r.hole, (r.patch.src_w, r.patch.src_h), &work).unwrap())
            .collect::<Vec<_>>();
        for (first, second) in removed.iter().zip(&again) {
            assert_eq!((first.patch.x, first.patch.y, first.patch.w, first.patch.h), (second.patch.x, second.patch.y, second.patch.w, second.patch.h), "same place");
            assert_eq!(first.patch.png, second.patch.png, "the redo is the same picture");
            assert_eq!(first.hole, second.hole);
        }
        let patches: Vec<StoredPatch> = removed.into_iter().map(|r| r.patch).collect();
        assert_eq!(crate::removal::apply_removals(&mut after, &patches), 2);
        assert_ne!(before.as_raw(), after.as_raw());
        let out = std::env::temp_dir().join("emulsion-removal-job-after.png");
        after.save(&out).unwrap();
        eprintln!("after: {}", out.display());
        let _ = std::fs::remove_dir_all(&work);
    }
}
