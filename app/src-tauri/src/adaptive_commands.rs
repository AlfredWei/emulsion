//! Tauri command for the adaptive-preset region resolvers (RFC-0035 B1). Thin: the resolvers are pure functions in
//! `adaptive_regions`; this reads the stored face rows and the unedited, uncropped Develop preview (the frame every
//! mask uses) and runs one of them. The History panel's Adaptive block calls it through `actions/adaptiveActions.js` (B2).

use crate::adaptive_regions::{self, FaceBox, Cut, ClickModel, Outcome, RegionResult};
use crate::ai::protocol::{BBox, Candidate, ModelPaths, PersonBox, Prompt, Request};
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

impl HelperModel {
    fn decode(&self, prompts: Vec<Prompt>, bbox: Option<BBox>) -> Result<Cut, String> {
        let v = self
            .helper
            .request(
                Request::SegmentDecode { models: self.models.clone(), content_hash: self.content_hash.clone(), prompts, refine: None, bbox },
                DECODE_TIMEOUT,
            )
            .map_err(|e| e.to_string())?;
        let candidates: Vec<Candidate> = serde_json::from_value(v).map_err(|e| e.to_string())?;
        // Best predicted IoU first (segment_decode's contract).
        let best = candidates.first().ok_or_else(|| "the model returned no cut".to_string())?;
        Ok(Cut { iou: best.iou, logits: adaptive_regions::decode_logits(&best.logits_png)? })
    }
}

impl ClickModel for HelperModel {
    fn cut(&mut self, clicks: &[(f32, f32, bool)]) -> Result<Cut, String> {
        self.decode(clicks.iter().map(|&(x, y, positive)| Prompt { x, y, positive }).collect(), None)
    }
    fn cut_box(&mut self, bbox: &BBox) -> Result<Cut, String> {
        self.decode(Vec::new(), Some(*bbox))
    }
}

/// A detection takes about a second on a laptop CPU (D-FINE medium, 640 x 640); hung means far longer.
const DETECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// RFC-0037: the people the detector sees in this photograph, or why it could not look (no model installed, helper failure).
/// Callers treat every error as "no detector" and keep the face-only answer: a person region must never fail because the
/// extra eyes did.
fn detect_people(helper: &AiHelper, dir: &std::path::Path, preview_path: &str) -> Result<Vec<PersonBox>, String> {
    let model = crate::detect_models::ready_path(dir).ok_or_else(|| "the person detector is not installed".to_string())?;
    let v = helper
        .request(
            Request::DetectPeople { model, image_path: PathBuf::from(preview_path), min_score: adaptive_regions::PERSON_DETECT_MIN_SCORE },
            DETECT_TIMEOUT,
        )
        .map_err(|e| e.to_string())?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// Starts the click-select model on this photograph (the encoder runs once here); the returned model decodes clicks.
fn prepare_model(helper: Arc<AiHelper>, dir: &std::path::Path, content_hash: String, preview_path: &str) -> Result<HelperModel, String> {
    let paths = segment_models::ready_paths(dir).ok_or_else(|| "the click-select model is not installed".to_string())?;
    let models = ModelPaths { encoder: paths.encoder, decoder: paths.decoder };
    helper
        .request(Request::SegmentPrepare { models: models.clone(), content_hash: content_hash.clone(), image_path: PathBuf::from(preview_path) }, PREPARE_TIMEOUT)
        .map_err(|e| e.to_string())?;
    Ok(HelperModel { helper, models, content_hash })
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
            .map(|f| FaceBox { x: f.bbox_x, y: f.bbox_y, w: f.bbox_w, h: f.bbox_h, score: f.score })
            .collect()
    } else {
        Vec::new()
    };
    let previews_dir = resolve_cache_root(&app, &state.catalog)?.join("previews");
    let person_models = if region == "person" || region == "sky" { Some(models_dir(&app, &state)?) } else { None };
    if region == "person" {
        // The detector ships in the installer beside the Select Subject files; make sure it is installed (a no-op otherwise).
        if let Some(dir) = &person_models {
            crate::bundled_models::seed_from_app(&app, dir, &crate::bundled_models::DETECT);
        }
    }
    let helper = helper.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let preview = preview_cache::ensure_develop_preview_for_hash(std::path::Path::new(&path), &content_hash, &previews_dir)
            .map_err(|e| e.user_message())?;
        match region.as_str() {
            "face" => Ok(adaptive_regions::resolve_faces(&faces, (preview.width, preview.height))),
            "person" => {
                // Faces to clicks (RFC-0033 section 3.1): the encoder runs once for the photograph, then one decode per person.
                let dir = person_models.ok_or_else(|| "no model folder".to_string())?;
                let mut model = prepare_model(helper.clone(), &dir, content_hash, &preview.path)?;
                adaptive_regions::trace(&format!("photo {path}"));
                let by_face = adaptive_regions::resolve_person(&faces, (preview.width, preview.height), &mut model)?;
                if by_face.outcome != Outcome::NotFound {
                    return Ok(by_face);
                }
                // Nobody with a usable face: look for people by their shape (RFC-0037). Without the detector model, or if it
                // fails, the face-only answer stands.
                match detect_people(&helper, &dir, &preview.path) {
                    Ok(people) => {
                        adaptive_regions::trace(&format!("person detector found {} candidate(s)", people.len()));
                        let by_shape = adaptive_regions::resolve_person_by_detection(&people, &mut model)?;
                        Ok(if by_shape.outcome == Outcome::NotFound { by_face } else { by_shape })
                    }
                    Err(e) => {
                        adaptive_regions::trace(&format!("person detector unavailable: {e}"));
                        Ok(by_face)
                    }
                }
            }
            "shadows" | "sky" => {
                let img = image::open(&preview.path).map_err(|e| e.to_string())?.to_rgb8();
                if region == "shadows" {
                    return Ok(adaptive_regions::resolve_shadows(&img));
                }
                // The sky is outlined by the click-select model when it is installed; without it (or if it fails) the colour
                // rule's own estimate stands, which is flagged for a look.
                let refined = person_models.as_deref().and_then(|dir| prepare_model(helper, dir, content_hash, &preview.path).ok());
                Ok(match refined {
                    Some(mut model) => adaptive_regions::resolve_sky_with_model(&img, &mut model),
                    None => adaptive_regions::resolve_sky(&img),
                })
            }
            other => Err(format!("unknown adaptive region \"{other}\"")),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
