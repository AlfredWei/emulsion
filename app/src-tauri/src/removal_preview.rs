//! The Develop preview of a photo with removals (M7, RFC-0032 §12.1).
//!
//! The canvas grades a downscaled decode on the GPU, so a removal cannot be a shader: the patches are laid over the
//! picture the canvas loads, exactly as the AI-denoise mix replaces it (`denoise_preview.rs`). The order is the plain
//! preview, then the denoise mix (when the photo has one), then the patches: the mix is a copy of the photograph as it
//! was, and mixing it in after a patch would pull the removed thing back in at the denoise amount.
//!
//! The result is written once per `(photo, tier, denoise copy and amount, patches)` into the previews directory
//! (`bake-<hash>-<tier>[_dn<amount>-<secs>]-<patch key>.png`, inside the webview's asset-protocol scope) and only the
//! latest one per photo and tier is kept, so adding ten removals in a row leaves one file, not ten.

use crate::catalog::StoredPatch;
use crate::denoise_preview::{ensure_blended, graded_tag};
use crate::preview_cache::DevelopPreviewInfo;
use std::path::{Path, PathBuf};

fn tier(full: bool) -> &'static str {
    if full {
        "full"
    } else {
        "draft"
    }
}

/// What names a set of patches: their ids in order (a patch's pixels never change under its id).
fn patch_key(patches: &[StoredPatch]) -> String {
    let ids: Vec<String> = patches.iter().map(|p| p.id.to_string()).collect();
    blake3::hash(ids.join(",").as_bytes()).to_hex()[..12].to_string()
}

/// Where the bake for this photo, tier, denoise state (`dn_tag`, from `graded_tag`) and patches is stored.
pub fn bake_path(out_dir: &Path, content_hash: &str, full: bool, dn_tag: &str, patches: &[StoredPatch]) -> PathBuf {
    out_dir.join(format!("bake-{content_hash}-{}{dn_tag}-{}.png", tier(full), patch_key(patches)))
}

