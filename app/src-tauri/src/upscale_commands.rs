//! Tauri commands for AI enlargement (M6 slice 4d, RFC-0029): the model file, the plan the dialog shows, and the
//! job that renders the photo, enlarges it in the `emulsion-ai` helper, catalogs the result as a connected photo
//! and starts its thumbnail. Thin wrappers around `upscale_job.rs` (the render and the helper round trip) and the
//! catalog's `add_enlargement`, as `denoise_commands.rs` is around its own pieces. One job at a time.

use crate::ai::protocol::{Progress, Request};
use crate::ai::supervisor::{AiError, AiHelper};
use crate::catalog::{edit_stack_hash, Catalog, EnlargementInput};
use crate::upscale_job::{self, Enlarged, Plan, Spec, Target};
use crate::upscale_models::{self, ModelStatus, MODEL_ID};
use crate::{export, import, metadata_writer, preview_cache, resolve_cache_root, resolve_denoise_dir, resolve_previews_dir, resolve_thumbnail_dir, AppState};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

/// Longest the helper may stay silent mid-job (a tile takes about half a second; a cold model load a few).
const SILENCE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

/// The request id of the running job, 0 when none: what `upscale_cancel` names, and the "one at a time" lock.
static RUNNING_JOB: AtomicU64 = AtomicU64::new(0);
/// Milliseconds per tile measured by the last finished job on this computer (`f64` bits; 0 = none yet).
static MEASURED_MS_PER_TILE: AtomicU64 = AtomicU64::new(0);

fn models_dir(app: &AppHandle, state: &State<'_, AppState>) -> Result<PathBuf, String> {
    Ok(resolve_cache_root(app, &state.catalog)?.join("upscale_models"))
}

fn ms_per_tile() -> f64 {
    match f64::from_bits(MEASURED_MS_PER_TILE.load(Ordering::Relaxed)) {
        v if v.is_finite() && v > 0.0 => v,
        _ => upscale_job::DEFAULT_MS_PER_TILE,
    }
}

/// What the dialog asks for: `{ "kind": "scale", "value": 4 }` or `{ "kind": "long_edge", "value": 6000 }`.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TargetArg {
    Scale { value: u32 },
    LongEdge { value: u32 },
}

impl From<TargetArg> for Target {
    fn from(t: TargetArg) -> Target {
        match t {
            TargetArg::Scale { value } => Target::Scale(value),
            TargetArg::LongEdge { value } => Target::LongEdge(value),
        }
    }
}

#[derive(Clone, Copy, Serialize)]
struct JobProgress {
    done: u32,
    total: u32,
}

#[derive(Clone, Serialize)]
struct ModelProgress {
    downloaded: u64,
    total: u64,
}

#[derive(Serialize)]
pub struct UpscaleJobResult {
    pub image_id: i64,
    pub version_id: i64,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub ms: u64,
}

/// Whether the super-resolution model file is installed and valid.
#[tauri::command]
pub async fn upscale_model_status(app: AppHandle, state: State<'_, AppState>) -> Result<ModelStatus, String> {
    let dir = models_dir(&app, &state)?;
    tauri::async_runtime::spawn_blocking(move || upscale_models::status(&dir)).await.map_err(|e| e.to_string())
}

static DOWNLOAD_CANCEL: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn upscale_cancel_download() {
    DOWNLOAD_CANCEL.store(true, Ordering::Relaxed);
}

