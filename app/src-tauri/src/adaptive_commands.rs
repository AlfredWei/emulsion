//! Tauri command for the adaptive-preset region resolvers (RFC-0035 B1). Thin: the resolvers are pure functions in
//! `adaptive_regions`; this reads the stored face rows and the unedited, uncropped Develop preview (the frame every
//! mask uses) and runs one of them. The History panel's Adaptive block calls it through `actions/adaptiveActions.js` (B2).

use crate::adaptive_regions::{self, FaceBox, RegionResult};
use crate::{preview_cache, resolve_cache_root, AppState};
use tauri::{AppHandle, State};

/// Resolves `region` (`"shadows"`, `"face"` or `"sky"`) for one photograph. The face resolver reads the stored
/// `faces` rows, so the caller scans first (`faces_scanned` on the image) -- no rows reads as "no face".
#[tauri::command]
pub async fn resolve_adaptive_region(
    app: AppHandle,
    state: State<'_, AppState>,
    image_id: i64,
    path: String,
    content_hash: String,
    region: String,
) -> Result<RegionResult, String> {
    let faces = if region == "face" {
        let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
        catalog
            .get_faces_for_image(image_id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|f| FaceBox { x: f.bbox_x, y: f.bbox_y, w: f.bbox_w, h: f.bbox_h, score: None })
            .collect()
    } else {
        Vec::new()
    };
    let previews_dir = resolve_cache_root(&app, &state.catalog)?.join("previews");
    tauri::async_runtime::spawn_blocking(move || {
        let preview = preview_cache::ensure_develop_preview_for_hash(std::path::Path::new(&path), &content_hash, &previews_dir)
            .map_err(|e| e.user_message())?;
        match region.as_str() {
            "face" => Ok(adaptive_regions::resolve_faces(&faces, (preview.width, preview.height))),
            "shadows" | "sky" => {
                let img = image::open(&preview.path).map_err(|e| e.to_string())?.to_rgb8();
                Ok(if region == "sky" { adaptive_regions::resolve_sky(&img) } else { adaptive_regions::resolve_shadows(&img) })
            }
            other => Err(format!("unknown adaptive region \"{other}\"")),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
