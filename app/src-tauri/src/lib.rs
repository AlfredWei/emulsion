mod catalog;
mod develop_engine;
mod export;
mod hdr_merge;
mod import;
mod jpeg_decode;
mod lens_profile;
mod metadata;
mod preview_cache;
mod print;
mod raw_decode;
mod soft_proof;
mod source_decode;
mod storage;

use catalog::{
    BackupOutcome, BackupSettings, Catalog, CollectionSummary, EditStack, HistoryEntry, ImageKeywordAssignment,
    ImageSummary, KeywordNode, KeywordRef, PresetEntry, SnapshotEntry,
};
use export::{ExportOptions, ExportResult};
use import::{ImportProgress, ImportSummary};
use preview_cache::DevelopPreviewInfo;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use storage::StorageInfo;
use tauri::menu::{AboutMetadataBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, State};

/// App-wide state: the one catalog connection for this run, per ADR-0005
/// (a single local catalog file, not per-window/per-command connections).
struct AppState {
    catalog: Arc<Mutex<Catalog>>,
}

/// Resolves the directory thumbnails/previews are read from and written
/// to right now: the user's Settings > Storage override
/// (`storage::move_cache_dir`) if one is set, else the OS's own app-data
/// directory. Called fresh on every command rather than cached, so a
/// location change takes effect on the very next operation -- see
/// `storage::move_cache_dir`'s own doc comment for the one narrow race
/// this doesn't close.
fn resolve_cache_root(app: &AppHandle, catalog: &Arc<Mutex<Catalog>>) -> Result<PathBuf, String> {
    let default_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let catalog = catalog.lock().map_err(|e| e.to_string())?;
    storage::resolve_cache_root(&catalog, &default_dir).map_err(|e| e.to_string())
}

fn resolve_thumbnail_dir(app: &AppHandle, catalog: &Arc<Mutex<Catalog>>) -> Result<PathBuf, String> {
    Ok(resolve_cache_root(app, catalog)?.join("thumbnails"))
}

fn resolve_previews_dir(app: &AppHandle, catalog: &Arc<Mutex<Catalog>>) -> Result<PathBuf, String> {
    Ok(resolve_cache_root(app, catalog)?.join("previews"))
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
    let thumbnail_dir = resolve_thumbnail_dir(&app, &catalog)?;

    let summary = {
        let catalog = catalog.clone();
        let progress_app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let catalog = catalog.lock().map_err(|e| e.to_string())?;
            Ok::<_, String>(import::scan_and_import_with_progress(
                std::path::Path::new(&path),
                &catalog,
                &thumbnail_dir,
                |current, total| {
                    let _ = progress_app.emit("import-progress", ImportProgress { current, total });
                },
            ))
        })
        .await
        .map_err(|e| e.to_string())?
    }?;

    // Fire-and-forget: pre-generate Develop previews for the whole catalog
    // in the background, rather than making every Develop open pay for a
    // fresh decode. Not awaited -- doesn't delay this command's response.
    // Thumbnail backfill is a SEPARATE, explicitly-awaited step now (see
    // `backfill_missing_thumbnails` below) -- the frontend's import
    // progress bar stays up through that call too, so "import done" and
    // "every thumbnail ready" are the same moment from the user's
    // perspective, not "done" followed by an untracked silent wait.
    let previews_dir = resolve_previews_dir(&app, &catalog)?;
    tauri::async_runtime::spawn_blocking(move || {
        preview_cache::pregenerate_missing(&catalog, &previews_dir);
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
    let thumbnail_dir = resolve_thumbnail_dir(&app, &catalog)?;

    let summary = {
        let catalog = catalog.clone();
        let progress_app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let paths: Vec<std::path::PathBuf> = paths.into_iter().map(std::path::PathBuf::from).collect();
            let catalog = catalog.lock().map_err(|e| e.to_string())?;
            Ok::<_, String>(import::import_paths_with_progress(
                &paths,
                &catalog,
                &thumbnail_dir,
                |current, total| {
                    let _ = progress_app.emit("import-progress", ImportProgress { current, total });
                },
            ))
        })
        .await
        .map_err(|e| e.to_string())?
    }?;

    // See import_folder's own comment: preview pregeneration stays
    // fire-and-forget, thumbnail backfill is now a separate awaited step.
    let previews_dir = resolve_previews_dir(&app, &catalog)?;
    tauri::async_runtime::spawn_blocking(move || {
        preview_cache::pregenerate_missing(&catalog, &previews_dir);
    });

    Ok(summary)
}

/// Backfills thumbnails for every cataloged image that doesn't have one
/// yet (import.rs's `generate_missing_thumbnails_with_progress`), emitting
/// a `"thumbnail-progress"` event per image so the frontend's import
/// progress bar can stay visible through this phase too -- previously this
/// ran fire-and-forget from inside `import_folder`/`import_files` with no
/// progress signal at all, so the progress bar disappeared the moment
/// cataloging finished even though freshly-imported JPEGs could still be
/// blank placeholders in the Library grid for a while longer. Called
/// explicitly by the frontend right after `import_folder`/`import_files`
/// resolves, and awaited -- once THIS command's promise resolves, every
/// thumbnail that import's own batch needed is on disk.
///
/// Scoped to `import_batch` (`ImportSummary::import_batch`, the batch id
/// that import's own images were tagged with) rather than the whole
/// catalog -- see `generate_missing_thumbnails_with_progress`'s own doc
/// comment for the real backlog this scoping was found to matter for: an
/// unrelated pre-existing backlog elsewhere in the catalog must never make
/// an unrelated NEW import's own progress bar wait on it.
#[tauri::command]
async fn backfill_missing_thumbnails(app: AppHandle, state: State<'_, AppState>, import_batch: i64) -> Result<(), String> {
    let catalog = state.catalog.clone();
    let thumbnail_dir = resolve_thumbnail_dir(&app, &catalog)?;

    tauri::async_runtime::spawn_blocking(move || {
        import::generate_missing_thumbnails_with_progress(&catalog, &thumbnail_dir, Some(import_batch), |current, total| {
            let _ = app.emit("thumbnail-progress", ImportProgress { current, total });
        });
    })
    .await
    .map_err(|e| e.to_string())
}

