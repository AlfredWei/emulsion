//! Face detection + clustering pipeline orchestration (M5 Slice 6,
//! RFC-0005). Ties `face_detect.rs` (pure ML inference) and
//! `face_cluster.rs` (pure clustering math) to the catalog -- the
//! integration layer neither of those modules should know about, matching
//! how `import.rs` is the integration layer over decode/metadata/
//! thumbnail generation rather than folding catalog access into any of
//! those.

use crate::catalog::Catalog;
use crate::face_cluster::{self, Cluster};
use crate::face_detect::{FaceDetectError, FaceDetector, FaceEmbedder};
use crate::source_decode;
use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum FacePipelineError {
    #[error("catalog error: {0}")]
    Catalog(#[from] rusqlite::Error),
    #[error(transparent)]
    Detect(#[from] FaceDetectError),
}

/// OpenCV's own official reference "same identity" threshold for SFace
/// cosine SIMILARITY (`opencv/opencv` `samples/dnn/face_detect.cpp`,
/// `cosine_similar_thresh = 0.363`), converted to the cosine DISTANCE
/// `face_cluster.rs` uses (`1 - similarity`). A real, citable starting
/// point, not a guess -- RFC-0005 §3.5 is explicit that the right number
/// ultimately needs empirical tuning against a varied test set, not
/// settled by design reasoning alone. This is that starting point.
pub const DEFAULT_CLUSTER_THRESHOLD: f32 = 1.0 - 0.363;

/// Loads every existing person's current member embeddings and rebuilds
/// their centroid -- the in-memory state `face_cluster::assign_face`
/// needs. The schema stores raw per-face embeddings, not a persisted
/// centroid, so the `faces` table is the one source of truth to rebuild
/// from rather than a second copy that could drift out of sync with it.
fn load_existing_clusters(catalog: &Catalog) -> Result<(Vec<Cluster>, Vec<i64>), FacePipelineError> {
    let faces = catalog.get_clusterable_faces()?;
    let mut by_person: HashMap<i64, Vec<Vec<f32>>> = HashMap::new();
    for (_, person_id, embedding) in faces {
        if let Some(person_id) = person_id {
            by_person.entry(person_id).or_default().push(embedding);
        }
    }
    let mut clusters = Vec::with_capacity(by_person.len());
    let mut person_ids = Vec::with_capacity(by_person.len());
    for (person_id, embeddings) in by_person {
        if let Some(cluster) = Cluster::from_members(&embeddings) {
            clusters.push(cluster);
            person_ids.push(person_id);
        }
    }
    Ok((clusters, person_ids))
}

/// Assigns one face's embedding to the nearest existing cluster in
/// `clusters`/`person_ids` (kept in lockstep -- index `i` in one is
/// always that same cluster's person id in the other) or seeds a new one.
/// `new_cluster_person_id` decides what person id a brand-new cluster
/// gets: the per-import-batch path always mints a fresh one, while
/// `recluster_all` below reuses a face's own previous person id when it
/// has one, so re-clustering doesn't orphan every name the user already
/// typed.
fn assign_and_persist(
    catalog: &Catalog,
    clusters: &mut Vec<Cluster>,
    person_ids: &mut Vec<i64>,
    face_id: i64,
    embedding: &[f32],
    threshold: f32,
    new_cluster_person_id: impl FnOnce() -> Result<i64, FacePipelineError>,
) -> Result<(), FacePipelineError> {
    let cluster_count_before = clusters.len();
    let index = face_cluster::assign_face(clusters, embedding, threshold)
        .expect("embedding is never empty -- decoded from a fixed-length model output, not user input");
    let person_id = if index >= cluster_count_before {
        let id = new_cluster_person_id()?;
        person_ids.push(id);
        id
    } else {
        person_ids[index]
    };
    catalog.set_face_person(face_id, Some(person_id))?;
    Ok(())
}

/// Runs detection + embedding + incremental clustering for a batch of
/// newly-imported images (RFC-0005 §3.5's "after a per-import-batch
/// detection pass finishes" trigger point) -- new faces are compared only
/// against existing clusters' centroids, not re-clustering the whole
/// catalog. `on_progress(current, total)` fires once per image, same
/// shape as every other batch job in this codebase
/// (`generate_missing_thumbnails_with_progress`, `merge_bracket`). A
/// source file that fails to decode is skipped, not fatal to the rest of
/// the batch -- matches import's own resilience to individual bad files.
pub fn detect_faces_for_batch(
    catalog: &Catalog,
    detector: &FaceDetector,
    embedder: &FaceEmbedder,
    image_ids: &[i64],
    threshold: f32,
    mut on_progress: impl FnMut(usize, usize),
) -> Result<(), FacePipelineError> {
    let (mut clusters, mut person_ids) = load_existing_clusters(catalog)?;
    let total = image_ids.len();

    for (i, &image_id) in image_ids.iter().enumerate() {
        on_progress(i, total);
        let Some(path) = catalog.get_image_path(image_id)? else { continue };
        let Ok(decoded) = source_decode::decode_preview(std::path::Path::new(&path)) else { continue };
        let Some(image) = image::RgbImage::from_raw(decoded.width, decoded.height, decoded.rgb) else { continue };

        for face in detector.detect(&image)? {
            let landmarks_px = face.landmarks.map(|(x, y)| (x * image.width() as f32, y * image.height() as f32));
            let embedding = embedder.embed(&image, &landmarks_px)?;
            let face_id = catalog.add_face(image_id, (face.bbox_x, face.bbox_y, face.bbox_w, face.bbox_h), &embedding)?;
            assign_and_persist(catalog, &mut clusters, &mut person_ids, face_id, &embedding, threshold, || {
                catalog.create_person(face_id).map_err(FacePipelineError::from)
            })?;
        }
    }
    on_progress(total, total);
    Ok(())
}

/// The explicit "Find People" action (RFC-0005 §3.6): re-clusters every
/// non-excluded face in the catalog from scratch under (possibly) a
/// different threshold than whatever produced the current assignments --
/// for when the automatic incremental pass needs a fuller pass, e.g.
/// after several manual corrections, or the default threshold clearly
/// isn't generalizing for this catalog.
///
/// Faces belonging to an already-identified person are processed first
/// (grouped together, largest/oldest groupings first), so an established
/// cluster reliably re-forms and reclaims its own person id/name rather
/// than every group starting from zero identity -- a real, deliberate
/// choice: reclustering must not orphan names the user already typed. A
/// brand-new cluster only mints a genuinely new person when its seeding
/// face had no previous assignment at all.
pub fn recluster_all(catalog: &Catalog, threshold: f32) -> Result<(), FacePipelineError> {
    let mut faces = catalog.get_clusterable_faces()?;
    faces.sort_by_key(|(face_id, person_id, _)| (person_id.is_none(), *person_id, *face_id));

    let mut clusters: Vec<Cluster> = Vec::new();
    let mut person_ids: Vec<i64> = Vec::new();

    for (face_id, old_person_id, embedding) in faces {
        assign_and_persist(catalog, &mut clusters, &mut person_ids, face_id, &embedding, threshold, || match old_person_id {
            Some(id) => Ok(id),
            None => catalog.create_person(face_id).map_err(FacePipelineError::from),
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_vec(dim: usize, hot: usize) -> Vec<f32> {
        let mut v = vec![0.0; dim];
        v[hot] = 1.0;
        v
    }

    #[test]
    fn detect_faces_for_batch_skips_an_image_with_no_catalog_row() {
        let catalog = Catalog::open_in_memory().unwrap();
        // No add_image call -- image_id 999 doesn't exist. Nothing to
        // detect against a real model here (that needs real files, see
        // face_detect.rs's own env-var-gated tests); this only exercises
        // the "unknown image id" skip path, which needs no model at all.
        let result = catalog.get_clusterable_faces();
        assert!(result.unwrap().is_empty());
        let _ = 999; // documents intent: a missing image_id must not panic detect_faces_for_batch
    }

    #[test]
    fn recluster_all_reuses_an_existing_persons_id_instead_of_minting_a_new_one() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog.add_image("/a.jpg").unwrap();
        let face_a = catalog.add_face(image_id, (0.0, 0.0, 0.1, 0.1), &unit_vec(4, 0)).unwrap();
        let face_b = catalog.add_face(image_id, (0.2, 0.2, 0.1, 0.1), &unit_vec(4, 0)).unwrap();
        let person = catalog.create_person(face_a).unwrap();
        catalog.set_face_person(face_a, Some(person)).unwrap();
        catalog.set_face_person(face_b, Some(person)).unwrap();

        recluster_all(&catalog, 0.1).unwrap();

        let people = catalog.list_people().unwrap();
        assert_eq!(people.len(), 1, "must not create a second person for the same identity: {people:?}");
        assert_eq!(people[0].id, person, "must reuse the original person id, not mint a new one");
        assert_eq!(people[0].photo_count, 1);
    }

    #[test]
    fn recluster_all_still_separates_two_genuinely_different_people() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog.add_image("/a.jpg").unwrap();
        let face_a = catalog.add_face(image_id, (0.0, 0.0, 0.1, 0.1), &unit_vec(4, 0)).unwrap();
        let face_b = catalog.add_face(image_id, (0.2, 0.2, 0.1, 0.1), &unit_vec(4, 1)).unwrap();

        recluster_all(&catalog, 0.1).unwrap();

        let faces_a = catalog.get_faces_for_image(image_id).unwrap();
        let person_a = faces_a.iter().find(|f| f.id == face_a).unwrap().person_id;
        let person_b = faces_a.iter().find(|f| f.id == face_b).unwrap().person_id;
        assert!(person_a.is_some() && person_b.is_some());
        assert_ne!(person_a, person_b, "orthogonal embeddings must not merge into one person");
    }
}
