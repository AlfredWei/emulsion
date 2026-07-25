mod catalog;
mod import;
mod raw_decode;

use catalog::{Catalog, EditStack, ImageSummary};
use import::ImportSummary;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State};

/// Interactive Develop preview is capped to this on its longest edge,
/// regardless of what the RAW decode itself produced -- `decode_develop_preview`'s
/// half_size request is best-effort (see raw_decode.rs), not a guarantee,
/// so this resize is what actually bounds the preview's size.
const DEVELOP_PREVIEW_MAX_DIMENSION: u32 = 2048;

#[derive(Debug, Clone, serde::Serialize)]
struct DevelopPreviewInfo {
    path: String,
    width: u32,
    height: u32,
}

fn capped_dimensions(width: u32, height: u32, max_dim: u32) -> (u32, u32) {
    if width <= max_dim && height <= max_dim {
        return (width, height);
    }
    let scale = max_dim as f64 / width.max(height) as f64;
    (
        ((width as f64) * scale).round().max(1.0) as u32,
        ((height as f64) * scale).round().max(1.0) as u32,
    )
}

/// App-wide state: the one catalog connection for this run, per ADR-0005
/// (a single local catalog file, not per-window/per-command connections).
struct AppState {
    catalog: Arc<Mutex<Catalog>>,
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// M0 spike only (docs/adr/ADR-0004, PROGRESS.md): the in-webview WebGPU
/// test page at /m0-spike can't be screenshotted by the tooling used to
/// build this app, so it reports its result here and this prints it to
/// the Rust process's own stdout, where it can be captured directly.
#[tauri::command]
fn report_spike_result(result_json: String) {
    println!("M0_SPIKE_RESULT: {result_json}");
}

/// Import (M1 Slice 1, see import.rs). Runs on a blocking thread so a
/// large folder of RAW files can't stall the UI (PRD §7.2).
#[tauri::command]
async fn import_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<ImportSummary, String> {
    let catalog = state.catalog.clone();
    let thumbnail_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("thumbnails");

    tauri::async_runtime::spawn_blocking(move || {
        let catalog = catalog.lock().map_err(|e| e.to_string())?;
        Ok(import::scan_and_import(
            std::path::Path::new(&path),
            &catalog,
            &thumbnail_dir,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn list_images(state: State<'_, AppState>) -> Result<Vec<ImageSummary>, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.list_images().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_rating(state: State<'_, AppState>, version_id: i64, rating: u8) -> Result<(), String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.set_rating(version_id, rating).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_flag(state: State<'_, AppState>, version_id: i64, flag: String) -> Result<(), String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.set_flag(version_id, &flag).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_color_label(
    state: State<'_, AppState>,
    version_id: i64,
    color_label: String,
) -> Result<(), String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog
        .set_color_label(version_id, &color_label)
        .map_err(|e| e.to_string())
}

/// Develop preview (M1 Slice 3, see raw_decode.rs). Decode-only concern --
/// doesn't touch the catalog, matching how raw_decode.rs/import.rs are
/// already decoupled from catalog specifics. Runs on a blocking thread
/// (same pattern as `import_folder`) since RAW decode is CPU-heavy.
#[tauri::command]
async fn get_develop_preview(app: AppHandle, path: String) -> Result<DevelopPreviewInfo, String> {
    let preview_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("previews");

    tauri::async_runtime::spawn_blocking(move || {
        std::fs::create_dir_all(&preview_dir).map_err(|e| e.to_string())?;

        let decoded = raw_decode::decode_develop_preview(std::path::Path::new(&path))
            .map_err(|e| e.to_string())?;
        let source = image::RgbImage::from_raw(decoded.width, decoded.height, decoded.rgb)
            .ok_or_else(|| {
                "decoded RGB buffer size didn't match its own reported dimensions".to_string()
            })?;

        let (target_w, target_h) =
            capped_dimensions(source.width(), source.height(), DEVELOP_PREVIEW_MAX_DIMENSION);
        let resized = if (target_w, target_h) == (source.width(), source.height()) {
            source
        } else {
            image::imageops::resize(
                &source,
                target_w,
                target_h,
                image::imageops::FilterType::Triangle,
            )
        };

        // Filename keyed off the source path's hash (not the catalog's
        // version_id) so this command can stay catalog-decoupled.
        let file_stem = blake3::hash(path.as_bytes()).to_hex();
        let out_path = preview_dir.join(format!("{file_stem}.png"));
        resized.save(&out_path).map_err(|e| e.to_string())?;

        Ok(DevelopPreviewInfo {
            path: out_path.to_string_lossy().to_string(),
            width: resized.width(),
            height: resized.height(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_edit_stack(state: State<'_, AppState>, version_id: i64) -> Result<EditStack, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.get_edit_stack(version_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_edit_stack(
    state: State<'_, AppState>,
    version_id: i64,
    stack: EditStack,
) -> Result<(), String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog
        .update_edit_stack(version_id, &stack)
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
            let catalog = Catalog::open(app_data_dir.join("catalog.sqlite"))?;
            app.manage(AppState {
                catalog: Arc::new(Mutex::new(catalog)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            report_spike_result,
            import_folder,
            list_images,
            set_rating,
            set_flag,
            set_color_label,
            get_develop_preview,
            get_edit_stack,
            set_edit_stack,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
