//! The Develop preview of a denoised photo (M6 slice 3c, RFC-0027 §3.3).
//!
//! The Develop canvas grades a downscaled decode on the GPU, so the cached full-size denoised copy cannot be
//! used as is. Instead the preview **source** becomes the blend: the cached copy is scaled to the preview's
//! size and mixed in by the `ai_denoise` amount, `out = preview + (denoised - preview) * amount / 100`, and the
//! result is a PNG that replaces the preview as the canvas's source texture. The shader, the masks and the
//! histogram are untouched, and the model never runs here.
//!
//! The blend is written once per `(photo, tier, amount)` into the previews directory (`blend-<hash>-<tier>-<amount>.png`;
//! that directory, unlike the denoise cache, is inside the webview's asset-protocol scope, so the canvas can load it)
//! and only the latest one per photo and tier is kept, so dragging the slider over a hundred values leaves one
//! file, not a hundred. The frontend asks only when the amount has settled.

use crate::denoise_cache::entry_path;
use crate::denoise_models::MODEL_ID;
use crate::develop_engine::blend_denoised;
use crate::preview_cache::DevelopPreviewInfo;
use image::RgbImage;
use std::path::{Path, PathBuf};

/// How far the two pictures' aspect ratios may differ before the cached copy is judged to be of something
/// else. A RAW's half-size develop decode and its full decode round differently by a pixel or two.
const ASPECT_TOLERANCE: f64 = 0.01;

/// `denoised` at `size`, or `None` when it cannot be the same picture (a different aspect ratio: the source
/// was replaced, or decoded another way). Same size passes through; otherwise it is scaled with the filter the
/// draft preview itself was made with, so a flat area stays flat and edges line up.
pub fn fit_to(denoised: RgbImage, size: (u32, u32)) -> Option<RgbImage> {
    if denoised.dimensions() == size {
        return Some(denoised);
    }
    let ((dw, dh), (w, h)) = (denoised.dimensions(), size);
    if dw == 0 || dh == 0 || w == 0 || h == 0 {
        return None;
    }
    let (a, b) = (dw as f64 * h as f64, dh as f64 * w as f64);
    if (a - b).abs() / a.max(b) > ASPECT_TOLERANCE {
        return None;
    }
    Some(image::imageops::resize(&denoised, w, h, image::imageops::FilterType::Triangle))
}

fn tier(full: bool) -> &'static str {
    if full {
        "full"
    } else {
        "draft"
    }
}

/// Where the blend for this photo, tier and amount is stored.
pub fn blend_path(out_dir: &Path, content_hash: &str, full: bool, amount: u32) -> PathBuf {
    out_dir.join(format!("blend-{content_hash}-{}-{amount}.png", tier(full)))
}