/// On-demand "jump the queue" thumbnail generation for one image (see
/// import.rs's `ensure_thumbnail` for the full reasoning): called when the
/// frontend opens Loupe or Develop on a photo whose `thumbnail_path` is
/// still null, so that photo's own thumbnail exists as soon as possible
/// regardless of how far behind it the background backfill pass is.
/// Returns the thumbnail path (freshly generated, or already-present if
/// the backfill pass got there first) so the frontend can patch its local
/// image list without a full `list_images()` round trip.
#[tauri::command]
async fn ensure_thumbnail(app: AppHandle, state: State<'_, AppState>, version_id: i64) -> Result<Option<String>, String> {
    let catalog = state.catalog.clone();
    let thumbnail_dir = resolve_thumbnail_dir(&app, &catalog)?;

    tauri::async_runtime::spawn_blocking(move || import::ensure_thumbnail(&catalog, version_id, &thumbnail_dir))
        .await
        .map_err(|e| e.to_string())
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

#[tauri::command]
fn set_geo_location(
    state: State<'_, AppState>,
    image_id: i64,
    latitude: Option<f64>,
    longitude: Option<f64>,
    altitude: Option<f32>,
) -> Result<(), String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog
        .set_geo_location(image_id, latitude, longitude, altitude)
        .map_err(|e| e.to_string())
}

