mod catalog;
mod export;
mod import;
mod jpeg_decode;
mod metadata;
mod preview_cache;
mod raw_decode;
mod source_decode;

use catalog::{Catalog, EditStack, ImageSummary, KeywordNode, KeywordRef};
use export::{ExportOptions, ExportResult};
use import::ImportSummary;
use preview_cache::DevelopPreviewInfo;
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
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let thumbnail_dir = app_data_dir.join("thumbnails");

    let summary = {
        let catalog = catalog.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let catalog = catalog.lock().map_err(|e| e.to_string())?;
            Ok::<_, String>(import::scan_and_import(
                std::path::Path::new(&path),
                &catalog,
                &thumbnail_dir,
            ))
        })
        .await
        .map_err(|e| e.to_string())?
    }?;

    // Fire-and-forget: pre-generate Develop previews and backfill any
    // missing thumbnails (M1 Slice 4 / M2 Slice 1, see preview_cache.rs /
    // import.rs) for the whole catalog in the background, rather than
    // making every Develop open pay for a fresh decode or leaving JPEG
    // imports permanently thumbnail-less. Not awaited -- doesn't delay
    // this command's response.
    let previews_dir = app_data_dir.join("previews");
    let thumbnail_dir_for_bg = app_data_dir.join("thumbnails");
    let catalog_for_thumbs = catalog.clone();
    tauri::async_runtime::spawn_blocking(move || {
        preview_cache::pregenerate_missing(&catalog, &previews_dir);
    });
    tauri::async_runtime::spawn_blocking(move || {
        import::generate_missing_thumbnails(&catalog_for_thumbs, &thumbnail_dir_for_bg);
    });

    Ok(summary)
}

/// Multi-file import (M2 Slice 1, see import.rs): the picker-dialog
/// alternative to `import_folder`'s whole-directory walk -- takes an
/// explicit list of user-selected file paths instead. Shares the same
/// per-file core (`import::import_paths`) and the same background-trigger
/// pattern as `import_folder`.
#[tauri::command]
async fn import_files(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<ImportSummary, String> {
    let catalog = state.catalog.clone();
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let thumbnail_dir = app_data_dir.join("thumbnails");

    let summary = {
        let catalog = catalog.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let paths: Vec<std::path::PathBuf> = paths.into_iter().map(std::path::PathBuf::from).collect();
            let catalog = catalog.lock().map_err(|e| e.to_string())?;
            Ok::<_, String>(import::import_paths(&paths, &catalog, &thumbnail_dir))
        })
        .await
        .map_err(|e| e.to_string())?
    }?;

    let previews_dir = app_data_dir.join("previews");
    let thumbnail_dir_for_bg = app_data_dir.join("thumbnails");
    let catalog_for_thumbs = catalog.clone();
    tauri::async_runtime::spawn_blocking(move || {
        preview_cache::pregenerate_missing(&catalog, &previews_dir);
    });
    tauri::async_runtime::spawn_blocking(move || {
        import::generate_missing_thumbnails(&catalog_for_thumbs, &thumbnail_dir_for_bg);
    });

    Ok(summary)
}

