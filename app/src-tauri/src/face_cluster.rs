//! Face clustering (M5 Slice 6, RFC-0005 §3.5): groups face-embedding
//! vectors into clusters, each cluster becoming a "person" the catalog
//! layer can persist as a `people` row. Pure vector math, no catalog/
//! SQLite dependency and no ONNX/model dependency -- same "this module
//! only knows about its own narrow input" split as `panorama_merge.rs`
//! (pixels/homographies, no catalog) and `hdr_merge.rs` (pixels/alignment,
//! no catalog). The caller (not yet written -- blocked on RFC-0005 §5's
//! model choice) is responsible for turning `embedding` blobs read from
//! `faces` rows into `&[f32]`, calling `assign_face` per new face, and
//! writing the resulting cluster membership back as `faces.person_id`.
//!
//! Hand-rolled rather than pulling in a clustering crate, matching this
//! codebase's precedent (RFC-0003 MTB alignment, RFC-0004 homography) of
//! hand-rolling compact, directly-testable algorithms instead of adding a
//! dependency for something this small. The algorithm itself is the
//! **incremental centroid-threshold** scheme RFC-0005 §3.5 settled on: a
//! new embedding is compared only against each existing cluster's
//! centroid (not every member embedding), and joins the nearest cluster
//! under the distance threshold or else seeds a new singleton cluster --
//! `O(faces × clusters)` per pass, not `O(faces²)`.

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ClusterError {
    #[error("embedding must not be empty")]
    EmptyEmbedding,
    #[error("embedding has {0} dimensions, but existing clusters have {1}")]
    DimensionMismatch(usize, usize),
}

/// One cluster's running state: its centroid (the mean of every member
/// embedding assigned to it so far) and how many members contributed to
/// that mean. Deliberately does NOT hold the member embeddings or face
/// ids themselves -- the incremental mean update in `assign_face` never
/// needs to revisit a past member, and which face ids belong to which
/// cluster is the catalog's job (`faces.person_id`), not this module's.
#[derive(Debug, Clone, PartialEq)]
pub struct Cluster {
    pub centroid: Vec<f32>,
    pub member_count: u32,
}

impl Cluster {
    fn seed(embedding: &[f32]) -> Self {
        Cluster { centroid: embedding.to_vec(), member_count: 1 }
    }

    /// Rebuilds a cluster's centroid from a person's already-stored member
    /// embeddings (`Catalog::get_clusterable_faces`, grouped by
    /// `person_id`) -- the real-world counterpart to `seed`/`absorb`,
    /// which only ever see one new embedding at a time and have no reason
    /// to be `pub` themselves. `None` for an empty slice (a person with no
    /// remaining non-excluded faces has no cluster to rebuild).
    pub fn from_members(embeddings: &[Vec<f32>]) -> Option<Self> {
        let (first, rest) = embeddings.split_first()?;
        let mut cluster = Self::seed(first);
        for embedding in rest {
            cluster.absorb(embedding);
        }
        Some(cluster)
    }

    /// Streaming mean update: `new_mean = old_mean + (x - old_mean) / (n + 1)`,
    /// applied per dimension. Gives the exact mean of all members assigned
    /// so far without ever storing them.
    fn absorb(&mut self, embedding: &[f32]) {
        let n = self.member_count as f32;
        for (c, &x) in self.centroid.iter_mut().zip(embedding) {
            *c += (x - *c) / (n + 1.0);
        }
        self.member_count += 1;
    }
}

/// Cosine distance (`1 - cosine_similarity`), in `[0, 2]`. Zero-vector
/// inputs (which have no defined direction) are treated as maximally
/// distant from everything, including each other -- there is no real
/// embedding model in this codebase's pipeline that could ever produce
/// one (RFC-0005 §3.3's embedding networks are trained so their output is
/// never all-zero), so this is an unreachable-in-practice edge case
/// handled just to keep the function total rather than a hidden `NaN`.
fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 2.0;
    }
    1.0 - (dot / (norm_a * norm_b))
}

