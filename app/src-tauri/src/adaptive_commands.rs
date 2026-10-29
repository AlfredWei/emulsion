//! Tauri command for the adaptive-preset region resolvers (RFC-0035 B1). Thin: the resolvers are pure functions in
//! `adaptive_regions`; this reads the stored face rows and the unedited, uncropped Develop preview (the frame every
//! mask uses) and runs one of them. The History panel's Adaptive block calls it through `actions/adaptiveActions.js` (B2).

use crate::adaptive_regions::{self, FaceBox, PersonCut, PersonDecoder, RegionResult};
use crate::ai::protocol::{Candidate, ModelPaths, Prompt, Request};
use crate::ai::supervisor::AiHelper;
use crate::segment_commands::{models_dir, DECODE_TIMEOUT, PREPARE_TIMEOUT};
use crate::segment_models;
use crate::{preview_cache, resolve_cache_root, AppState};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, State};

/// The click-select model as `resolve_person` uses it: the AI helper, with this photograph's embedding already prepared.
struct HelperModel {
    helper: Arc<AiHelper>,
    models: ModelPaths,
    content_hash: String,
}

impl PersonDecoder for HelperModel {
    fn cut(&mut self, clicks: &[(f32, f32)]) -> Result<PersonCut, String> {
        let prompts: Vec<Prompt> = clicks.iter().map(|&(x, y)| Prompt { x, y, positive: true }).collect();
        let v = self
            .helper
            .request(
                Request::SegmentDecode { models: self.models.clone(), content_hash: self.content_hash.clone(), prompts, refine: None },
                DECODE_TIMEOUT,
            )
            .map_err(|e| e.to_string())?;
        let candidates: Vec<Candidate> = serde_json::from_value(v).map_err(|e| e.to_string())?;
        // Best predicted IoU first (segment_decode's contract).
        let best = candidates.first().ok_or_else(|| "the model returned no cut".to_string())?;
        Ok(PersonCut { iou: best.iou, logits: adaptive_regions::decode_logits(&best.logits_png)? })
    }
}

/// Resolves `region` (`"shadows"`, `"face"`, `"person"` or `"sky"`) for one photograph. The face and person resolvers
/// read the stored `faces` rows, so the caller scans first (`faces_scanned` on the image) -- no rows reads as "no face".
#[tauri::command]
pub async fn resolve_adaptive_region(
    app: AppHandle,
    state: State<'_, AppState>,
    helper: State<'_, Arc<AiHelper>>,
    image_id: i64,
    path: String,
    content_hash: String,
    region: String,
) -> Result<RegionResult, String> {
    let faces = if region == "face" || region == "person" {
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
    let person_models = if region == "person" { Some(models_dir(&app, &state)?) } else { None };
    let helper = helper.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let preview = preview_cache::ensure_develop_preview_for_hash(std::path::Path::new(&path), &content_hash, &previews_dir)
            .map_err(|e| e.user_message())?;
        match region.as_str() {
            "face" => Ok(adaptive_regions::resolve_faces(&faces, (preview.width, preview.height))),
            "person" => {
                // Faces to clicks (RFC-0033 section 3.1): the encoder runs once for the photograph, then one decode per person.
                let dir: PathBuf = person_models.ok_or_else(|| "no model folder".to_string())?;
                let paths = segment_models::ready_paths(&dir).ok_or_else(|| "the click-select model is not installed".to_string())?;
                let models = ModelPaths { encoder: paths.encoder, decoder: paths.decoder };
                helper
                    .request(
                        Request::SegmentPrepare { models: models.clone(), content_hash: content_hash.clone(), image_path: PathBuf::from(&preview.path) },
                        PREPARE_TIMEOUT,
                    )
                    .map_err(|e| e.to_string())?;
                adaptive_regions::resolve_person(&faces, (preview.width, preview.height), &mut HelperModel { helper, models, content_hash })
            }
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