/// Deletes this photo's blends of one tier except `keep`.
fn sweep_other_blends(out_dir: &Path, content_hash: &str, full: bool, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(out_dir) else { return };
    let prefix = format!("blend-{content_hash}-{}-", tier(full));
    for e in entries.flatten() {
        if e.file_name().to_string_lossy().starts_with(&prefix) && e.path() != keep {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// The blend of `base` (the unedited draft or 1:1 preview) with this photo's cached denoised copy at `amount`
/// (1..=100), building and storing it unless it is already there. `Ok(None)` means "show the plain preview":
/// the amount is 0, there is no cached copy, or it is not of this picture. An unreadable preview or a failed
/// write is an error.
pub fn ensure_blended(base: &DevelopPreviewInfo, content_hash: &str, cache_dir: &Path, out_dir: &Path, full: bool, amount: u32) -> Result<Option<DevelopPreviewInfo>, String> {
    let amount = amount.min(100);
    let entry = entry_path(cache_dir, content_hash, MODEL_ID);
    if amount == 0 || !entry.exists() {
        return Ok(None);
    }
    let out = blend_path(out_dir, content_hash, full, amount);
    let info = |width: u32, height: u32| DevelopPreviewInfo { path: out.to_string_lossy().to_string(), width, height, is_smart_preview: base.is_smart_preview };
    // Reusable only if it was built after the cached copy it mixes in: a re-run replaces that copy, and the
    // old blends of the same amount must not outlive it.
    let newer_than_entry = || match (std::fs::metadata(&out).and_then(|m| m.modified()), std::fs::metadata(&entry).and_then(|m| m.modified())) {
        (Ok(blend), Ok(copy)) => blend >= copy,
        _ => false,
    };
    if let Ok((w, h)) = image::image_dimensions(&out) {
        if (w, h) == (base.width, base.height) && newer_than_entry() {
            return Ok(Some(info(w, h)));
        }
    }

    let mut image = image::open(&base.path).map_err(|e| format!("could not read the preview: {e}"))?.into_rgb8();
    // A damaged cache entry is the same as no entry: the plain preview stays.
    let Ok(denoised) = image::open(&entry) else { return Ok(None) };
    let Some(denoised) = fit_to(denoised.into_rgb8(), image.dimensions()) else { return Ok(None) };
    blend_denoised(&mut image, &denoised, amount as f32);

    let tmp = out.with_extension("png.part");
    crate::denoise_commands::write_png_fast(&image, &tmp)?;
    std::fs::rename(&tmp, &out).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("could not store the denoised preview: {e}")
    })?;
    sweep_other_blends(out_dir, content_hash, full, &out);
    Ok(Some(info(image.width(), image.height())))
}

/// The part of a graded preview's file name that says which denoised copy (if any) it was built from, so a
/// preview made before a copy existed, or from an older one, is never reused: `_dn<amount>-<copy's mtime in
/// seconds>`, or empty when the stack has no effective `ai_denoise`, there is no copy, or no cache is given.
pub fn graded_tag(effective_amount: f32, cache_dir: Option<&Path>, content_hash: &str) -> String {
    let amount = effective_amount.round() as u32;
    let Some(dir) = cache_dir.filter(|_| amount > 0 && !content_hash.is_empty()) else { return String::new() };
    let Ok(modified) = std::fs::metadata(entry_path(dir, content_hash, MODEL_ID)).and_then(|m| m.modified()) else { return String::new() };
    let secs = modified.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("_dn{amount}-{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulsion-denoise-preview-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn plain(dir: &Path, w: u32, h: u32, v: u8) -> DevelopPreviewInfo {
        let path = dir.join("plain.png");
        RgbImage::from_pixel(w, h, image::Rgb([v, v, v])).save(&path).unwrap();
        DevelopPreviewInfo { path: path.to_string_lossy().to_string(), width: w, height: h, is_smart_preview: false }
    }

    #[test]
    fn a_cached_copy_is_scaled_to_the_preview_only_when_it_is_the_same_picture() {
        let big = RgbImage::from_pixel(40, 20, image::Rgb([9, 9, 9]));
        assert_eq!(fit_to(big.clone(), (40, 20)).unwrap().dimensions(), (40, 20));
        let small = fit_to(big.clone(), (20, 10)).unwrap();
        assert_eq!(small.dimensions(), (20, 10));
        assert_eq!(small.get_pixel(7, 4).0, [9, 9, 9], "a flat area stays flat");
        // A pixel of rounding in a half-size decode is the same picture; another aspect ratio is not.
        assert!(fit_to(RgbImage::new(3001, 2000), (1500, 1000)).is_some());
        assert!(fit_to(big.clone(), (20, 20)).is_none());
        assert!(fit_to(big, (0, 5)).is_none());
    }

    #[test]
    fn the_blend_is_the_preview_moved_toward_the_scaled_copy_by_the_amount_and_is_reused() {
        let dir = test_dir("blend");
        let base = plain(&dir, 8, 4, 100);
        let hash = "abc";
        // No cached copy, or amount 0: show the plain preview.
        assert!(ensure_blended(&base, hash, &dir, &dir, false, 50).unwrap().is_none());
        RgbImage::from_pixel(16, 8, image::Rgb([200, 200, 200])).save(entry_path(&dir, hash, MODEL_ID)).unwrap();
        assert!(ensure_blended(&base, hash, &dir, &dir, false, 0).unwrap().is_none());

        let got = ensure_blended(&base, hash, &dir, &dir, false, 50).unwrap().expect("blended");
        assert_eq!((got.width, got.height), (8, 4));
        assert_eq!(image::open(&got.path).unwrap().to_rgb8().get_pixel(3, 2).0, [150, 150, 150]);
        let full = ensure_blended(&base, hash, &dir, &dir, false, 100).unwrap().unwrap();
        assert_eq!(image::open(&full.path).unwrap().to_rgb8().get_pixel(0, 0).0, [200, 200, 200]);

        // Only the latest amount per photo and tier stays; the other tier is its own.
        assert!(blend_path(&dir, hash, false, 100).exists());
        assert!(!blend_path(&dir, hash, false, 50).exists());
        ensure_blended(&base, hash, &dir, &dir, true, 30).unwrap().unwrap();
        assert!(blend_path(&dir, hash, false, 100).exists() && blend_path(&dir, hash, true, 30).exists());

        // A second ask for the same amount reuses the file (it is not rewritten).
        let before = std::fs::metadata(&full.path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        ensure_blended(&base, hash, &dir, &dir, false, 100).unwrap().unwrap();
        assert_eq!(std::fs::metadata(&full.path).unwrap().modified().unwrap(), before);

        // A re-run replaces the cached copy: the blend of the same amount is rebuilt from the new one.
        std::thread::sleep(std::time::Duration::from_millis(20));
        RgbImage::from_pixel(16, 8, image::Rgb([40, 40, 40])).save(entry_path(&dir, hash, MODEL_ID)).unwrap();
        let rebuilt = ensure_blended(&base, hash, &dir, &dir, false, 100).unwrap().unwrap();
        assert_eq!(image::open(&rebuilt.path).unwrap().to_rgb8().get_pixel(0, 0).0, [40, 40, 40]);
    }

    #[test]
    fn the_graded_tag_names_the_amount_and_the_copy_and_is_empty_without_either() {
        let dir = test_dir("tag");
        assert_eq!(graded_tag(60.0, Some(&dir), "h"), "", "no copy yet");
        RgbImage::new(2, 2).save(entry_path(&dir, "h", MODEL_ID)).unwrap();
        let tag = graded_tag(60.0, Some(&dir), "h");
        assert!(tag.starts_with("_dn60-"), "{tag}");
        assert_eq!(graded_tag(0.0, Some(&dir), "h"), "");
        assert_eq!(graded_tag(60.0, None, "h"), "");
        assert_eq!(graded_tag(60.0, Some(&dir), ""), "");
        assert_ne!(graded_tag(61.0, Some(&dir), "h"), tag);
    }

    #[test]
    fn a_copy_of_another_picture_or_a_damaged_one_leaves_the_plain_preview() {
        let dir = test_dir("other");
        let base = plain(&dir, 8, 4, 100);
        RgbImage::new(8, 8).save(entry_path(&dir, "wrong-aspect", MODEL_ID)).unwrap();
        assert!(ensure_blended(&base, "wrong-aspect", &dir, &dir, false, 50).unwrap().is_none());
        std::fs::write(entry_path(&dir, "broken", MODEL_ID), b"not a png").unwrap();
        assert!(ensure_blended(&base, "broken", &dir, &dir, false, 50).unwrap().is_none());
        assert!(!blend_path(&dir, "broken", false, 50).exists());
    }
}