/// Deletes this photo's bakes of one tier except `keep`.
fn sweep_other_bakes(out_dir: &Path, content_hash: &str, full: bool, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(out_dir) else { return };
    let prefix = format!("bake-{content_hash}-{}", tier(full));
    for e in entries.flatten() {
        if e.file_name().to_string_lossy().starts_with(&prefix) && e.path() != keep {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// The picture the canvas should load for this photo: `base` (the unedited draft or 1:1 preview) with the kept denoised
/// copy mixed in at `amount` (0 = none) and `patches` laid over it, built and stored unless it is already there.
/// `Ok(None)` means "show the plain preview": no amount and no patch applies. With no patch that applies, the answer is
/// the denoise mix alone, which is what `ensure_blended` says.
pub fn ensure_baked(
    base: &DevelopPreviewInfo,
    content_hash: &str,
    denoise_dir: &Path,
    out_dir: &Path,
    full: bool,
    amount: u32,
    patches: &[StoredPatch],
) -> Result<Option<DevelopPreviewInfo>, String> {
    let blended = if amount > 0 { ensure_blended(base, content_hash, denoise_dir, out_dir, full, amount)? } else { None };
    if patches.is_empty() {
        return Ok(blended);
    }
    // The tag is empty unless the photo's denoise copy exists, so a mix that could not be built is not in the name.
    let dn_tag = if blended.is_some() { graded_tag(amount as f32, Some(denoise_dir), content_hash) } else { String::new() };
    let out = bake_path(out_dir, content_hash, full, &dn_tag, patches);
    let info = |width: u32, height: u32| DevelopPreviewInfo { path: out.to_string_lossy().to_string(), width, height, is_smart_preview: base.is_smart_preview };
    if let Ok((w, h)) = image::image_dimensions(&out) {
        if (w, h) == (base.width, base.height) {
            return Ok(Some(info(w, h)));
        }
    }

    let from = blended.as_ref().map_or(&base.path, |b| &b.path);
    let mut image = image::open(from).map_err(|e| format!("could not read the preview: {e}"))?.into_rgb8();
    if crate::removal::apply_removals(&mut image, patches) == 0 {
        // Every patch is of another picture or damaged: nothing to show that the mix alone does not.
        return Ok(blended);
    }

    let tmp = out.with_extension("png.part");
    crate::denoise_commands::write_png_fast(&image, &tmp)?;
    std::fs::rename(&tmp, &out).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("could not store the preview with the removals: {e}")
    })?;
    sweep_other_bakes(out_dir, content_hash, full, &out);
    Ok(Some(info(image.width(), image.height())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::denoise_cache::entry_path;
    use crate::denoise_models::MODEL_ID;
    use image::{Rgba, RgbaImage, RgbImage};

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulsion-removal-preview-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn plain(dir: &Path, w: u32, h: u32, v: u8) -> DevelopPreviewInfo {
        let path = dir.join("plain.png");
        RgbImage::from_pixel(w, h, image::Rgb([v, v, v])).save(&path).unwrap();
        DevelopPreviewInfo { path: path.to_string_lossy().to_string(), width: w, height: h, is_smart_preview: false }
    }

    /// An opaque square of `colour`, made on a decode of `src`.
    fn patch(id: i64, x: u32, y: u32, side: u32, src: (u32, u32), colour: u8) -> StoredPatch {
        let img = RgbaImage::from_pixel(side, side, Rgba([colour, colour, colour, 255]));
        let mut png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png).unwrap();
        StoredPatch { id, x, y, w: side, h: side, src_w: src.0, src_h: src.1, engine: "t".into(), png: png.into_inner() }
    }

    fn pixel(info: &DevelopPreviewInfo, x: u32, y: u32) -> [u8; 3] {
        image::open(&info.path).unwrap().to_rgb8().get_pixel(x, y).0
    }

    #[test]
    fn the_patches_go_over_the_plain_preview_scaled_to_it_and_nothing_else_changes() {
        let dir = test_dir("plain");
        let base = plain(&dir, 40, 20, 100);
        assert!(ensure_baked(&base, "h", &dir, &dir, false, 0, &[]).unwrap().is_none(), "no patch, no denoise: the plain preview");
        // Made on an 80 x 40 decode: on this 40 x 20 preview it is half the size and half the distance.
        let got = ensure_baked(&base, "h", &dir, &dir, false, 0, &[patch(3, 20, 10, 8, (80, 40), 7)]).unwrap().expect("baked");
        assert_eq!((got.width, got.height), (40, 20));
        assert_eq!(pixel(&got, 11, 6), [7, 7, 7]);
        assert_eq!(pixel(&got, 0, 0), [100, 100, 100]);
        assert_eq!(pixel(&got, 20, 15), [100, 100, 100]);
    }

    #[test]
    fn the_denoise_mix_comes_first_and_the_patch_is_not_mixed_with_the_old_picture() {
        let dir = test_dir("mix");
        let base = plain(&dir, 20, 20, 100);
        RgbImage::from_pixel(20, 20, image::Rgb([200, 200, 200])).save(entry_path(&dir, "h", MODEL_ID)).unwrap();
        let got = ensure_baked(&base, "h", &dir, &dir, false, 50, &[patch(1, 4, 4, 4, (20, 20), 9)]).unwrap().unwrap();
        assert_eq!(pixel(&got, 5, 5), [9, 9, 9], "the patch is whole, not mixed with the denoised copy of what was there");
        assert_eq!(pixel(&got, 15, 15), [150, 150, 150], "the rest is the 50 % mix");
        assert!(got.path.contains("_dn50-"), "the name says which copy it is built from: {}", got.path);
        // No patch applies: the mix alone.
        let mix = ensure_baked(&base, "h", &dir, &dir, false, 50, &[patch(2, 0, 0, 4, (20, 40), 9)]).unwrap().unwrap();
        assert!(!mix.path.contains("bake-"), "a patch of another picture is skipped");
        assert_eq!(pixel(&mix, 1, 1), [150, 150, 150]);
    }

    #[test]
    fn it_is_reused_per_set_of_patches_and_only_the_latest_set_stays() {
        let dir = test_dir("reuse");
        let base = plain(&dir, 20, 20, 100);
        let one = [patch(1, 2, 2, 4, (20, 20), 9)];
        let first = ensure_baked(&base, "h", &dir, &dir, false, 0, &one).unwrap().unwrap();
        let written = std::fs::metadata(&first.path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(ensure_baked(&base, "h", &dir, &dir, false, 0, &one).unwrap().unwrap().path, first.path);
        assert_eq!(std::fs::metadata(&first.path).unwrap().modified().unwrap(), written, "not rewritten");

        let two = [one[0].clone(), patch(2, 10, 10, 4, (20, 20), 5)];
        let second = ensure_baked(&base, "h", &dir, &dir, false, 0, &two).unwrap().unwrap();
        assert_ne!(second.path, first.path);
        assert!(!Path::new(&first.path).exists() && Path::new(&second.path).exists(), "the older set is swept");
        // The other tier is its own.
        let full = ensure_baked(&base, "h", &dir, &dir, true, 0, &two).unwrap().unwrap();
        assert!(Path::new(&second.path).exists() && Path::new(&full.path).exists());
    }

    #[test]
    fn undoing_a_removal_gives_back_the_picture_without_it() {
        let dir = test_dir("undo");
        let base = plain(&dir, 20, 20, 100);
        let (a, b) = (patch(1, 2, 2, 4, (20, 20), 9), patch(2, 10, 10, 4, (20, 20), 5));
        let both = ensure_baked(&base, "h", &dir, &dir, false, 0, &[a.clone(), b]).unwrap().unwrap();
        assert_eq!(pixel(&both, 11, 11), [5, 5, 5]);
        let only_a = ensure_baked(&base, "h", &dir, &dir, false, 0, &[a]).unwrap().unwrap();
        assert_eq!(pixel(&only_a, 11, 11), [100, 100, 100]);
        assert_eq!(pixel(&only_a, 3, 3), [9, 9, 9]);
        assert!(ensure_baked(&base, "h", &dir, &dir, false, 0, &[]).unwrap().is_none(), "all undone: the plain preview");
    }
}
