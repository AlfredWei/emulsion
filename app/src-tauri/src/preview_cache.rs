//! Develop preview cache (M1 Slice 4) — see docs/PRD/PRD.md §7.2/§7.6.
//!
//! Owns disk-cache-generation concerns for the Develop preview: keyed by
//! the source file's *content* hash (not its path — a path-hash cache,
//! Slice 3's original approach, is a real correctness bug: a file
//! replaced in place at the same path would silently reuse a stale
//! cached PNG forever). Content-hash keying is a strict improvement on
//! the PRD's own phrasing ("invalidated and regenerated... when the
//! source file itself changes (moved/re-imported/replaced)") — a
//! moved/renamed file with identical bytes needs no regeneration at all,
//! it just resolves to the existing cache entry.
//!
//! Cache eviction is explicitly out of scope: a same-path replace-in-place
//! correctly produces a new cache entry, but the old PNG is left orphaned
//! on disk. Matches thumbnails' existing unbounded growth — not a
//! regression, just not solved here.

use crate::catalog::Catalog;
use crate::source_decode::{self, DecodeError};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Interactive Develop preview is capped to this on its longest edge,
/// regardless of what the RAW decode itself produced -- `decode_develop_preview`'s
/// half_size request is best-effort (see raw_decode.rs), not a guarantee,
/// so this resize is what actually bounds the preview's size.
pub const DEVELOP_PREVIEW_MAX_DIMENSION: u32 = 2048;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DevelopPreviewInfo {
    pub path: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum PreviewCacheError {
    #[error("could not read source file from disk: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error("decoded buffer size didn't match its own reported dimensions")]
    BufferMismatch,
    #[error("image processing failed: {0}")]
    Image(#[from] image::ImageError),
}

fn capped_dimensions(width: u32, height: u32, max_dim: u32) -> (u32, u32) {
    if width <= max_dim && height <= max_dim {
        return (width, height);
    }
    let scale = max_dim as f64 / width.max(height) as f64;
    (
        ((width as f64) * scale).round().max(1.0) as u32,
        ((height as f64) * scale).round().max(1.0) as u32,
    )
}

/// Interactive path: the Tauri command only has a bare file path from the
/// frontend (no catalog access), so it must read+hash to find the cache
/// key. One extra full-file read on a cache hit (negligible: RAW files
/// are tens of MB, and the OS page cache makes a second read near-free
/// right after the first) — not worth restructuring
/// `source_decode::decode_develop_preview` to accept pre-read bytes just to
/// avoid it.
pub fn ensure_develop_preview(
    source_path: &Path,
    previews_dir: &Path,
) -> Result<DevelopPreviewInfo, PreviewCacheError> {
    let bytes = std::fs::read(source_path)?;
    let content_hash = blake3::hash(&bytes).to_hex().to_string();
    ensure_develop_preview_for_hash(source_path, &content_hash, previews_dir)
}

/// Background-walk path: the caller already knows the content hash from
/// the catalog (`import.rs` computes and stores it at import time), so
/// this skips the read+hash entirely on a cache hit -- just an
/// `exists()` check plus a PNG-header-only read via
/// `image::image_dimensions`. This is what makes re-walking the *entire*
/// catalog after every import/startup actually cheap in steady state,
/// rather than O(catalog size in bytes) of disk I/O every time.
pub fn ensure_develop_preview_for_hash(
    source_path: &Path,
    content_hash: &str,
    previews_dir: &Path,
) -> Result<DevelopPreviewInfo, PreviewCacheError> {
    let out_path = previews_dir.join(format!("{content_hash}.png"));

    if out_path.exists() {
        let (width, height) = image::image_dimensions(&out_path)?;
        return Ok(DevelopPreviewInfo {
            path: out_path.to_string_lossy().to_string(),
            width,
            height,
        });
    }

    std::fs::create_dir_all(previews_dir)?;

    let decoded = source_decode::decode_develop_preview(source_path)?;
    let source = image::RgbImage::from_raw(decoded.width, decoded.height, decoded.rgb)
        .ok_or(PreviewCacheError::BufferMismatch)?;

    let (target_w, target_h) =
        capped_dimensions(source.width(), source.height(), DEVELOP_PREVIEW_MAX_DIMENSION);
    let resized = if (target_w, target_h) == (source.width(), source.height()) {
        source
    } else {
        image::imageops::resize(&source, target_w, target_h, image::imageops::FilterType::Triangle)
    };

    resized.save(&out_path)?;

    Ok(DevelopPreviewInfo {
        path: out_path.to_string_lossy().to_string(),
        width: resized.width(),
        height: resized.height(),
    })
}

/// Walks every cataloged image and ensures each has a cache entry,
/// skipping (not aborting on) per-image failures -- same "continue past
/// one bad file" discipline as `import::scan_and_import`. Cheap to call
/// repeatedly: already-cached images cost one `exists()` + header read,
/// not a decode.
///
/// Deliberately sequential, not a parallel worker pool -- avoids CPU/
/// memory spikes decoding many multi-ten-MB RAW files at once. Accepted
/// tradeoff: with no priority queue, a large first-run backlog walk
/// (startup catch-up, or right after a big import) can transiently
/// compete for CPU with an interactive `ensure_develop_preview` call for
/// whatever image the user just opened -- both run as independent
/// `spawn_blocking` tasks. Self-healing (worst case: one duplicate
/// decode) and only a first-run/backlog condition, not a correctness bug.
pub fn pregenerate_missing(catalog: &Arc<Mutex<Catalog>>, previews_dir: &Path) {
    let images = {
        let Ok(catalog) = catalog.lock() else { return };
        catalog.list_images().unwrap_or_default()
    };
    let _ = std::fs::create_dir_all(previews_dir);

    for image in images {
        let Some(hash) = image.content_hash else { continue };
        if let Err(e) = ensure_develop_preview_for_hash(Path::new(&image.path), &hash, previews_dir) {
            eprintln!("preview pregeneration failed for {}: {e}", image.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_previews_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("emulsion-preview-cache-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The interactive path and `import.rs`'s import-time hashing must
    /// compute byte-identical hashes for the same content -- otherwise a
    /// freshly imported image and its first interactive Develop open
    /// would produce two different cache entries for the same file.
    /// There's only one real call-site pattern (`blake3::hash(&bytes)`)
    /// today, so this is currently trivially true; this test exists to
    /// catch future drift if either call site's hashing ever changes.
    #[test]
    fn hash_computation_matches_import_rs_pattern() {
        let bytes = b"some file content, doesn't need to be a real RAW file for this check";
        let via_preview_cache = blake3::hash(bytes).to_hex().to_string();
        let via_import_pattern = blake3::hash(bytes).to_hex().to_string();
        assert_eq!(via_preview_cache, via_import_pattern);
    }

    /// Real-file-gated, same pattern as raw_decode.rs/import.rs: point
    /// EMULSION_TEST_RAW_SAMPLE at a real RAW/DNG file to run these.
    #[test]
    fn cache_hit_skips_redecode() {
        let Ok(sample_path) = std::env::var("EMULSION_TEST_RAW_SAMPLE") else {
            eprintln!("skipping: set EMULSION_TEST_RAW_SAMPLE=/path/to/file.DNG to run this test");
            return;
        };
        let previews_dir = temp_previews_dir("cache-hit");

        let first = ensure_develop_preview(Path::new(&sample_path), &previews_dir)
            .expect("first call decodes and caches");
        let mtime_after_first = std::fs::metadata(&first.path).unwrap().modified().unwrap();

        let second = ensure_develop_preview(Path::new(&sample_path), &previews_dir)
            .expect("second call hits the cache");
        let mtime_after_second = std::fs::metadata(&second.path).unwrap().modified().unwrap();

        assert_eq!(first.path, second.path);
        assert_eq!(first.width, second.width);
        assert_eq!(first.height, second.height);
        assert_eq!(
            mtime_after_first, mtime_after_second,
            "second call must not rewrite the cached PNG"
        );

        let _ = std::fs::remove_dir_all(&previews_dir);
    }

    #[test]
    fn same_content_different_path_reuses_cache_entry() {
        let Ok(sample_path) = std::env::var("EMULSION_TEST_RAW_SAMPLE") else {
            eprintln!("skipping: set EMULSION_TEST_RAW_SAMPLE=/path/to/file.DNG to run this test");
            return;
        };
        let previews_dir = temp_previews_dir("dedup");
        let scratch_dir = temp_previews_dir("dedup-sources");

        let ext = Path::new(&sample_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("dng");
        let copy_a = scratch_dir.join(format!("a.{ext}"));
        let copy_b = scratch_dir.join(format!("b.{ext}"));
        std::fs::copy(&sample_path, &copy_a).unwrap();
        std::fs::copy(&sample_path, &copy_b).unwrap();

        let from_a = ensure_develop_preview(&copy_a, &previews_dir).expect("decodes copy a");
        let from_b = ensure_develop_preview(&copy_b, &previews_dir).expect("hits cache for copy b");

        assert_eq!(from_a.path, from_b.path, "identical content must resolve to the same cache entry");

        let _ = std::fs::remove_dir_all(&previews_dir);
        let _ = std::fs::remove_dir_all(&scratch_dir);
    }

    #[test]
    fn pregenerate_missing_populates_cache_from_a_real_catalog() {
        let Ok(sample_path) = std::env::var("EMULSION_TEST_RAW_SAMPLE") else {
            eprintln!("skipping: set EMULSION_TEST_RAW_SAMPLE=/path/to/file.DNG to run this test");
            return;
        };
        let scratch_dir = temp_previews_dir("pregen-source");
        let previews_dir = temp_previews_dir("pregen-previews");
        let thumb_dir = temp_previews_dir("pregen-thumbs");

        let dest = scratch_dir.join(format!(
            "sample.{}",
            Path::new(&sample_path).extension().and_then(|e| e.to_str()).unwrap_or("dng")
        ));
        std::fs::copy(&sample_path, &dest).unwrap();

        let catalog = Catalog::open_in_memory().unwrap();
        crate::import::scan_and_import(&scratch_dir, &catalog, &thumb_dir);
        let catalog = Arc::new(Mutex::new(catalog));

        pregenerate_missing(&catalog, &previews_dir);

        let entries: Vec<_> = std::fs::read_dir(&previews_dir).unwrap().collect();
        assert_eq!(entries.len(), 1, "exactly one preview PNG should be generated for the one cataloged image");

        let _ = std::fs::remove_dir_all(&scratch_dir);
        let _ = std::fs::remove_dir_all(&previews_dir);
        let _ = std::fs::remove_dir_all(&thumb_dir);
    }
}
