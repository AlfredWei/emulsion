mod catalog;
mod import;
mod raw_decode;

use catalog::{Catalog, ImageSummary};
use import::ImportSummary;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State};

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