/// Non-destructive removal (M2 Slice 3): catalog rows plus the app's own
/// derived files (thumbnail JPEG, content-hash-keyed Develop preview PNG)
/// -- the user's source file is NEVER touched (hard PRD constraint;
/// `RemovedImage` doesn't even carry the source path). File cleanup is
/// best-effort *after* the transaction commits: a failed unlink leaves an
/// orphaned file (the same accepted-orphan class as preview_cache.rs's
/// documented non-eviction), never a half-removed catalog row.
///
/// Cleanup is prefix-based, not a fixed list of filenames: thumbnails are
/// named `{image_id}.jpg` (`import.rs`'s plain thumbnail) or
/// `{image_id}-{stack_hash8}.jpg` (its edited-thumbnail variant, one new
/// file per distinct edit stack the image has ever had), and previews are
/// named `{content_hash}.png` / `{content_hash}_full.png` /
/// `{content_hash}_{stack_hash8}_graded.png` /
/// `{content_hash}_{stack_hash8}_proof_{settings_key}.png`
/// (`preview_cache.rs`) -- every one of those is a real, still-reachable
/// filename this app itself can produce for one image, and a fixed
/// two-filename list here silently left every graded/soft-proof variant
/// (and every edited-thumbnail variant beyond the current one) as a
/// permanent orphan on removal. `{content_hash}` is a fixed-length blake3
/// hex digest, so a `starts_with` prefix match against it can't
/// accidentally match a different image's files. Known, accepted race: a
/// background thumbnail/preview pass snapshotted before this removal can
/// still write a *new* file for a just-removed image after this scan
/// already ran -- an orphan on disk, bounded to the one in-flight pass;
/// rows can't be resurrected (those passes only UPDATE by image_id, which
/// matches nothing after the DELETE).
#[tauri::command]
async fn remove_images(
    app: AppHandle,
    state: State<'_, AppState>,
    image_ids: Vec<i64>,
) -> Result<(), String> {
    let catalog = state.catalog.clone();
    let previews_dir = resolve_previews_dir(&app, &catalog)?;
    let thumbnail_dir = resolve_thumbnail_dir(&app, &catalog)?;

    tauri::async_runtime::spawn_blocking(move || {
        let removed = {
            let catalog = catalog.lock().map_err(|e| e.to_string())?;
            catalog.remove_images(&image_ids).map_err(|e| e.to_string())?
        };

        for image in removed {
            // A plain `starts_with(image_id)` prefix check would be unsafe
            // here (unlike the content-hash case below): image_id "5" is a
            // literal string prefix of "50.jpg", "512-abcd1234.jpg", etc,
            // so removing one image could delete a completely different
            // image's thumbnails. Match either the exact plain filename or
            // the edited-variant prefix WITH its separator included.
            let plain_name = format!("{}.jpg", image.id);
            let edited_prefix = format!("{}-", image.id);
            remove_matching_files(&thumbnail_dir, |name| name == plain_name || name.starts_with(&edited_prefix));

            if let Some(content_hash) = image.content_hash {
                // Safe as a plain prefix match: content_hash is a
                // fixed-length blake3 hex digest, so two different hashes
                // can never have one be a literal prefix of the other.
                remove_matching_files(&previews_dir, |name| name.starts_with(&content_hash));
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Deletes every entry directly inside `dir` whose filename satisfies
/// `matches` -- shared by `remove_images`'s thumbnail and preview cleanup,
/// both of which need to catch every derived-cache variant an image can
/// have, not just the one filename the catalog happens to point at.
/// Best-effort per file and per directory read, same as the rest of this
/// command: a missing/unreadable `dir` (e.g. nothing was ever generated
/// there) or one file that can't be removed is logged, never surfaced as a
/// command failure.
fn remove_matching_files(dir: &std::path::Path, matches: impl Fn(&str) -> bool) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            eprintln!("cache cleanup: could not read {}: {e}", dir.display());
            return;
        }
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if matches(&name.to_string_lossy()) {
            if let Err(e) = std::fs::remove_file(entry.path()) {
                eprintln!("cache cleanup failed for {}: {e}", entry.path().display());
            }
        }
    }
}

/// HDR merge (M5, RFC-0003): merges `image_ids` (>= 2, all RAW, in the
/// caller's own bracket order) into one radiometrically-merged, tone-
/// mapped JPEG, cataloged as a new image with `hdr_merge_sources`
/// provenance rows (§3.6). `hdr_merge.rs` itself has no catalog
/// dependency -- this command is the bridge: resolve each id's path/EXIF
/// from the catalog, run the pure pixel pipeline, then write the result
/// and its provenance back. Runs on a blocking thread: linear-decoding
/// several full-resolution RAW files plus the alignment/merge pipeline is
/// real CPU work, the same class of cost as import's own RAW decode path.
/// The RAW-only check happens before any decode is attempted, matching
/// `hdr_merge::merge_bracket`'s own "reject fast on a known-bad input set"
/// ordering for its EV check.
///
/// The result is written to a NEW, content-hashed file under an
/// app-managed `merges` directory (mirroring `thumbnails`/`previews`) and
/// cataloged exactly like a JPEG import: `thumbnail_path` stays NULL, so
/// the existing background `generate_missing_thumbnails` pass picks it up
/// with no special-casing needed here.
#[tauri::command]
async fn merge_hdr_bracket(
    app: AppHandle,
    state: State<'_, AppState>,
    image_ids: Vec<i64>,
) -> Result<i64, String> {
    if image_ids.len() < 2 {
        return Err(format!("HDR merge needs at least 2 images, got {}", image_ids.len()));
    }

    let catalog = state.catalog.clone();
    // merges_dir holds real, irreplaceable output (a merge result IS the
    // image, not a regenerable cache of one) -- stays in the fixed
    // app-data location, unlike thumbnails/previews below (Settings >
    // Storage, resolve_thumbnail_dir/resolve_previews_dir).
    let merges_dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("merges");
    let previews_dir = resolve_previews_dir(&app, &catalog)?;
    let thumbnail_dir = resolve_thumbnail_dir(&app, &catalog)?;

    let result_image_id = tauri::async_runtime::spawn_blocking({
        let catalog = catalog.clone();
        move || -> Result<i64, String> {
            let inputs: Vec<hdr_merge::BracketInput> = {
                let catalog = catalog.lock().map_err(|e| e.to_string())?;
                image_ids
                    .iter()
                    .map(|&id| {
                        let info = catalog
                            .get_image_exposure_info(id)
                            .map_err(|e| e.to_string())?
                            .ok_or_else(|| format!("image {id} not found in catalog"))?;
                        let path = std::path::PathBuf::from(&info.path);
                        if source_decode::ImageFormat::from_path(&path) != Some(source_decode::ImageFormat::Raw) {
                            return Err(format!("HDR merge requires RAW files; {} is not RAW", info.path));
                        }
                        Ok(hdr_merge::BracketInput {
                            path,
                            iso: info.iso,
                            aperture: info.aperture,
                            shutter_speed: info.shutter_speed,
                        })
                    })
                    .collect::<Result<_, String>>()?
            };

            let merged = hdr_merge::merge_bracket(&inputs).map_err(|e| e.to_string())?;

            std::fs::create_dir_all(&merges_dir).map_err(|e| e.to_string())?;
            // Save-then-hash-then-rename: the encoded JPEG bytes ARE this
            // image's content, so (unlike every other content_hash in this
            // codebase, hashed from a pre-existing source file) there's no
            // hash to compute before the encode happens. A nanosecond-
            // timestamped temp name keeps concurrent merges from colliding
            // before the final content-hashed rename.
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let tmp_path = merges_dir.join(format!("tmp-{nanos}.jpg"));
            merged.image.save(&tmp_path).map_err(|e| e.to_string())?;
            let bytes = std::fs::read(&tmp_path).map_err(|e| e.to_string())?;
            let content_hash = blake3::hash(&bytes).to_hex().to_string();
            let out_path = merges_dir.join(format!("{content_hash}.jpg"));
            std::fs::rename(&tmp_path, &out_path).map_err(|e| e.to_string())?;

            let result_metadata = metadata::ImageMetadata {
                width: Some(merged.image.width()),
                height: Some(merged.image.height()),
                ..metadata::ImageMetadata::default()
            };

            let catalog = catalog.lock().map_err(|e| e.to_string())?;
            let result_image_id = catalog
                .add_image_with_edit_stack(
                    &out_path.to_string_lossy(),
                    &content_hash,
                    bytes.len() as i64,
                    &EditStack::empty(),
                    &result_metadata,
                )
                .map_err(|e| e.to_string())?;

            // `ev_offset` is relative to the reference frame (`ev_i -
            // ev_ref`), not each frame's own absolute EV -- the same
            // quantity `hdr_merge::merge_radiance` actually exponentiates
            // to build its radiometric scale factor, so this is the
            // number that explains *why* the merge weighted each source
            // the way it did, not just each source's own camera settings
            // (independently recoverable from its own `images` row
            // anyway).
            let reference_ev = merged.evs[merged.reference_idx];
            let sources: Vec<(i64, i32, f32, i32, i32)> = image_ids
                .iter()
                .enumerate()
                .map(|(i, &source_id)| {
                    let (dx, dy) = merged.offsets[i];
                    (source_id, i as i32, merged.evs[i] - reference_ev, dx, dy)
                })
                .collect();
            catalog
                .add_hdr_merge_sources(result_image_id, &sources)
                .map_err(|e| e.to_string())?;

            Ok(result_image_id)
        }
    })
    .await
    .map_err(|e| e.to_string())??;

    // Same backstop as import_files: fills in the new image's thumbnail
    // (and, opportunistically, anything else missing) in the background
    // rather than blocking this command's own return on it.
    tauri::async_runtime::spawn_blocking(move || {
        preview_cache::pregenerate_missing(&catalog, &previews_dir);
    });
    let catalog_for_thumbs = state.catalog.clone();
    tauri::async_runtime::spawn_blocking(move || {
        import::generate_missing_thumbnails(&catalog_for_thumbs, &thumbnail_dir);
    });

    Ok(result_image_id)
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

/// Collections (M2 Slice 5). Backs Smart Collections' "has keyword" /
/// "untagged" rules -- fetched once, flat, independent of `list_images()`.
#[tauri::command]
fn list_all_image_keywords(state: State<'_, AppState>) -> Result<Vec<ImageKeywordAssignment>, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.list_all_image_keywords().map_err(|e| e.to_string())
}

#[tauri::command]
fn create_collection(state: State<'_, AppState>, name: String) -> Result<i64, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.create_collection(&name).map_err(|e| e.to_string())
}

#[tauri::command]
fn create_collection_with_images(
    state: State<'_, AppState>,
    name: String,
    image_ids: Vec<i64>,
) -> Result<i64, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog
        .create_collection_with_images(&name, &image_ids)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn create_smart_collection(
    state: State<'_, AppState>,
    name: String,
    rules: Vec<serde_json::Value>,
) -> Result<i64, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.create_smart_collection(&name, &rules).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_smart_collection_rules(
    state: State<'_, AppState>,
    collection_id: i64,
    rules: Vec<serde_json::Value>,
) -> Result<(), String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog
        .update_smart_collection_rules(collection_id, &rules)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_collection(state: State<'_, AppState>, collection_id: i64) -> Result<(), String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.delete_collection(collection_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn add_images_to_collection(
    state: State<'_, AppState>,
    collection_id: i64,
    image_ids: Vec<i64>,
) -> Result<(), String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog
        .add_images_to_collection(collection_id, &image_ids)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn remove_images_from_collection(
    state: State<'_, AppState>,
    collection_id: i64,
    image_ids: Vec<i64>,
) -> Result<(), String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog
        .remove_images_from_collection(collection_id, &image_ids)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_collections(state: State<'_, AppState>) -> Result<Vec<CollectionSummary>, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.list_collections().map_err(|e| e.to_string())
}

#[tauri::command]
fn list_collection_image_ids(state: State<'_, AppState>, collection_id: i64) -> Result<Vec<i64>, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.list_collection_image_ids(collection_id).map_err(|e| e.to_string())
}

/// Develop preview (M1 Slice 3, cache-aware since M1 Slice 4 — see
/// preview_cache.rs). Decode-only concern -- doesn't touch the catalog,
/// matching how raw_decode.rs/import.rs are already decoupled from
/// catalog specifics: `content_hash` is a caller-supplied input (the
/// frontend already holds it on the `ImageSummary` it fetched for the
/// Library/Develop), not looked up here. Runs on a blocking thread (same
/// pattern as `import_folder`) since a cache-miss decode is CPU-heavy (a
/// cache hit is cheap, but still worth keeping off the async executor's
/// own thread since it still touches disk).
///
/// `content_hash` backs Smart Previews (M4): if the source file itself
/// can't be read (moved/offline drive), `ensure_develop_preview` falls
/// back to this hash's existing cache entry rather than failing outright.
/// `None` (e.g. a catalog row that predates the `content_hash` column) is
/// still a fully valid call -- just with no fallback available.
#[tauri::command]
async fn get_develop_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    content_hash: Option<String>,
) -> Result<DevelopPreviewInfo, String> {
    let previews_dir = resolve_previews_dir(&app, &state.catalog)?;

    tauri::async_runtime::spawn_blocking(move || {
        // user_message(), not to_string() -- a missing/moved/offline
        // source file (PreviewCacheError::Io with ErrorKind::NotFound) is
        // a normal, expected-to-happen-eventually reality for any photo
        // catalog; the raw OS errno string ("No such file or directory
        // (os error 2)") means nothing to someone who never touched the
        // filesystem directly. See PreviewCacheError::user_message's own
        // doc comment.
        preview_cache::ensure_develop_preview(
            std::path::Path::new(&path),
            &previews_dir,
            content_hash.as_deref(),
        )
        .map_err(|e| e.user_message())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The 1:1 tier alongside `get_develop_preview`'s Standard tier -- a
/// sibling command, not a parameter on the existing one, matching this
/// codebase's established one-function-per-concern split (e.g.
/// `decode_preview`/`decode_develop_preview`). `DevelopCanvas.svelte`
/// only calls this once the user actually zooms an image to 100%; see
/// `preview_cache::ensure_develop_full_preview`'s own doc comment for why
/// this is lazy, not part of the background pregeneration pass.
#[tauri::command]
async fn get_develop_full_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    content_hash: Option<String>,
) -> Result<DevelopPreviewInfo, String> {
    let previews_dir = resolve_previews_dir(&app, &state.catalog)?;

    tauri::async_runtime::spawn_blocking(move || {
        // user_message(), not to_string() -- same reasoning as
        // get_develop_preview's own identical switch above.
        preview_cache::ensure_develop_full_preview(
            std::path::Path::new(&path),
            &previews_dir,
            content_hash.as_deref(),
        )
        .map_err(|e| e.user_message())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Edit-graded preview for Library mode's Loupe view (`LibraryImageViewer.svelte`)
/// -- unlike `get_develop_preview`/`get_develop_full_preview` (both a pure,
/// unedited decode, existing purely as a GPU source texture for
/// `DevelopCanvas.svelte`'s own shader pipeline to grade), this bakes the
/// CURRENT edit stack in on the Rust side, via the same pipeline
/// `regenerate_thumbnail` already established for the grid thumbnail --
/// see `preview_cache::ensure_graded_preview_for_hash`'s own doc comment.
/// Takes `version_id` (not a bare path) since it needs to look up the
/// image's current edit stack, which only the catalog knows -- re-reads
/// it fresh on every call rather than trusting a client-supplied one, same
/// "always re-read from the catalog" discipline `regenerate_thumbnail`
/// already follows.
#[tauri::command]
async fn get_graded_develop_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    version_id: i64,
) -> Result<DevelopPreviewInfo, String> {
    let catalog = state.catalog.clone();
    let previews_dir = resolve_previews_dir(&app, &catalog)?;

    tauri::async_runtime::spawn_blocking(move || {
        let (source, stack) = {
            let catalog = catalog.lock().map_err(|e| e.to_string())?;
            let source = catalog.get_version_source(version_id).map_err(|e| e.to_string())?;
            let stack = catalog.get_edit_stack(version_id).map_err(|e| e.to_string())?;
            (source, stack)
        };

        preview_cache::ensure_graded_preview_for_hash(
            std::path::Path::new(&source.path),
            source.content_hash.as_deref().unwrap_or(""),
            &stack,
            &previews_dir,
        )
        .map_err(|e| e.user_message())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Soft-proof simulation (M4) of the CURRENT edit-graded look on a target
/// ICC profile, for `DevelopPanel.svelte`'s "Soft Proof" toggle -- mirrors
/// `get_graded_develop_preview`'s own shape exactly (look up source+stack
/// under a brief catalog lock, release it, then do the slow work on a
/// blocking thread), with `settings` (target profile, rendering intent,
/// gamut-warning flag) supplied fresh by the frontend on every call rather
/// than persisted anywhere -- this is ephemeral view state, not part of
/// the edit stack (see `soft_proof.rs`'s own doc comment).
#[tauri::command]
async fn get_soft_proof_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    version_id: i64,
    settings: soft_proof::SoftProofSettings,
) -> Result<DevelopPreviewInfo, String> {
    let catalog = state.catalog.clone();
    let previews_dir = resolve_previews_dir(&app, &catalog)?;

    tauri::async_runtime::spawn_blocking(move || {
        let (source, stack) = {
            let catalog = catalog.lock().map_err(|e| e.to_string())?;
            let source = catalog.get_version_source(version_id).map_err(|e| e.to_string())?;
            let stack = catalog.get_edit_stack(version_id).map_err(|e| e.to_string())?;
            (source, stack)
        };

        preview_cache::ensure_soft_proof_preview_for_hash(
            std::path::Path::new(&source.path),
            source.content_hash.as_deref().unwrap_or(""),
            &stack,
            &settings,
            &previews_dir,
        )
        .map_err(|e| e.user_message())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_edit_stack(state: State<'_, AppState>, version_id: i64) -> Result<EditStack, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.get_edit_stack(version_id).map_err(|e| e.to_string())
}

/// Matches a photo's camera+lens EXIF against the bundled lens-correction
/// database (M3: Lens Corrections). Takes the metadata directly (not an
/// image_id) -- the frontend already holds every field on the
/// `ImageSummary` it fetched for the Library grid/Develop, so there's no
/// need for a second catalog round trip just to re-fetch what's already
/// in memory. `None` covers "no match" for any reason (missing metadata,
/// no equipment match, matched equipment with no usable calibration data)
/// -- see `lens_profile::match_profile`'s own doc comment.
#[tauri::command]
fn lookup_lens_profile(
    camera_make: Option<String>,
    camera_model: Option<String>,
    lens_model: Option<String>,
    focal_length: Option<f32>,
    aperture: Option<f32>,
) -> Option<lens_profile::LensProfileMatch> {
    lens_profile::match_profile(
        camera_make.as_deref(),
        camera_model.as_deref(),
        lens_model.as_deref(),
        focal_length,
        aperture,
    )
}

/// `label` is `Some` for a real, user-attributable edit (e.g. "Exposure",
/// "Crop") and `None` for the many flush call sites that fire
/// unconditionally with nothing new pending (switching images, exporting,
/// closing the window) -- see `Catalog::record_edit_stack`'s own doc
/// comment. Returns the version's fresh history list so the frontend's
/// History panel can refresh in the same round trip, without a second
/// `get_history` call.
#[tauri::command]
fn set_edit_stack(
    state: State<'_, AppState>,
    version_id: i64,
    stack: EditStack,
    label: Option<String>,
) -> Result<Vec<HistoryEntry>, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog
        .record_edit_stack(version_id, &stack, label.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_history(state: State<'_, AppState>, version_id: i64) -> Result<Vec<HistoryEntry>, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.get_history(version_id).map_err(|e| e.to_string())
}

/// Moves the live edit stack to a past history entry -- undo, redo, and
/// jump-to-entry are all the same operation client-side (see
/// `Catalog::restore_history_entry`'s doc comment: no persisted cursor,
/// the client just re-derives its position from the returned stack).
/// Deliberately does NOT itself create a new history row.
#[tauri::command]
fn restore_history_entry(
    state: State<'_, AppState>,
    version_id: i64,
    history_id: i64,
) -> Result<EditStack, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog
        .restore_history_entry(version_id, history_id)
        .map_err(|e| e.to_string())
}

/// Renders a small graded-preview thumbnail for a PAST history entry's
/// resulting look (M4.5 hover-preview) -- merges `Catalog::peek_history_entry`'s
/// read-only lookup (never writes to `image_versions`, unlike
/// `restore_history_entry` above) with a render into one round trip,
/// rather than a separate "fetch the stack" + "render it" pair of calls.
/// Reuses `preview_cache::ensure_graded_preview_for_hash` -- the SAME
/// draft-tier, content-hash-cached preview Library's Loupe view already
/// renders (`get_graded_develop_preview` below) -- so a hover never has
/// to drive the full interactive Develop canvas (DevelopCanvas.svelte's
/// own WebGPU pipeline) just to preview a look; only this one small
/// static image is produced, and it's cached by content hash + stack
/// hash, so re-hovering the same entry is free after the first time.
#[tauri::command]
async fn preview_history_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    version_id: i64,
    history_id: i64,
    path: String,
    content_hash: Option<String>,
) -> Result<DevelopPreviewInfo, String> {
    let catalog = state.catalog.clone();
    let previews_dir = resolve_previews_dir(&app, &catalog)?;

    tauri::async_runtime::spawn_blocking(move || {
        let stack = {
            let catalog = catalog.lock().map_err(|e| e.to_string())?;
            catalog.peek_history_entry(version_id, history_id).map_err(|e| e.to_string())?
        };
        preview_cache::ensure_graded_preview_for_hash(
            std::path::Path::new(&path),
            content_hash.as_deref().unwrap_or(""),
            &stack,
            &previews_dir,
        )
        .map_err(|e| e.user_message())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Same hover-preview purpose as `preview_history_entry` above, for a
/// snapshot -- see that command's own doc comment.
#[tauri::command]
async fn preview_snapshot(
    app: AppHandle,
    state: State<'_, AppState>,
    version_id: i64,
    snapshot_id: i64,
    path: String,
    content_hash: Option<String>,
) -> Result<DevelopPreviewInfo, String> {
    let catalog = state.catalog.clone();
    let previews_dir = resolve_previews_dir(&app, &catalog)?;

    tauri::async_runtime::spawn_blocking(move || {
        let stack = {
            let catalog = catalog.lock().map_err(|e| e.to_string())?;
            catalog.peek_snapshot(version_id, snapshot_id).map_err(|e| e.to_string())?
        };
        preview_cache::ensure_graded_preview_for_hash(
            std::path::Path::new(&path),
            content_hash.as_deref().unwrap_or(""),
            &stack,
            &previews_dir,
        )
        .map_err(|e| e.user_message())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Renders a small graded-preview thumbnail for an ARBITRARY, not-yet-
/// applied `EditStack` -- backs the Preset hover-preview (M4.5): unlike
/// History/Snapshot entries, a preset's ops are already fully in memory
/// on the frontend (`PresetEntry.edit_stack`), so JS merges it onto the
/// currently open image's real edit stack itself (`applyPresetOps`) and
/// this command only needs to render the resulting look, not look
/// anything up in the catalog.
#[tauri::command]
async fn preview_edit_stack(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    content_hash: Option<String>,
    stack: EditStack,
) -> Result<DevelopPreviewInfo, String> {
    let previews_dir = resolve_previews_dir(&app, &state.catalog)?;

    tauri::async_runtime::spawn_blocking(move || {
        preview_cache::ensure_graded_preview_for_hash(
            std::path::Path::new(&path),
            content_hash.as_deref().unwrap_or(""),
            &stack,
            &previews_dir,
        )
        .map_err(|e| e.user_message())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn add_snapshot(state: State<'_, AppState>, version_id: i64, name: String) -> Result<SnapshotEntry, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.add_snapshot(version_id, &name).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_snapshots(state: State<'_, AppState>, version_id: i64) -> Result<Vec<SnapshotEntry>, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.get_snapshots(version_id).map_err(|e| e.to_string())
}

/// Restoring a snapshot IS a new, undoable edit (unlike
/// `restore_history_entry`) -- see `Catalog::restore_snapshot`'s doc
/// comment. Returns the fresh history list alongside the restored stack
/// for the same single-round-trip reason `set_edit_stack` does.
#[tauri::command]
fn restore_snapshot(
    state: State<'_, AppState>,
    version_id: i64,
    snapshot_id: i64,
) -> Result<(EditStack, Vec<HistoryEntry>), String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog
        .restore_snapshot(version_id, snapshot_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_snapshot(state: State<'_, AppState>, version_id: i64, snapshot_id: i64) -> Result<(), String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog
        .delete_snapshot(version_id, snapshot_id)
        .map_err(|e| e.to_string())
}

/// Presets (M3): global, catalog-wide -- `stack` is expected to already
/// be filtered to the preset-eligible op subset (JS's job, via
/// develop.js's `PRESET_EXCLUDED_OP_NAMES`); Rust stores it as-is, same
/// "never interprets `ops`" boundary every other edit-stack command
/// keeps. Shared by both the direct "Save Current as Preset" flow and,
/// after JS re-filters an imported file's ops, the import flow below.
#[tauri::command]
fn create_preset(state: State<'_, AppState>, name: String, stack: EditStack) -> Result<PresetEntry, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.create_preset(&name, &stack).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_presets(state: State<'_, AppState>) -> Result<Vec<PresetEntry>, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.list_presets().map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_preset(state: State<'_, AppState>, preset_id: i64) -> Result<(), String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    catalog.delete_preset(preset_id).map_err(|e| e.to_string())
}

/// The on-disk shape of an exported preset file -- deliberately a plain
/// `{name, schema_version, ops}` struct, not `PresetEntry` (which carries
/// a catalog `id`/`created_at` that are meaningless once exported to a
/// portable file another catalog might import).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PresetFile {
    name: String,
    schema_version: u32,
    ops: Vec<serde_json::Value>,
}

/// Raw file read + parse only -- no catalog write, no op-name filtering.
/// A foreign or hand-edited file could contain `crop`/mask ops; JS is
/// responsible for re-applying `PRESET_EXCLUDED_OP_NAMES` defensively
/// before ever calling `create_preset` with the result (see that
/// command's own doc comment) -- Rust never interprets `ops`, so it
/// can't safely do this filtering itself.
#[tauri::command]
fn import_preset_file(path: String) -> Result<PresetFile, String> {
    let contents = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&contents).map_err(|e| e.to_string())
}

#[tauri::command]
fn export_preset_file(name: String, stack: EditStack, path: String) -> Result<(), String> {
    let file = PresetFile { name, schema_version: stack.schema_version, ops: stack.ops };
    let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// Thumbnail refresh after a Develop edit -- called by the frontend after
/// `set_edit_stack` has already persisted, never chained onto that same
/// call/promise (a stale Library thumbnail is a much smaller loss than a
/// lost edit, so this must never be able to block on/delay the edit save
/// itself, or app quit, the way M1 Slice 6 already found and fixed for a
/// similar edit-stack-flush hang). Re-reads the edit stack from the
/// catalog rather than trusting a client-supplied one, since this always
/// runs immediately after a real save. Returns `Ok(None)` (not an error)
/// on any failure -- see `import::regenerate_edited_thumbnail`'s doc
/// comment for why a stale thumbnail isn't worth surfacing as a hard
/// error to the frontend.
#[tauri::command]
async fn regenerate_thumbnail(app: AppHandle, state: State<'_, AppState>, version_id: i64) -> Result<Option<String>, String> {
    let catalog = state.catalog.clone();
    let previews_dir = resolve_previews_dir(&app, &catalog)?;
    let thumbnail_dir = resolve_thumbnail_dir(&app, &catalog)?;

    tauri::async_runtime::spawn_blocking(move || {
        let (source, stack) = {
            let catalog = catalog.lock().map_err(|e| e.to_string())?;
            let source = catalog.get_version_source(version_id).map_err(|e| e.to_string())?;
            let stack = catalog.get_edit_stack(version_id).map_err(|e| e.to_string())?;
            (source, stack)
        };

        let Some(out_path) = import::regenerate_edited_thumbnail(
            std::path::Path::new(&source.path),
            source.content_hash.as_deref().unwrap_or(""),
            source.image_id,
            &stack,
            &previews_dir,
            &thumbnail_dir,
        ) else {
            return Ok(None);
        };
        let out_path_str = out_path.to_string_lossy().to_string();

        let catalog = catalog.lock().map_err(|e| e.to_string())?;
        catalog
            .set_thumbnail_path(source.image_id, &out_path_str)
            .map_err(|e| e.to_string())?;
        Ok(Some(out_path_str))
    })
    .await
    .map_err(|e| e.to_string())?
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

/// Print module (M4, final scope item, see print.rs). Same shape as
/// `get_graded_develop_preview`/`get_soft_proof_preview`: takes bare
/// `version_id`s, re-resolves each one's source path/content_hash/edit
/// stack fresh from the catalog under a brief lock (never a
/// client-supplied path or hash) rather than `export_images`'s
/// frontend-trusts-the-path convention -- print needs the content_hash for
/// its own cache key, which only the catalog holds. `color_management` is
/// supplied fresh by the frontend on every call, matching soft-proof
/// settings' own "ephemeral view state, never persisted" treatment.
#[tauri::command]
async fn get_print_ready_images(
    app: AppHandle,
    state: State<'_, AppState>,
    version_ids: Vec<i64>,
    color_management: print::PrintColorManagement,
) -> Result<Vec<print::PrintReadyResult>, String> {
    let catalog = state.catalog.clone();
    let previews_dir = resolve_previews_dir(&app, &catalog)?;

    tauri::async_runtime::spawn_blocking(move || {
        // A version_id whose source can't be resolved (deleted from the
        // catalog mid-selection, etc.) becomes an immediate error result --
        // never silently dropped, so the frontend's result list always
        // matches its request 1:1 and can tell which item failed.
        let (resolvable, mut results): (Vec<_>, Vec<_>) = {
            let catalog = catalog.lock().map_err(|e| e.to_string())?;
            let mut resolvable = Vec::new();
            let mut results = Vec::new();
            for version_id in version_ids {
                match catalog.get_version_source(version_id) {
                    Ok(source) => {
                        let stack = catalog.get_edit_stack(version_id).unwrap_or_else(|_| EditStack::empty());
                        resolvable.push((
                            version_id,
                            std::path::PathBuf::from(source.path),
                            source.content_hash.unwrap_or_default(),
                            stack,
                        ));
                    }
                    Err(e) => results.push(print::PrintReadyResult {
                        version_id,
                        path: None,
                        width: None,
                        height: None,
                        error: Some(e.to_string()),
                    }),
                }
            }
            (resolvable, results)
        };
        results.extend(print::generate_print_ready_batch(resolvable, &color_management, &previews_dir));
        Ok::<_, String>(results)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Deserialize)]
struct PrintPdfRequest {
    version_ids: Vec<i64>,
    destination_path: String,
    layout: print::PdfLayout,
    page: print::PdfPageSetup,
    color_management: print::PrintColorManagement,
}

/// "Export as PDF" (see print.rs's own doc comment on `export_pdf`) --
/// unlike `get_print_ready_images`, a PDF export is one atomic file, not a
/// list of independent per-item results, so any single unresolvable
/// `version_id` fails the whole command rather than producing a partial or
/// silently-wrong PDF.
#[tauri::command]
async fn export_print_pdf(app: AppHandle, state: State<'_, AppState>, request: PrintPdfRequest) -> Result<(), String> {
    let catalog = state.catalog.clone();
    let previews_dir = resolve_previews_dir(&app, &catalog)?;

    tauri::async_runtime::spawn_blocking(move || {
        let resolved = {
            let catalog = catalog.lock().map_err(|e| e.to_string())?;
            let mut resolved = Vec::new();
            for version_id in &request.version_ids {
                let source = catalog.get_version_source(*version_id).map_err(|e| e.to_string())?;
                let stack = catalog.get_edit_stack(*version_id).unwrap_or_else(|_| EditStack::empty());
                resolved.push((*version_id, std::path::PathBuf::from(source.path), source.content_hash.unwrap_or_default(), stack));
            }
            resolved
        };

        print::export_pdf(
            resolved,
            &request.layout,
            &request.page,
            &request.color_management,
            &previews_dir,
            std::path::Path::new(&request.destination_path),
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_backup_settings(state: State<'_, AppState>) -> Result<BackupSettings, String> {
    state.catalog.lock().map_err(|e| e.to_string())?.get_backup_settings().map_err(|e| e.to_string())
}

#[tauri::command]
fn update_backup_settings(state: State<'_, AppState>, settings: BackupSettings) -> Result<(), String> {
    state
        .catalog
        .lock()
        .map_err(|e| e.to_string())?
        .update_backup_settings(&settings)
        .map_err(|e| e.to_string())
}

/// Settings > Storage: reports where thumbnails + the Develop preview
/// cache currently live (the user's override if one is set, else the
/// default) and how much disk space each is using -- the two facts the
/// Storage settings tab needs to render (PRD's underlying motivation:
/// these are the two caches most likely to grow into real GBs on a large
/// library, unlike the catalog database itself).
#[tauri::command]
fn get_storage_info(app: AppHandle, state: State<'_, AppState>) -> Result<StorageInfo, String> {
    let default_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    storage::get_storage_info(&catalog, &default_dir)
}

/// Moves existing thumbnails + preview-cache files to `new_dir` (or back
/// to the default app-data location if `new_dir` is `None`), rewrites the
/// catalog's stored thumbnail paths to match, and persists the choice --
/// see `storage::move_cache_dir`'s own doc comment for the full mechanics
/// and the one narrow race it doesn't close. `spawn_blocking`: this walks
/// and copies real files, potentially many, not a small metadata update.
#[tauri::command]
async fn set_cache_dir(app: AppHandle, state: State<'_, AppState>, new_dir: Option<String>) -> Result<StorageInfo, String> {
    let catalog = state.catalog.clone();
    let default_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        storage::move_cache_dir(&catalog, &default_dir, new_dir.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Catalog backup (PRD §7.6). `spawn_blocking`, matching `export_images` --
/// a real multi-second operation (optional integrity check + VACUUM + a
/// full-catalog copy), not something to run on the async executor's own
/// thread. Deliberately holds the catalog lock for the whole operation,
/// unlike `export_images`'s lock-briefly pattern -- see the doc comment on
/// `Catalog::perform_backup` for why that's an accepted, named exception
/// here rather than the same regression class this file's other commands
/// take care to avoid.
#[tauri::command]
async fn perform_catalog_backup(
    app: AppHandle,
    state: State<'_, AppState>,
    folder: String,
    check_integrity: bool,
    optimize: bool,
) -> Result<BackupOutcome, String> {
    let catalog = state.catalog.clone();
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;

    tauri::async_runtime::spawn_blocking(move || {
        let catalog = catalog.lock().map_err(|e| e.to_string())?;
        catalog
            .perform_backup(std::path::Path::new(&folder), &app_data_dir, check_integrity, optimize)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Native OS menu bar (M4.5 Slice 4): every item here just re-emits its id
/// as a `"menu-action"` event for the frontend to dispatch to the exact
/// same handler functions the in-UI controls already call (see
/// `on_menu_event` below and `+page.svelte`'s `listen("menu-action", ...)`)
/// -- Rust never interprets these ids, matching the "Rust never interprets
/// `ops`" boundary this file already keeps for edit-stack data.
///
/// Deliberately gives NO accelerator to Undo/Redo/Copy Settings/Paste
/// Settings/Select All/Deselect All: those already have DOM-level keyboard
/// shortcuts (`+page.svelte`'s `handleGlobalKeydown`) that explicitly skip
/// firing while a text field has focus (`isTypingTarget`), so a photographer
/// can Cmd+A-select text in a Caption/Keyword field. A native menu
/// accelerator is checked by the OS's own key-equivalent handling *before*
/// the event ever reaches the webview's DOM, bypassing that guard entirely
/// -- binding Cmd+A/Cmd+Z here would silently break normal text editing
/// anywhere in the app. The accelerators that ARE set below (Import Files,
/// Export, Preferences, module switching) don't collide with any
/// text-editing convention, so they're safe to expose as real OS shortcuts.
fn build_menu<R: tauri::Runtime>(app: &AppHandle<R>) -> tauri::Result<tauri::menu::Menu<R>> {
    let mut builder = MenuBuilder::new(app);

    if cfg!(target_os = "macos") {
        let about = PredefinedMenuItem::about(
            app,
            Some("About Emulsion"),
            Some(AboutMetadataBuilder::new().version(Some("0.1.0")).build()),
        )?;
        let preferences = MenuItemBuilder::with_id("preferences", "Preferences…")
            .accelerator("CmdOrCtrl+,")
            .build(app)?;
        let app_menu = SubmenuBuilder::new(app, "Emulsion")
            .item(&about)
            .separator()
            .item(&preferences)
            .separator()
            .services()
            .separator()
            .hide()
            .hide_others()
            .show_all()
            .separator()
            .quit()
            .build()?;
        builder = builder.item(&app_menu);
    }

    let import_folder = MenuItemBuilder::with_id("import_folder", "Import Folder…").build(app)?;
    let import_files = MenuItemBuilder::with_id("import_files", "Import Files…")
        .accelerator("CmdOrCtrl+Shift+I")
        .build(app)?;
    let export = MenuItemBuilder::with_id("export", "Export…")
        .accelerator("CmdOrCtrl+Shift+E")
        .build(app)?;
    let export_pdf = MenuItemBuilder::with_id("export_pdf", "Export Contact Sheet PDF…").build(app)?;
    let mut file_menu = SubmenuBuilder::new(app, "File")
        .item(&import_folder)
        .item(&import_files)
        .separator()
        .item(&export)
        .item(&export_pdf);
    if !cfg!(target_os = "macos") {
        let preferences = MenuItemBuilder::with_id("preferences", "Preferences…")
            .accelerator("CmdOrCtrl+,")
            .build(app)?;
        file_menu = file_menu.separator().item(&preferences).separator().quit();
    }
    builder = builder.item(&file_menu.build()?);

    let undo = MenuItemBuilder::with_id("undo", "Undo").build(app)?;
    let redo = MenuItemBuilder::with_id("redo", "Redo").build(app)?;
    let copy_settings = MenuItemBuilder::with_id("copy_settings", "Copy Settings").build(app)?;
    let paste_settings = MenuItemBuilder::with_id("paste_settings", "Paste Settings").build(app)?;
    let select_all = MenuItemBuilder::with_id("select_all", "Select All").build(app)?;
    let deselect_all = MenuItemBuilder::with_id("deselect_all", "Deselect All").build(app)?;
    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .item(&undo)
        .item(&redo)
        .separator()
        .item(&copy_settings)
        .item(&paste_settings)
        .separator()
        .item(&select_all)
        .item(&deselect_all)
        .build()?;
    builder = builder.item(&edit_menu);

    let view_library = MenuItemBuilder::with_id("view_library", "Library")
        .accelerator("CmdOrCtrl+1")
        .build(app)?;
    let view_develop = MenuItemBuilder::with_id("view_develop", "Develop")
        .accelerator("CmdOrCtrl+2")
        .build(app)?;
    let view_print = MenuItemBuilder::with_id("view_print", "Print")
        .accelerator("CmdOrCtrl+3")
        .build(app)?;
    let view_grid = MenuItemBuilder::with_id("view_grid", "Grid").build(app)?;
    let view_loupe = MenuItemBuilder::with_id("view_loupe", "Loupe").build(app)?;
    let view_compare = MenuItemBuilder::with_id("view_compare", "Compare").build(app)?;
    let view_survey = MenuItemBuilder::with_id("view_survey", "Survey").build(app)?;
    let toggle_clipping =
        MenuItemBuilder::with_id("toggle_clipping", "Toggle Clipping Overlay").build(app)?;
    let toggle_before_after =
        MenuItemBuilder::with_id("toggle_before_after", "Toggle Before / After").build(app)?;
    let view_menu = SubmenuBuilder::new(app, "View")
        .item(&view_library)
        .item(&view_develop)
        .item(&view_print)
        .separator()
        .item(&view_grid)
        .item(&view_loupe)
        .item(&view_compare)
        .item(&view_survey)
        .separator()
        .item(&toggle_clipping)
        .item(&toggle_before_after)
        .build()?;
    builder = builder.item(&view_menu);

    let window_menu = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .separator()
        .close_window()
        .build()?;
    builder = builder.item(&window_menu);

    builder.build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init());

    #[cfg(feature = "wdio-webdriver")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());

    builder
        .menu(|app| build_menu(app))
        .on_menu_event(|app, event| {
            let id: &str = event.id().as_ref();
            let _ = app.emit("menu-action", id);
        })
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
            // Settings > Storage override, if the user set one on a
            // previous run -- same resolver every other command uses.
            // (resolve_*_dir return `Result<_, String>` for command call
            // sites; `io::Error::other` bridges to this hook's own
            // `Box<dyn Error>` return type.)
            let previews_dir =
                resolve_previews_dir(app.handle(), &catalog).map_err(std::io::Error::other)?;
            let thumbnail_dir =
                resolve_thumbnail_dir(app.handle(), &catalog).map_err(std::io::Error::other)?;
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
            backfill_missing_thumbnails,
            ensure_thumbnail,
            get_supported_extensions,
            list_images,
            set_rating,
            set_flag,
            set_color_label,
            set_caption,
            set_copyright,
            set_contact,
            set_geo_location,
            remove_images,
            merge_hdr_bracket,
            assign_keyword_path,
            remove_keyword_from_image,
            get_image_keywords,
            list_keywords,
            list_all_image_keywords,
            create_collection,
            create_collection_with_images,
            create_smart_collection,
            update_smart_collection_rules,
            delete_collection,
            add_images_to_collection,
            remove_images_from_collection,
            list_collections,
            list_collection_image_ids,
            get_develop_preview,
            get_develop_full_preview,
            get_graded_develop_preview,
            get_soft_proof_preview,
            get_print_ready_images,
            export_print_pdf,
            get_edit_stack,
            set_edit_stack,
            lookup_lens_profile,
            get_history,
            restore_history_entry,
            preview_history_entry,
            add_snapshot,
            get_snapshots,
            restore_snapshot,
            preview_snapshot,
            preview_edit_stack,
            delete_snapshot,
            create_preset,
            list_presets,
            delete_preset,
            import_preset_file,
            export_preset_file,
            regenerate_thumbnail,
            export_images,
            get_backup_settings,
            update_backup_settings,
            get_storage_info,
            set_cache_dir,
            perform_catalog_backup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulsion-remove-images-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(dir: &std::path::Path, name: &str) {
        std::fs::write(dir.join(name), b"x").unwrap();
    }

    /// The bug this test pins: a plain `starts_with(image_id.to_string())`
    /// prefix check (an earlier draft of this cleanup) would delete image 5's
    /// files AND wrongly sweep up "50.jpg"/"512-....jpg" -- a different
    /// image's thumbnails -- because "5" is a literal string prefix of "50"
    /// and "512". Removing image 5 must delete exactly its own plain and
    /// edited-thumbnail-variant files, and touch nothing belonging to a
    /// numerically-prefixed sibling id.
    #[test]
    fn removing_a_thumbnail_does_not_sweep_up_a_numerically_prefixed_sibling_id() {
        let dir = test_dir("thumbnail-prefix-safety");
        touch(&dir, "5.jpg");
        touch(&dir, "5-aaaa1111.jpg");
        touch(&dir, "5-bbbb2222.jpg");
        touch(&dir, "50.jpg");
        touch(&dir, "512-cccc3333.jpg");

        let plain_name = "5.jpg".to_string();
        let edited_prefix = "5-".to_string();
        remove_matching_files(&dir, |name| name == plain_name || name.starts_with(&edited_prefix));

        let remaining: std::collections::BTreeSet<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            remaining,
            ["50.jpg", "512-cccc3333.jpg"].into_iter().map(String::from).collect(),
            "only image 5's own files should be removed"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A removed image's Develop preview cache can have several files under
    /// the same content_hash -- the draft tier, the lazily-built full-res
    /// tier, and one graded/soft-proof variant per distinct edit stack or
    /// proof profile it was ever viewed with. All of them must go; only a
    /// different image's (different-hash) files must survive.
    #[test]
    fn removing_a_preview_deletes_every_variant_under_its_content_hash() {
        let dir = test_dir("preview-hash-variants");
        let hash = "a".repeat(64);
        let other_hash = "b".repeat(64);
        touch(&dir, &format!("{hash}.png"));
        touch(&dir, &format!("{hash}_full.png"));
        touch(&dir, &format!("{hash}_abcd1234_graded.png"));
        touch(&dir, &format!("{hash}_abcd1234_proof_deadbeef.png"));
        touch(&dir, &format!("{other_hash}.png"));
        touch(&dir, &format!("{other_hash}_full.png"));

        remove_matching_files(&dir, |name| name.starts_with(&hash));

        let remaining: std::collections::BTreeSet<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            remaining,
            [format!("{other_hash}.png"), format!("{other_hash}_full.png")]
                .into_iter()
                .collect(),
            "every variant under the removed image's hash should be gone; the other image's untouched"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `remove_images` calls this against `thumbnail_dir`/`previews_dir`,
    /// neither of which is guaranteed to exist yet (e.g. an image removed
    /// before any thumbnail was ever generated) -- must be a quiet no-op,
    /// not a panic or a surfaced command error.
    #[test]
    fn matching_against_a_nonexistent_directory_is_a_quiet_no_op() {
        let dir = std::env::temp_dir().join("emulsion-remove-images-test-does-not-exist");
        let _ = std::fs::remove_dir_all(&dir);
        remove_matching_files(&dir, |_| true);
    }
}