/// Backs the "Import Files…" picker dialog's filter list (M2 Slice 1) --
/// the frontend reads this at runtime instead of hand-duplicating
/// `source_decode::ImageFormat`'s extension list into JS, where it could
/// silently drift as more formats are added later.
#[tauri::command]
fn get_supported_extensions() -> Vec<String> {
    source_decode::ImageFormat::all_supported_extensions()
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

/// IPTC (M2 Slice 2). Persist directly to the catalog, never touch the
/// source file -- satisfies PRD's "never modify originals" constraint
/// with no IPTC-*writing* library needed at all. `caption` is per-version
/// (image_versions), `copyright`/`contact` are per-image (images) -- see
/// ImageSummary's doc comment in catalog.rs for why that split exists.
#[tauri::command]
fn set_caption(state: State<'_, AppState>, version_id: i64, caption: String) -> Result<(), String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.set_caption(version_id, &caption).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_copyright(state: State<'_, AppState>, image_id: i64, copyright: String) -> Result<(), String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.set_copyright(image_id, &copyright).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_contact(state: State<'_, AppState>, image_id: i64, contact: String) -> Result<(), String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.set_contact(image_id, &contact).map_err(|e| e.to_string())
}

/// Non-destructive removal (M2 Slice 3): catalog rows plus the app's own
/// derived files (thumbnail JPEG, content-hash-keyed Develop preview PNG)
/// -- the user's source file is NEVER touched (hard PRD constraint;
/// `RemovedImage` doesn't even carry the source path). File cleanup is
/// best-effort *after* the transaction commits: a failed unlink leaves an
/// orphaned file (the same accepted-orphan class as preview_cache.rs's
/// documented non-eviction), never a half-removed catalog row. Preview
/// deletion by content_hash is safe because import's `find_by_hash` dedupe
/// guarantees no second row shares a hash. Known, accepted race: a
/// background thumbnail/preview pass snapshotted before this removal can
/// re-create a file for a just-removed image -- an orphan on disk, bounded
/// to the one in-flight pass; rows can't be resurrected (those passes only
/// UPDATE by image_id, which matches nothing after the DELETE).
#[tauri::command]
async fn remove_images(
    app: AppHandle,
    state: State<'_, AppState>,
    image_ids: Vec<i64>,
) -> Result<(), String> {
    let catalog = state.catalog.clone();
    let previews_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("previews");

    tauri::async_runtime::spawn_blocking(move || {
        let removed = {
            let catalog = catalog.lock().map_err(|e| e.to_string())?;
            catalog.remove_images(&image_ids).map_err(|e| e.to_string())?
        };

        for image in removed {
            if let Some(thumbnail_path) = image.thumbnail_path {
                if let Err(e) = std::fs::remove_file(&thumbnail_path) {
                    eprintln!("thumbnail cleanup failed for {thumbnail_path}: {e}");
                }
            }
            if let Some(content_hash) = image.content_hash {
                let preview_path = previews_dir.join(format!("{content_hash}.png"));
                if preview_path.exists() {
                    if let Err(e) = std::fs::remove_file(&preview_path) {
                        eprintln!("preview cleanup failed for {}: {e}", preview_path.display());
                    }
                }
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Keywording (M2 Slice 4). Resolves (creating any missing level) a
/// hierarchical keyword path and assigns the leaf to every image in
/// `image_ids` -- batches across a multi-selection in one call. Returns
/// the leaf keyword id.
#[tauri::command]
fn assign_keyword_path(
    state: State<'_, AppState>,
    image_ids: Vec<i64>,
    path: Vec<String>,
) -> Result<i64, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.assign_keyword_path(&image_ids, &path).map_err(|e| e.to_string())
}

/// Anchor-only, matching the IPTC caption/copyright/contact precedent --
/// removing a keyword chip only affects the one image it's shown against.
#[tauri::command]
fn remove_keyword_from_image(
    state: State<'_, AppState>,
    image_id: i64,
    keyword_id: i64,
) -> Result<(), String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.remove_keyword_from_image(image_id, keyword_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_image_keywords(state: State<'_, AppState>, image_id: i64) -> Result<Vec<KeywordRef>, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.get_image_keywords(image_id).map_err(|e| e.to_string())
}

/// Backs the assignment input's autocomplete suggestions -- the frontend
/// builds full display paths client-side from this flat, parent_id-linked
/// list.
#[tauri::command]
fn list_keywords(state: State<'_, AppState>) -> Result<Vec<KeywordNode>, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.list_keywords().map_err(|e| e.to_string())
}

/// Develop preview (M1 Slice 3, cache-aware since M1 Slice 4 — see
/// preview_cache.rs). Decode-only concern -- doesn't touch the catalog,
/// matching how raw_decode.rs/import.rs are already decoupled from
/// catalog specifics. Runs on a blocking thread (same pattern as
/// `import_folder`) since a cache-miss decode is CPU-heavy (a cache hit
/// is cheap, but still worth keeping off the async executor's own thread
/// since it still touches disk).
#[tauri::command]
async fn get_develop_preview(app: AppHandle, path: String) -> Result<DevelopPreviewInfo, String> {
    let previews_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("previews");

    tauri::async_runtime::spawn_blocking(move || {
        preview_cache::ensure_develop_preview(std::path::Path::new(&path), &previews_dir)
            .map_err(|e| e.to_string())
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

#[derive(serde::Deserialize)]
struct ExportItem {
    path: String,
    version_id: i64,
}

/// Export (M1 Slice 5, see export.rs). Resolves each item's edit stack
/// under a brief catalog lock, then releases it before the slow
/// decode+resize+encode work -- matters more here than in the Slice 4
/// background job: AppState.catalog is one Mutex shared by every command,
/// and this is a request-response call the user is actively waiting on,
/// so holding the lock across a multi-second full-res export would
/// visibly stall unrelated UI actions (rating a photo, switching
/// selection) elsewhere in the app.
#[tauri::command]
async fn export_images(
    state: State<'_, AppState>,
    items: Vec<ExportItem>,
    options: ExportOptions,
) -> Result<Vec<ExportResult>, String> {
    let catalog = state.catalog.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let resolved = {
            let catalog = catalog.lock().map_err(|e| e.to_string())?;
            items
                .into_iter()
                .map(|item| {
                    let stack = catalog.get_edit_stack(item.version_id).unwrap_or_else(|_| EditStack::empty());
                    (std::path::PathBuf::from(item.path), stack)
                })
                .collect::<Vec<_>>()
        };
        Ok::<_, String>(export::export_batch(resolved, &options))
    })
    .await
    .map_err(|e| e.to_string())?
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
            let catalog = Arc::new(Mutex::new(catalog));

            // Catch-up pass (M1 Slice 4 / M2 Slice 1): pre-generate Develop
            // previews and backfill missing thumbnails for any cataloged
            // image that doesn't have one yet -- covers images cataloged
            // before these features existed, or left un-pregenerated by an
            // interrupted previous run. Cheap in steady state, fire-and-
            // forget so startup itself isn't delayed.
            let previews_dir = app_data_dir.join("previews");
            let thumbnail_dir = app_data_dir.join("thumbnails");
            let catalog_for_previews = catalog.clone();
            let catalog_for_thumbs = catalog.clone();
            tauri::async_runtime::spawn_blocking(move || {
                preview_cache::pregenerate_missing(&catalog_for_previews, &previews_dir);
            });
            tauri::async_runtime::spawn_blocking(move || {
                import::generate_missing_thumbnails(&catalog_for_thumbs, &thumbnail_dir);
            });

            app.manage(AppState { catalog });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            report_spike_result,
            import_folder,
            import_files,
            get_supported_extensions,
            list_images,
            set_rating,
            set_flag,
            set_color_label,
            set_caption,
            set_copyright,
            set_contact,
            remove_images,
            assign_keyword_path,
            remove_keyword_from_image,
            get_image_keywords,
            list_keywords,
            get_develop_preview,
            get_edit_stack,
            set_edit_stack,
            export_images,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