/// Assigns one face embedding to the nearest existing cluster in
/// `clusters`, or seeds a new singleton cluster if none is within
/// `threshold` cosine distance of it. Returns the index into `clusters`
/// of whichever cluster the embedding ended up in.
///
/// A tie (two clusters equally near) resolves to the first one found
/// (`clusters` is scanned in order) -- an arbitrary but deterministic
/// choice; no real embedding model produces exact ties in practice.
///
/// `threshold` is intentionally a caller-supplied parameter, not a
/// constant here: RFC-0005 §3.5 names the right value as an empirical
/// tuning question that needs a real, varied test set to settle, and
/// flags it as a possible future per-catalog setting -- this function
/// stays agnostic to whatever that number turns out to be.
pub fn assign_face(clusters: &mut Vec<Cluster>, embedding: &[f32], threshold: f32) -> Result<usize, ClusterError> {
    if embedding.is_empty() {
        return Err(ClusterError::EmptyEmbedding);
    }
    if let Some(existing) = clusters.first() {
        if existing.centroid.len() != embedding.len() {
            return Err(ClusterError::DimensionMismatch(embedding.len(), existing.centroid.len()));
        }
    }

    let nearest = clusters
        .iter()
        .enumerate()
        .map(|(i, c)| (i, cosine_distance(&c.centroid, embedding)))
        .min_by(|(_, d1), (_, d2)| d1.partial_cmp(d2).expect("cosine_distance never produces NaN"));

    match nearest {
        Some((i, distance)) if distance <= threshold => {
            clusters[i].absorb(embedding);
            Ok(i)
        }
        _ => {
            clusters.push(Cluster::seed(embedding));
            Ok(clusters.len() - 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trio of near-identical vectors (small per-dimension jitter, the
    /// shape of the same person's face across a few photos) must all land
    /// in one cluster.
    #[test]
    fn tight_cluster_of_near_identical_vectors_merges_into_one() {
        let mut clusters = Vec::new();
        let base = [1.0, 0.0, 0.0, 0.0];
        let jittered = [
            [1.0, 0.0, 0.0, 0.0],
            [0.98, 0.02, 0.0, 0.0],
            [0.99, -0.01, 0.01, 0.0],
        ];
        let mut assigned = Vec::new();
        for v in &jittered {
            assigned.push(assign_face(&mut clusters, v, 0.05).unwrap());
        }
        assert_eq!(clusters.len(), 1, "near-identical vectors must stay in one cluster, got {clusters:?}");
        assert_eq!(assigned, vec![0, 0, 0]);
        assert_eq!(clusters[0].member_count, 3);
        // Sanity: centroid distance to the shared direction stays tiny.
        assert!(cosine_distance(&clusters[0].centroid, &base) < 0.01);
    }

    /// Two clearly-separated directions (orthogonal vectors -- as
    /// different as two face embeddings get) must seed two clusters, not
    /// merge into one, at a realistic threshold.
    #[test]
    fn clearly_separated_groups_stay_in_separate_clusters() {
        let mut clusters = Vec::new();
        let person_a = [[1.0, 0.0], [0.99, 0.01]];
        let person_b = [[0.0, 1.0], [0.01, 0.99]];

        let mut assigned = Vec::new();
        for v in person_a.iter().chain(person_b.iter()) {
            assigned.push(assign_face(&mut clusters, v, 0.1).unwrap());
        }

        assert_eq!(clusters.len(), 2, "orthogonal groups must not merge, got {clusters:?}");
        assert_eq!(assigned, vec![0, 0, 1, 1], "each group must consistently join its own cluster");
    }

    /// A pair placed exactly at the threshold boundary: this function
    /// joins on `distance <= threshold` (inclusive), so a threshold equal
    /// to the pair's actual distance must merge, and a threshold one ULP
    /// below that distance must not. Locks down the boundary as a real,
    /// tested contract rather than leaving `<` vs `<=` an unstated
    /// implementation detail. Uses the pair's own computed distance as the
    /// threshold (rather than aiming for a round number like `0.1` via a
    /// `acos`/`cos` round-trip) so the comparison is bit-exact, not
    /// dependent on trig rounding landing on the right side of the line.
    #[test]
    fn borderline_pair_at_exact_threshold_merges_but_just_past_it_does_not() {
        let seed = [1.0, 0.0];
        let candidate = [0.9, 0.4];
        let distance = cosine_distance(&seed, &candidate);

        let mut clusters = vec![Cluster::seed(&seed)];
        let idx = assign_face(&mut clusters, &candidate, distance).unwrap();
        assert_eq!(idx, 0, "a threshold equal to the actual distance must merge (inclusive boundary)");
        assert_eq!(clusters.len(), 1);

        let just_below = f32::from_bits(distance.to_bits() - 1);
        let mut clusters = vec![Cluster::seed(&seed)];
        let idx = assign_face(&mut clusters, &candidate, just_below).unwrap();
        assert_eq!(idx, 1, "a threshold one ULP below the actual distance must seed a new cluster");
        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn empty_embedding_is_rejected() {
        let mut clusters = Vec::new();
        assert_eq!(assign_face(&mut clusters, &[], 0.1), Err(ClusterError::EmptyEmbedding));
    }

    #[test]
    fn mismatched_dimension_is_rejected() {
        let mut clusters = vec![Cluster::seed(&[1.0, 0.0, 0.0])];
        assert_eq!(
            assign_face(&mut clusters, &[1.0, 0.0], 0.1),
            Err(ClusterError::DimensionMismatch(2, 3))
        );
    }

    #[test]
    fn incremental_centroid_matches_the_true_mean_of_all_members() {
        let mut clusters = Vec::new();

        // Three members with a known exact mean, all mutually close
        // enough (threshold generous) to land in one cluster.
        let vectors = [[1.0, 0.0, 0.0], [0.9, 0.1, 0.0], [0.95, -0.05, 0.05]];
        for v in &vectors {
            assign_face(&mut clusters, v, 1.0).unwrap();
        }
        assert_eq!(clusters.len(), 1);
        let expected_mean = [
            (1.0 + 0.9 + 0.95) / 3.0,
            (0.0 + 0.1 - 0.05) / 3.0,
            (0.0 + 0.0 + 0.05) / 3.0,
        ];
        for (got, want) in clusters[0].centroid.iter().zip(expected_mean) {
            assert!((got - want).abs() < 1e-6, "got {got}, want {want}");
        }
    }

    #[test]
    fn from_members_matches_the_true_mean() {
        let members = vec![vec![1.0, 0.0, 0.0], vec![0.9, 0.1, 0.0], vec![0.95, -0.05, 0.05]];
        let cluster = Cluster::from_members(&members).unwrap();
        assert_eq!(cluster.member_count, 3);
        let expected_mean = [(1.0 + 0.9 + 0.95) / 3.0, (0.0 + 0.1 - 0.05) / 3.0, (0.0 + 0.0 + 0.05) / 3.0];
        for (got, want) in cluster.centroid.iter().zip(expected_mean) {
            assert!((got - want).abs() < 1e-6, "got {got}, want {want}");
        }
    }

    #[test]
    fn from_members_is_none_for_an_empty_slice() {
        assert!(Cluster::from_members(&[]).is_none());
    }
}
