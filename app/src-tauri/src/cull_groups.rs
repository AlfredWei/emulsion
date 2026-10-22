//! Similar-photo groups (RFC-0031 slice B): which photos are frames of the same moment, which one is the sharpest,
//! and which are soft next to it. A pure function over the stored measurements (`cull_analysis`) plus capture time and
//! camera; nothing is stored, so there is no stale state when photos come and go.
//!
//! The numbers behind the defaults are RFC-0031 §2 (synthetic edits only; real bursts tune them in slice D). A hash
//! alone must never decide (the hard negative, two shots at one venue, sits close), so a group needs all three of:
//! the same camera, capture times within a few seconds, and hashes within a distance.

use crate::cull::hamming;
use serde::Serialize;

/// A frame is compared with at most this many of the frames after it. A burst at 30 fps over the 18 s widest window
/// is 540 frames, but real windows are far smaller; the cap exists so a camera whose clock stuck on one second cannot
/// make 50,000 photos cost 50,000 x 50,000 comparisons.
const MAX_LOOKAHEAD: usize = 400;

/// One photo as the grouping sees it.
#[derive(Debug, Clone)]
pub struct GroupInput {
    pub image_id: i64,
    pub version_id: i64,
    /// Make and model; two photos only group on the same camera.
    pub camera: String,
    /// The catalog's `captured_at` text (ISO 8601); a photo without one is never grouped.
    pub captured_at: Option<String>,
    pub sharpness_tile: f64,
    pub dhash: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupParams {
    /// Two frames are neighbours when their capture times differ by at most this many seconds.
    pub max_seconds: i64,
    /// ... and their hashes by at most this many bits (of 256).
    pub max_distance: u32,
    /// A photo is soft when its score is under this share of the group's best.
    pub soft_ratio: f64,
    /// The best frame must beat the runner-up by this share, else the group has no clear best.
    pub best_margin: f64,
}

impl GroupParams {
    /// The one "Similarity" slider (0 = stricter, 1 = looser). 0.5 is the RFC's starting point: 10 s and 64 bits.
    pub fn from_similarity(similarity: f64) -> Self {
        let s = similarity.clamp(0.0, 1.0);
        Self {
            max_seconds: (2.0 + 16.0 * s).round() as i64,
            max_distance: (40.0 + 48.0 * s).round() as u32,
            soft_ratio: 0.5,
            best_margin: 0.1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupMember {
    pub image_id: i64,
    pub version_id: i64,
    /// This frame's score as a share of the group's best (1.0 = the best).
    pub relative_sharpness: f64,
    /// Under `soft_ratio` of the group's best.
    pub soft: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SimilarGroup {
    /// Chronological.
    pub members: Vec<GroupMember>,
    /// The sharpest frame, only when it clearly beats the runner-up.
    pub best_image_id: Option<i64>,
}

/// Seconds since the epoch of an ISO 8601 `YYYY-MM-DD[T ]HH:MM:SS...`, any offset or fraction ignored (the camera's
/// own clock is what is compared). `None` when it does not parse.
pub fn parse_capture_seconds(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() < 19 || !matches!(b[10], b'T' | b' ') || b[4] != b'-' || b[7] != b'-' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let num = |a: usize, z: usize| text.get(a..z)?.parse::<i64>().ok();
    let (y, m, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hh, mm, ss) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    // Days from civil (Howard Hinnant).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hh * 3600 + mm * 60 + ss)
}

struct Dsu(Vec<usize>);

impl Dsu {
    fn find(&mut self, mut x: usize) -> usize {
        while self.0[x] != x {
            self.0[x] = self.0[self.0[x]];
            x = self.0[x];
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[b] = a;
        }
    }
}

/// Groups of two or more frames, newest group first, members chronological.
pub fn similar_groups(inputs: &[GroupInput], params: &GroupParams) -> Vec<SimilarGroup> {
    // Only photos with a capture time; sorted by it (ties by id, so the result is deterministic).
    let mut items: Vec<(i64, &GroupInput)> =
        inputs.iter().filter_map(|i| Some((parse_capture_seconds(i.captured_at.as_deref()?)?, i))).collect();
    items.sort_by_key(|(secs, i)| (*secs, i.image_id));

    // Single linkage over the time window: a frame joins a group when it is close to ANY member (a pan drifts).
    let mut dsu = Dsu((0..items.len()).collect());
    for a in 0..items.len() {
        for b in (a + 1)..items.len().min(a + 1 + MAX_LOOKAHEAD) {
            if items[b].0 - items[a].0 > params.max_seconds {
                break;
            }
            if items[a].1.camera == items[b].1.camera && hamming(&items[a].1.dhash, &items[b].1.dhash) <= params.max_distance {
                dsu.union(a, b);
            }
        }
    }

    let mut by_root: std::collections::BTreeMap<usize, Vec<usize>> = std::collections::BTreeMap::new();
    for i in 0..items.len() {
        let root = dsu.find(i);
        by_root.entry(root).or_default().push(i);
    }

    let mut groups: Vec<(i64, SimilarGroup)> = by_root
        .into_values()
        .filter(|members| members.len() >= 2)
        .map(|members| {
            let scores: Vec<f64> = members.iter().map(|&i| items[i].1.sharpness_tile).collect();
            let top = scores.iter().cloned().fold(0.0, f64::max);
            let mut sorted = scores.clone();
            sorted.sort_by(|a, b| b.total_cmp(a));
            let clear = top > 0.0 && sorted[0] > sorted[1] * (1.0 + params.best_margin);
            let best_image_id = clear.then(|| {
                let at = scores.iter().position(|&s| s == top).unwrap_or(0);
                items[members[at]].1.image_id
            });
            let out = members
                .iter()
                .map(|&i| {
                    let input = items[i].1;
                    let relative = if top > 0.0 { input.sharpness_tile / top } else { 1.0 };
                    GroupMember { image_id: input.image_id, version_id: input.version_id, relative_sharpness: relative, soft: top > 0.0 && relative < params.soft_ratio }
                })
                .collect();
            (items[members[0]].0, SimilarGroup { members: out, best_image_id })
        })
        .collect();
    groups.sort_by_key(|(start, g)| std::cmp::Reverse((*start, g.members[0].image_id)));
    groups.into_iter().map(|(_, g)| g).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_with_flips(flips: usize) -> [u8; 32] {
        let mut h = [0u8; 32];
        for bit in 0..flips {
            h[bit / 8] |= 1 << (bit % 8);
        }
        h
    }

    fn frame(id: i64, time: &str, flips: usize, sharp: f64) -> GroupInput {
        GroupInput { image_id: id, version_id: id * 10, camera: "Canon R5".into(), captured_at: Some(time.into()), sharpness_tile: sharp, dhash: hash_with_flips(flips) }
    }

    fn at(sec: u32) -> String {
        format!("2026-03-10T10:{:02}:{:02}", sec / 60, sec % 60)
    }

    fn ids(g: &SimilarGroup) -> Vec<i64> {
        g.members.iter().map(|m| m.image_id).collect()
    }

    const DEFAULT: fn() -> GroupParams = || GroupParams::from_similarity(0.5);

    #[test]
    fn parses_capture_times() {
        assert_eq!(parse_capture_seconds("2024-03-15T10:30:00"), Some(1_710_498_600));
        assert_eq!(parse_capture_seconds("2024-03-15 10:30:00"), Some(1_710_498_600));
        assert_eq!(parse_capture_seconds("2024-03-15T10:30:00+08:00"), Some(1_710_498_600), "the offset is ignored");
        assert_eq!(parse_capture_seconds("1970-01-01T00:00:00"), Some(0));
        assert_eq!(parse_capture_seconds("2024-02-29T00:00:00").map(|s| s % 86400), Some(0), "leap day");
        assert_eq!(parse_capture_seconds("yesterday"), None);
        assert_eq!(parse_capture_seconds("2024-13-15T10:30:00"), None);
        assert_eq!(parse_capture_seconds(""), None);
    }

    #[test]
    fn the_slider_maps_to_the_rfcs_starting_point_and_loosens_monotonically() {
        let mid = GroupParams::from_similarity(0.5);
        assert_eq!((mid.max_seconds, mid.max_distance), (10, 64));
        let (strict, loose) = (GroupParams::from_similarity(0.0), GroupParams::from_similarity(1.0));
        assert!(strict.max_seconds < mid.max_seconds && mid.max_seconds < loose.max_seconds);
        assert!(strict.max_distance < mid.max_distance && mid.max_distance < loose.max_distance);
        assert!(loose.max_distance < 95, "stays under the closest unrelated pair measured in RFC-0031 §2");
        assert_eq!(GroupParams::from_similarity(7.0), loose, "clamped");
    }

    #[test]
    fn a_burst_is_one_group_with_its_sharpest_frame_and_its_soft_ones() {
        let inputs = [frame(1, &at(0), 0, 100.0), frame(2, &at(1), 5, 300.0), frame(3, &at(2), 8, 120.0), frame(4, &at(3), 6, 20.0)];
        let groups = similar_groups(&inputs, &DEFAULT());
        assert_eq!(groups.len(), 1);
        assert_eq!(ids(&groups[0]), vec![1, 2, 3, 4]);
        assert_eq!(groups[0].best_image_id, Some(2));
        let soft: Vec<bool> = groups[0].members.iter().map(|m| m.soft).collect();
        assert_eq!(soft, vec![true, false, true, true], "33%, 100%, 40% and 7% of the best");
    }

    #[test]
    fn soft_means_under_half_of_the_best() {
        let inputs = [frame(1, &at(0), 0, 100.0), frame(2, &at(1), 0, 49.0), frame(3, &at(2), 0, 51.0)];
        let g = &similar_groups(&inputs, &DEFAULT())[0];
        let by_id = |id| g.members.iter().find(|m| m.image_id == id).unwrap();
        assert!(by_id(2).soft);
        assert!(!by_id(3).soft);
        assert!(!by_id(1).soft);
        assert_eq!(by_id(1).relative_sharpness, 1.0);
    }

    #[test]
    fn two_bursts_a_minute_apart_are_two_groups_newest_first() {
        let inputs = [frame(1, &at(0), 0, 10.0), frame(2, &at(2), 3, 10.0), frame(3, &at(60), 0, 10.0), frame(4, &at(62), 2, 10.0)];
        let groups = similar_groups(&inputs, &DEFAULT());
        assert_eq!(groups.len(), 2);
        assert_eq!(ids(&groups[0]), vec![3, 4], "the later group first");
        assert_eq!(ids(&groups[1]), vec![1, 2]);
    }

    #[test]
    fn a_different_shot_at_the_same_venue_does_not_join() {
        // 5 s apart but 100 bits apart: far enough that the hash says "another picture".
        let inputs = [frame(1, &at(0), 0, 10.0), frame(2, &at(5), 100, 10.0)];
        assert!(similar_groups(&inputs, &DEFAULT()).is_empty());
    }

    #[test]
    fn a_similar_picture_taken_long_after_does_not_join() {
        let inputs = [frame(1, &at(0), 0, 10.0), frame(2, &at(30), 0, 10.0)];
        assert!(similar_groups(&inputs, &DEFAULT()).is_empty());
    }

    #[test]
    fn another_camera_or_no_capture_time_never_groups() {
        let mut other = frame(2, &at(1), 0, 10.0);
        other.camera = "Nikon Z6".into();
        assert!(similar_groups(&[frame(1, &at(0), 0, 10.0), other], &DEFAULT()).is_empty());

        let mut untimed = frame(2, &at(1), 0, 10.0);
        untimed.captured_at = None;
        let mut garbled = frame(3, &at(1), 0, 10.0);
        garbled.captured_at = Some("n/a".into());
        assert!(similar_groups(&[frame(1, &at(0), 0, 10.0), untimed, garbled], &DEFAULT()).is_empty());
    }

    #[test]
    fn the_distance_limit_is_inclusive() {
        let p = DEFAULT();
        let d = p.max_distance as usize;
        assert_eq!(similar_groups(&[frame(1, &at(0), 0, 1.0), frame(2, &at(1), d, 1.0)], &p).len(), 1);
        assert!(similar_groups(&[frame(1, &at(0), 0, 1.0), frame(2, &at(1), d + 1, 1.0)], &p).is_empty());
    }

    #[test]
    fn a_pan_that_drifts_chains_through_its_neighbours() {
        // Hashes are bit ranges set: each frame differs from the next by 40 bits (inside the 64 limit), but the
        // first and last by 120. Times are 8 s apart (inside the 10 s window), so the ends are 24 s apart.
        let bits = |upto: usize| {
            let mut x = [0u8; 32];
            for b in 0..upto {
                x[b / 8] |= 1 << (b % 8);
            }
            x
        };
        let mut inputs: Vec<GroupInput> = (0..4).map(|n| frame(n as i64 + 1, &at(n as u32 * 8), 0, 10.0)).collect();
        for (n, input) in inputs.iter_mut().enumerate() {
            input.dhash = bits(n * 40);
        }
        assert_eq!(hamming(&inputs[0].dhash, &inputs[3].dhash), 120);
        let groups = similar_groups(&inputs, &DEFAULT());
        assert_eq!(groups.len(), 1);
        assert_eq!(ids(&groups[0]), vec![1, 2, 3, 4]);
    }

    #[test]
    fn equal_sharpness_has_no_clear_best_and_a_clear_winner_needs_a_margin() {
        let equal = [frame(1, &at(0), 0, 100.0), frame(2, &at(1), 0, 100.0)];
        assert_eq!(similar_groups(&equal, &DEFAULT())[0].best_image_id, None);
        let close = [frame(1, &at(0), 0, 100.0), frame(2, &at(1), 0, 105.0)];
        assert_eq!(similar_groups(&close, &DEFAULT())[0].best_image_id, None, "5% is inside the margin");
        let clear = [frame(1, &at(0), 0, 100.0), frame(2, &at(1), 0, 130.0)];
        assert_eq!(similar_groups(&clear, &DEFAULT())[0].best_image_id, Some(2));
    }

    #[test]
    fn flat_frames_have_no_best_and_nothing_is_soft() {
        let g = &similar_groups(&[frame(1, &at(0), 0, 0.0), frame(2, &at(1), 0, 0.0)], &DEFAULT())[0];
        assert_eq!(g.best_image_id, None);
        assert!(g.members.iter().all(|m| !m.soft && m.relative_sharpness == 1.0));
    }

    #[test]
    fn same_second_frames_group_in_a_stable_order() {
        let inputs = [frame(5, &at(0), 0, 10.0), frame(3, &at(0), 1, 10.0), frame(4, &at(0), 2, 10.0)];
        assert_eq!(ids(&similar_groups(&inputs, &DEFAULT())[0]), vec![3, 4, 5]);
    }

    #[test]
    fn a_looser_slider_joins_what_a_stricter_one_keeps_apart() {
        let inputs = [frame(1, &at(0), 0, 10.0), frame(2, &at(14), 80, 10.0)];
        assert!(similar_groups(&inputs, &GroupParams::from_similarity(0.5)).is_empty());
        assert_eq!(similar_groups(&inputs, &GroupParams::from_similarity(1.0)).len(), 1);
    }

    /// The 50,000-photo catalog RFC-0031 §6 asks about, with a worst case thrown in: a quarter of the photos share one
    /// stuck-clock second. Prints the time; asserts only that the work is bounded.
    ///   cargo test --release --lib cull_groups_scale -- --ignored --nocapture
    #[test]
    #[ignore]
    fn cull_groups_scale() {
        let mut inputs = Vec::new();
        for n in 0..50_000i64 {
            let (time, flips) = if n % 4 == 0 { ("2026-01-01T00:00:00".to_string(), (n % 200) as usize) } else { (format!("2026-02-{:02}T{:02}:{:02}:{:02}", 1 + (n / 3000) % 28, (n / 120) % 24, (n / 2) % 60, (n * 7) % 60), (n % 90) as usize) };
            inputs.push(frame(n, &time, flips, (n % 97) as f64));
        }
        let t = std::time::Instant::now();
        let groups = similar_groups(&inputs, &DEFAULT());
        println!("50,000 photos: {} groups in {:.0} ms", groups.len(), t.elapsed().as_secs_f64() * 1e3);
    }
}
