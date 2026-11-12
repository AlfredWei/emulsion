//! The page's way to the eye check (RFC-0031 §8.15): "what do the eyes of these photographs look like?", asked for the members of a
//! similar group when the user opens it. Rows already stored (of the current rules version) are served as they are; the rest are
//! measured now, one photograph at a time on the full-resolution decode, and stored. Nothing here runs on its own.

use crate::blink_mesh::EyeMesh;
use crate::blink_models;
use crate::blink_pass::{analyse_image, counts, BlinkCounts, FaceBlink, ALGO_VERSION};
use crate::face_detect::FaceDetector;
use crate::{face_models, resolve_cache_root, resolve_face_models_dir, source_decode, AppState};
use serde::Serialize;
use tauri::{AppHandle, State};

/// What the eye check said of one photograph.
#[derive(Debug, Serialize)]
pub struct BlinkPhoto {
    pub image_id: i64,
    #[serde(flatten)]
    pub counts: BlinkCounts,
    /// Per face, in fractions of the photograph.
    pub boxes: Vec<FaceBlink>,
}

#[derive(Debug, Serialize)]
pub struct BlinkResult {
    /// False when a model the check needs is not there: `reason` is "mesh_model" (the bundled file is missing) or "face_models"
    /// (the face detector has not been fetched yet, which the first face scan does).
    pub available: bool,
    pub reason: String,
    pub photos: Vec<BlinkPhoto>,
}

fn unavailable(reason: &str) -> BlinkResult {
    BlinkResult { available: false, reason: reason.into(), photos: Vec::new() }
}

#[tauri::command]
pub async fn blink_for_images(app: AppHandle, state: State<'_, AppState>, image_ids: Vec<i64>) -> Result<BlinkResult, String> {
    let catalog = state.catalog.clone();
    let blink_dir = resolve_cache_root(&app, &catalog)?.join(blink_models::DIR);
    let face_dir = resolve_face_models_dir(&app, &catalog)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::bundled_models::seed_from_app(&app, &blink_dir, &crate::bundled_models::BLINK);
        let Some(mesh_path) = blink_models::ready_path(&blink_dir) else { return Ok(unavailable("mesh_model")) };
        let Some(yunet) = face_models::cached_detector(&face_dir) else { return Ok(unavailable("face_models")) };

        let have: Vec<i64> = {
            let c = catalog.lock().map_err(|e| e.to_string())?;
            c.get_cull_blink(&image_ids, ALGO_VERSION).map_err(|e| e.to_string())?.into_iter().map(|(id, ..)| id).collect()
        };
        let todo: Vec<i64> = image_ids.iter().copied().filter(|id| !have.contains(id)).collect();
        if !todo.is_empty() {
            let detector = FaceDetector::load(&yunet).map_err(|e| e.to_string())?;
            let mesh = EyeMesh::load(&mesh_path).map_err(|e| e.to_string())?;
            for id in todo {
                let path = catalog.lock().map_err(|e| e.to_string())?.get_image_path(id).map_err(|e| e.to_string())?;
                let Some(path) = path else { continue };
                // A decoder or model that panics on a hostile file costs that one photograph, not the call.
                let faces = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Option<Vec<FaceBlink>> {
                    let decoded = source_decode::decode_preview(std::path::Path::new(&path)).ok()?;
                    let image = image::RgbImage::from_raw(decoded.width, decoded.height, decoded.rgb)?;
                    analyse_image(&detector, &mesh, &image).ok()
                }))
                .unwrap_or_else(|_| {
                    eprintln!("eye check panicked for {path}");
                    None
                });
                if let Some(faces) = faces {
                    catalog.lock().map_err(|e| e.to_string())?.set_cull_blink(id, &faces, ALGO_VERSION).map_err(|e| e.to_string())?;
                }
            }
        }
        let rows = catalog.lock().map_err(|e| e.to_string())?.get_cull_blink(&image_ids, ALGO_VERSION).map_err(|e| e.to_string())?;
        let photos = rows.into_iter().map(|(image_id, _, boxes)| BlinkPhoto { image_id, counts: counts(&boxes), boxes }).collect();
        Ok(BlinkResult { available: true, reason: String::new(), photos })
    })
    .await
    .map_err(|e| e.to_string())?
}