/// Fetches the model file after the user consented. Emits `"upscale-model-progress"` `{downloaded, total}` at
/// most every ~250 ms; the file is verified against its pinned SHA-256 before it is kept.
#[tauri::command]
pub async fn upscale_download_model(app: AppHandle, state: State<'_, AppState>) -> Result<ModelStatus, String> {
    let dir = models_dir(&app, &state)?;
    DOWNLOAD_CANCEL.store(false, Ordering::Relaxed);
    let mut last_emit = std::time::Instant::now() - std::time::Duration::from_secs(1);
    let progress_app = app.clone();
    upscale_models::ensure_model(&dir, &DOWNLOAD_CANCEL, move |downloaded, total| {
        if downloaded == total || last_emit.elapsed() >= std::time::Duration::from_millis(250) {
            last_emit = std::time::Instant::now();
            let _ = progress_app.emit("upscale-model-progress", ModelProgress { downloaded, total });
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || upscale_models::status(&dir)).await.map_err(|e| e.to_string())
}

/// The offline path: verify the user-chosen model file by name and checksum and copy it in.
#[tauri::command]
pub async fn upscale_import_model(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<ModelStatus, String> {
    let dir = models_dir(&app, &state)?;
    tauri::async_runtime::spawn_blocking(move || upscale_models::import_file(&dir, Path::new(&path)).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

/// The numbers the dialog shows before anything runs: output size, time on this computer, file size, the
/// very-large flag. Pure arithmetic over the catalog's recorded size and the photo's crop: nothing is rendered.
#[tauri::command]
pub fn upscale_plan(state: State<'_, AppState>, version_id: i64, target: TargetArg, jpeg_quality: u8) -> Result<Plan, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    plan_for_version(&catalog, version_id, target.into(), jpeg_quality, ms_per_tile())
}

fn plan_for_version(catalog: &Catalog, version_id: i64, target: Target, jpeg_quality: u8, ms_per_tile: f64) -> Result<Plan, String> {
    let source = catalog.get_version_source(version_id).map_err(|e| e.to_string())?;
    if catalog.is_enlargement(source.image_id).map_err(|e| e.to_string())? {
        return Err(ENLARGEMENT_OF_ENLARGEMENT.to_string());
    }
    let (w, h) = catalog.get_image_dimensions(source.image_id).map_err(|e| e.to_string())?.ok_or("the photo's size is not known")?;
    let stack = catalog.get_edit_stack(version_id).map_err(|e| e.to_string())?;
    let (cw, ch) = crate::develop_engine::cropped_dimensions(w, h, &stack);
    upscale_job::plan((cw, ch), target, jpeg_quality, ms_per_tile)
}

const ENLARGEMENT_OF_ENLARGEMENT: &str = "Enlarge (AI) is turned off on an enlargement, to avoid compounding invented detail";

/// The result's file stem: the user's name from the dialog without a `.jpg` / `.jpeg` extension, path
/// separators and control characters removed; `None` when nothing usable is left (the default name is used).
pub fn clean_stem(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    let base = if lower.ends_with(".jpeg") { &name[..name.len() - 5] } else if lower.ends_with(".jpg") { &name[..name.len() - 4] } else { name };
    let cleaned: String = base.chars().filter(|c| !c.is_control() && !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')).collect();
    let cleaned = cleaned.trim().trim_matches('.').trim().to_string();
    (!cleaned.is_empty()).then(|| cleaned.chars().take(120).collect())
}

/// "×4", "×2.5": the factor as the UI writes it (`lib/upscaleLinks.js` `scaleLabel`).
fn scale_label(scale: f64) -> String {
    let rounded = (scale * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 { format!("×{}", rounded as i64) } else { format!("×{rounded:.1}") }
}

/// The sentence the metadata check box writes into the JPEG's own EXIF (RFC-0029 §3.6).
pub fn enlargement_note(scale: f64, strength: u8) -> String {
    format!(
        "Enlarged {} with AI ({MODEL_ID}, strength {strength}%). The picture contains detail the camera did not capture.",
        scale_label(scale)
    )
}

/// What a finished job leaves to do: the optional note, the catalog row, and nothing on failure (the file is removed).
#[allow(clippy::too_many_arguments)]
fn register_result(
    catalog: &Catalog,
    version_id: i64,
    stack: &crate::catalog::EditStack,
    out_path: &Path,
    enlarged: &Enlarged,
    strength: u8,
    add_to_collections: bool,
    metadata_note: bool,
) -> Result<(i64, i64), String> {
    let result = (|| -> Result<(i64, i64), String> {
        let scale = enlarged.result.width.max(enlarged.result.height) as f64 / enlarged.source_size.0.max(enlarged.source_size.1) as f64;
        if metadata_note {
            let bytes = std::fs::read(out_path).map_err(|e| e.to_string())?;
            let noted = metadata_writer::add_enlargement_note(bytes, "Emulsion", &enlargement_note(scale, strength)).map_err(|e| e.to_string())?;
            let part = out_path.with_extension("jpg.noted");
            std::fs::write(&part, noted).map_err(|e| e.to_string())?;
            std::fs::rename(&part, out_path).map_err(|e| e.to_string())?;
        }
        let (content_hash, file_size) = upscale_job::fingerprint(out_path).map_err(|e| e.to_string())?;
        let image_id = catalog
            .add_enlargement(&EnlargementInput {
                source_version_id: version_id,
                path: out_path.to_string_lossy().to_string(),
                content_hash,
                file_size: file_size as i64,
                width: enlarged.result.width,
                height: enlarged.result.height,
                scale,
                strength,
                model_id: MODEL_ID.to_string(),
                source_stack_hash: edit_stack_hash(stack),
                add_to_collections,
            })
            .map_err(|e| e.to_string())?;
        let new_version = catalog.get_primary_version_id(image_id).map_err(|e| e.to_string())?;
        Ok((image_id, new_version))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(out_path);
    }
    result
}

/// Enlarges a photo and adds the result to the catalog as a connected photo (RFC-0029 §3.1). Emits
/// `"upscale-progress"` `{done, total}` after every tile. Fails with `an enlargement is already running`,
/// `the AI enlargement model is not installed`, `upscale cancelled` (nothing is written), or the helper's own error.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn upscale_run(
    app: AppHandle,
    state: State<'_, AppState>,
    helper: State<'_, Arc<AiHelper>>,
    version_id: i64,
    target: TargetArg,
    strength: u8,
    jpeg_quality: u8,
    file_name: Option<String>,
    add_to_collections: bool,
    metadata_note: bool,
) -> Result<UpscaleJobResult, String> {
    let models = models_dir(&app, &state)?;
    let denoise_dir = resolve_denoise_dir(&app, &state.catalog)?;
    let scratch = resolve_cache_root(&app, &state.catalog)?.join("enlarge_tmp");
    let previews = resolve_previews_dir(&app, &state.catalog)?;
    let thumbnails = resolve_thumbnail_dir(&app, &state.catalog)?;
    let catalog = state.catalog.clone();
    let helper = helper.inner().clone();
    let id = helper.reserve_id();
    if RUNNING_JOB.compare_exchange(0, id, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Err("an enlargement is already running".to_string());
    }
    let result = tauri::async_runtime::spawn_blocking({
        let (app, catalog) = (app.clone(), catalog.clone());
        move || {
            let emit = |p: JobProgress| {
                let _ = app.emit("upscale-progress", p);
            };
            run_job(&emit, &catalog, &helper, id, &models, &denoise_dir, &scratch, version_id, target.into(), strength, jpeg_quality, file_name.as_deref(), add_to_collections, metadata_note)
        }
    })
    .await;
    RUNNING_JOB.store(0, Ordering::SeqCst);
    let job = result.map_err(|e| e.to_string())??;
    // Same backstops as the merge commands: the new photo's thumbnail and Develop preview.
    tauri::async_runtime::spawn_blocking(move || preview_cache::pregenerate_missing(&catalog, &previews));
    let catalog_for_thumbs = state.catalog.clone();
    tauri::async_runtime::spawn_blocking(move || import::generate_missing_thumbnails(&catalog_for_thumbs, &thumbnails));
    Ok(job)
}

#[allow(clippy::too_many_arguments)]
fn run_job(
    emit: &dyn Fn(JobProgress),
    catalog: &Arc<std::sync::Mutex<Catalog>>,
    helper: &Arc<AiHelper>,
    id: u64,
    models: &Path,
    denoise_dir: &Path,
    scratch: &Path,
    version_id: i64,
    target: Target,
    strength: u8,
    jpeg_quality: u8,
    file_name: Option<&str>,
    add_to_collections: bool,
    metadata_note: bool,
) -> Result<UpscaleJobResult, String> {
    let model = upscale_models::ready_path(models).ok_or_else(|| "the AI enlargement model is not installed".to_string())?;
    let (source_path, stack) = {
        let catalog = catalog.lock().map_err(|e| e.to_string())?;
        let source = catalog.get_version_source(version_id).map_err(|e| e.to_string())?;
        if catalog.is_enlargement(source.image_id).map_err(|e| e.to_string())? {
            return Err(ENLARGEMENT_OF_ENLARGEMENT.to_string());
        }
        (PathBuf::from(source.path), catalog.get_edit_stack(version_id).map_err(|e| e.to_string())?)
    };
    // Next to the original, never overwriting (RFC §3.1): `<stem>-Enlarged-4x.jpg`, `-2`, `-3` ... on a clash.
    let dir = match source_path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let original_stem = source_path.file_stem().and_then(|s| s.to_str()).unwrap_or("photo");
    let stem = file_name.and_then(clean_stem).unwrap_or_else(|| upscale_job::result_stem(original_stem, target));
    let out_path = export::unique_output_path(&dir, &stem, "jpg");

    let spec = Spec { source_path: &source_path, stack: &stack, denoise_dir: Some(denoise_dir), scratch_dir: scratch, model: &model, out_path: &out_path, target, strength, jpeg_quality };
    let enlarged = upscale_job::run_enlargement(&spec, id, |req: Request| {
        helper
            .request_with_id(id, req, SILENCE_TIMEOUT, |p: Progress| emit(JobProgress { done: p.done, total: p.total }))
            .map_err(|e| match e {
                AiError::Remote { code, .. } if code == "cancelled" => "upscale cancelled".to_string(),
                other => other.to_string(),
            })
            .and_then(|v| serde_json::from_value(v).map_err(|e| e.to_string()))
    })?;
    let tiles = enlarged.result.tiles.max(1) as f64;
    MEASURED_MS_PER_TILE.store((enlarged.result.ms as f64 / tiles).to_bits(), Ordering::Relaxed);

    let (image_id, new_version) = {
        let catalog = catalog.lock().map_err(|e| e.to_string())?;
        register_result(&catalog, version_id, &stack, &out_path, &enlarged, strength, add_to_collections, metadata_note)?
    };
    Ok(UpscaleJobResult { image_id, version_id: new_version, path: out_path.to_string_lossy().to_string(), width: enlarged.result.width, height: enlarged.result.height, ms: enlarged.result.ms })
}

/// Stops the running job at its next tile (about half a second). A no-op when none is running.
#[tauri::command]
pub async fn upscale_cancel(helper: State<'_, Arc<AiHelper>>) -> Result<(), String> {
    let id = RUNNING_JOB.load(Ordering::SeqCst);
    if id == 0 {
        return Ok(());
    }
    let helper = helper.inner().clone();
    tauri::async_runtime::spawn_blocking(move || helper.cancel(id).map_err(|e| e.to_string())).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::protocol::UpscaleResult;
    use crate::catalog::EditStack;
    use serde_json::json;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("emulsion-upscale-commands-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_name_from_the_dialog_is_cleaned_into_a_safe_file_stem() {
        assert_eq!(clean_stem("IMG_1-Enlarged-4x.jpg").as_deref(), Some("IMG_1-Enlarged-4x"));
        assert_eq!(clean_stem("Poster.JPEG").as_deref(), Some("Poster"));
        assert_eq!(clean_stem("  my photo  ").as_deref(), Some("my photo"));
        assert_eq!(clean_stem("../../etc/passwd").as_deref(), Some("etcpasswd"));
        assert_eq!(clean_stem("a:b*c?.jpg").as_deref(), Some("abc"));
        assert_eq!(clean_stem(""), None);
        assert_eq!(clean_stem(" .jpg"), None);
        assert_eq!(clean_stem("..."), None);
        assert_eq!(clean_stem(&"x".repeat(300)).unwrap().len(), 120);
    }

    #[test]
    fn the_note_names_the_factor_the_model_and_the_strength() {
        let n = enlargement_note(4.0, 45);
        assert!(n.contains("×4") && n.contains(MODEL_ID) && n.contains("45%"), "{n}");
        assert!(n.contains("did not capture"));
    }

    #[test]
    fn targets_arrive_from_the_dialog_as_tagged_json() {
        let s: TargetArg = serde_json::from_value(json!({"kind": "scale", "value": 4})).unwrap();
        assert_eq!(Target::from(s), Target::Scale(4));
        let l: TargetArg = serde_json::from_value(json!({"kind": "long_edge", "value": 6000})).unwrap();
        assert_eq!(Target::from(l), Target::LongEdge(6000));
        assert!(serde_json::from_value::<TargetArg>(json!({"kind": "scale"})).is_err());
        assert!(serde_json::from_value::<TargetArg>(json!({"kind": "cube", "value": 1})).is_err());
    }

    fn catalog_with_photo(w: u32, h: u32, stack: &EditStack) -> (Catalog, i64, i64) {
        let catalog = Catalog::open_in_memory().unwrap();
        let meta = crate::metadata::ImageMetadata { width: Some(w), height: Some(h), ..Default::default() };
        let image = catalog.add_image_with_edit_stack("/orig.jpg", "hash-orig", 10, stack, &meta).unwrap();
        let version = catalog.list_images().unwrap().iter().find(|i| i.image_id == image).unwrap().version_id;
        (catalog, image, version)
    }

    #[test]
    fn the_plan_starts_from_the_recorded_size_and_follows_the_crop() {
        let (catalog, _, v) = catalog_with_photo(6000, 4000, &EditStack::empty());
        let p = plan_for_version(&catalog, v, Target::Scale(2), 95, 550.0).unwrap();
        assert_eq!((p.out_width, p.out_height), (12_000, 8_000));
        let cropped = EditStack { schema_version: 1, ops: vec![json!({"op": "crop", "x": 0.0, "y": 0.0, "width": 0.5, "height": 0.5, "angle": 0.0})] };
        let (catalog, _, v) = catalog_with_photo(6000, 4000, &cropped);
        let p = plan_for_version(&catalog, v, Target::Scale(2), 95, 550.0).unwrap();
        assert_eq!((p.source_width, p.source_height, p.out_width), (3000, 2000, 6000), "the enlargement is of the cropped picture");
    }

    #[test]
    fn an_enlargement_is_refused_and_so_is_a_photo_of_unknown_size() {
        let (catalog, _, v) = catalog_with_photo(6000, 4000, &EditStack::empty());
        let big = catalog
            .add_enlargement(&EnlargementInput {
                source_version_id: v,
                path: "/big.jpg".into(),
                content_hash: "h".into(),
                file_size: 1,
                width: 12_000,
                height: 8_000,
                scale: 2.0,
                strength: 50,
                model_id: "m".into(),
                source_stack_hash: "s".into(),
                add_to_collections: false,
            })
            .unwrap();
        let big_version = catalog.list_images().unwrap().iter().find(|i| i.image_id == big).unwrap().version_id;
        let err = plan_for_version(&catalog, big_version, Target::Scale(2), 95, 550.0).unwrap_err();
        assert!(err.contains("turned off on an enlargement"), "{err}");

        let unknown = Catalog::open_in_memory().unwrap();
        let image = unknown.add_image_with_edit_stack("/x.jpg", "hx", 1, &EditStack::empty(), &crate::metadata::ImageMetadata::default()).unwrap();
        let version = unknown.list_images().unwrap().iter().find(|i| i.image_id == image).unwrap().version_id;
        assert!(plan_for_version(&unknown, version, Target::Scale(2), 95, 550.0).unwrap_err().contains("not known"));
    }

    fn jpeg_file(d: &Path, name: &str) -> PathBuf {
        let p = d.join(name);
        image::RgbImage::from_pixel(48, 32, image::Rgb([100, 120, 80])).save(&p).unwrap();
        p
    }

    fn enlarged() -> Enlarged {
        Enlarged { result: UpscaleResult { width: 48, height: 32, tiles: 3, ms: 900 }, source_size: (12, 8) }
    }

    #[test]
    fn a_finished_file_is_noted_cataloged_and_linked_with_the_effective_scale() {
        let stack = EditStack { schema_version: 1, ops: vec![json!({"op": "exposure", "value": 0.4})] };
        let (catalog, orig, v) = catalog_with_photo(12, 8, &stack);
        let d = dir("register");
        let out = jpeg_file(&d, "big.jpg");
        let (image_id, version_id) = register_result(&catalog, v, &stack, &out, &enlarged(), 45, false, true).unwrap();
        let origin = catalog.get_enlargement_origin(image_id).unwrap().unwrap();
        assert_eq!((origin.source_image_id, origin.scale, origin.strength, origin.edited_since), (Some(orig), 4.0, 45, Some(false)));
        assert_eq!(catalog.get_edit_stack(version_id).unwrap(), EditStack::empty());
        let row = catalog.list_images().unwrap().into_iter().find(|i| i.image_id == image_id).unwrap();
        assert_eq!((row.width, row.height, row.path.as_str()), (Some(48), Some(32), out.to_str().unwrap()));
        // The note is in the file and the file is the one that was cataloged (hash and size match what is on disk).
        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(row.file_size, Some(bytes.len() as i64));
        assert_eq!(row.content_hash.as_deref(), Some(blake3::hash(&bytes).to_hex().as_str()));
        let meta = little_exif::metadata::Metadata::new_from_vec(&bytes, little_exif::filetype::FileExtension::JPEG).unwrap();
        assert!(meta.get_tag(&little_exif::exif_tag::ExifTag::ImageDescription(String::new())).next().is_some());
        assert!(!d.join("big.jpg.noted").exists());
    }

    #[test]
    fn without_the_check_box_the_file_is_left_exactly_as_the_helper_wrote_it() {
        let (catalog, _, v) = catalog_with_photo(12, 8, &EditStack::empty());
        let d = dir("no-note");
        let out = jpeg_file(&d, "big.jpg");
        let before = std::fs::read(&out).unwrap();
        register_result(&catalog, v, &EditStack::empty(), &out, &enlarged(), 45, false, false).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), before);
    }

    #[test]
    fn a_failed_catalog_insert_removes_the_file_it_was_about_to_register() {
        let (catalog, _, v) = catalog_with_photo(12, 8, &EditStack::empty());
        let d = dir("failed");
        let out = jpeg_file(&d, "big.jpg");
        let err = register_result(&catalog, 99_999, &EditStack::empty(), &out, &enlarged(), 45, false, false);
        assert!(err.is_err());
        assert!(!out.exists(), "an unregistered result must not be left next to the original");
        let _ = v;
    }

    /// The whole job with the real model and the real helper, against a real catalog:
    /// `UPSCALE_MODEL=/path/realesr_general_x4v3.onnx UPSCALE_HELPER=target/debug/emulsion-ai UPSCALE_IMAGE=/path/photo.jpg
    /// cargo test --release --lib -- --ignored --nocapture real_model_end_to_end`.
    #[test]
    #[ignore]
    fn real_model_end_to_end() {
        let model_src = PathBuf::from(std::env::var("UPSCALE_MODEL").expect("UPSCALE_MODEL"));
        let helper_exe = PathBuf::from(std::env::var("UPSCALE_HELPER").expect("UPSCALE_HELPER"));
        let image = PathBuf::from(std::env::var("UPSCALE_IMAGE").expect("UPSCALE_IMAGE"));
        let d = dir("e2e");
        let models = d.join("models");
        upscale_models::import_file(&models, &model_src).expect("the pinned model");
        let photo = d.join("photo.jpg");
        std::fs::copy(&image, &photo).unwrap();
        let (w, h) = image::image_dimensions(&photo).unwrap();
        let catalog = Arc::new(std::sync::Mutex::new(Catalog::open_in_memory().unwrap()));
        let version = {
            let c = catalog.lock().unwrap();
            let meta = crate::metadata::ImageMetadata { width: Some(w), height: Some(h), ..Default::default() };
            let id = c.add_image_with_edit_stack(&photo.to_string_lossy(), "h", 1, &EditStack::empty(), &meta).unwrap();
            c.list_images().unwrap().iter().find(|i| i.image_id == id).unwrap().version_id
        };
        let helper = Arc::new(AiHelper::new(helper_exe, 0));
        let seen = std::cell::Cell::new(0u32);
        let emit = |p: JobProgress| seen.set(p.done.max(seen.get()));
        let id = helper.reserve_id();
        let job = run_job(&emit, &catalog, &helper, id, &models, &d.join("denoise"), &d.join("scratch"), version, Target::Scale(2), 50, 95, None, false, true).expect("job");
        eprintln!("enlarged {w}x{h} -> {}x{} in {} ms, {} tiles reported, {}", job.width, job.height, job.ms, seen.get(), job.path);
        assert_eq!((job.width, job.height), (w * 2, h * 2));
        assert!(job.path.ends_with("photo-Enlarged-2x.jpg"));
        let c = catalog.lock().unwrap();
        assert!(c.is_enlargement(job.image_id).unwrap());
        assert_eq!(c.get_enlargement_origin(job.image_id).unwrap().unwrap().scale, 2.0);
        assert!(std::fs::read_dir(d.join("scratch")).unwrap().next().is_none(), "no intermediate left");
        helper.shutdown();
    }
}
