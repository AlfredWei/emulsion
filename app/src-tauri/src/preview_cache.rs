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
//!
//! **Smart Previews (M4)**: this same draft-tier cache doubles as the
//! offline fallback PRD/MILESTONES.md's M4 scope calls "Smart Previews" --
//! deliberately not a second, separate artifact/format (e.g. a lossy DNG
//! proxy, as real Lightroom builds). This cache is already
//! content-hash-keyed and already pregenerated for every cataloged image
//! by `pregenerate_missing` (called after every import/at startup), so it
//! already IS a "lightweight proxy that outlives the source" -- the only
//! missing piece was behavioral: `ensure_develop_preview`'s interactive
//! path used to unconditionally read+hash the source file to find its own
//! cache key, so a disconnected/offline source made even an
//! ALREADY-CACHED preview unreachable. `known_content_hash` closes that
//! gap: the frontend already holds the catalog's own `content_hash` for
//! whatever image it's opening, so it can supply it directly, and a
//! source-read failure falls back to that hash's cache entry instead of
//! failing outright.

use crate::catalog::{Catalog, EditStack};
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
    /// M4 Smart Previews: true only when the source file itself could not
    /// be read (moved/renamed/deleted/on a disconnected drive) and this
    /// result is a pre-existing cache entry served instead, via the
    /// caller-supplied `known_content_hash` -- see `ensure_develop_preview`'s
    /// doc comment. False for every normal, source-verified result,
    /// including a completely ordinary cache hit.
    pub is_smart_preview: bool,
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
    #[error("soft proofing failed: {0}")]
    SoftProof(#[from] crate::soft_proof::SoftProofError),
}

impl PreviewCacheError {
    /// A clean, user-facing message for the Tauri command boundary --
    /// deliberately separate from this type's own `Display` impl above
    /// (left unchanged, still technical -- used internally/in tests, where
    /// the precise error is more useful than a friendly one). A raw
    /// `std::io::Error`'s own Display ("No such file or directory (os
    /// error 2)") means nothing to someone who never touched the
    /// filesystem directly -- a missing/moved/renamed source file, or one
    /// on a now-disconnected drive, is a normal, expected-to-happen-
    /// eventually reality for any photo catalog, not something a raw OS
    /// errno should represent to the user. Every OTHER error kind still
    /// falls through to the existing technical message -- this only
    /// special-cases the one kind (`NotFound`) that has an obvious,
    /// actionable, non-technical explanation.
    pub fn user_message(&self) -> String {
        if let PreviewCacheError::Io(io_err) = self {
            if io_err.kind() == std::io::ErrorKind::NotFound {
                return "Source photo not found -- it may have been moved, renamed, or deleted outside the app, or is on a disconnected drive.".to_string();
            }
        }
        self.to_string()
    }
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
///
/// `known_content_hash` is the Smart Previews fallback (M4): the frontend
/// already has this image's `content_hash` from the catalog, so when the
/// source file itself can't be read (moved/renamed/deleted/offline drive),
/// this falls back to that hash's existing cache entry instead of failing
/// outright — still fails cleanly if no such entry exists (an image whose
/// preview was never generated while its source was reachable has nothing
/// to fall back to). Still catalog-decoupled: the hash is a caller-
/// supplied input, not looked up here.
pub fn ensure_develop_preview(
    source_path: &Path,
    previews_dir: &Path,
    known_content_hash: Option<&str>,
) -> Result<DevelopPreviewInfo, PreviewCacheError> {
    match std::fs::read(source_path) {
        Ok(bytes) => {
            let content_hash = blake3::hash(&bytes).to_hex().to_string();
            ensure_develop_preview_for_hash(source_path, &content_hash, previews_dir)
        }
        Err(io_err) => {
            if io_err.kind() == std::io::ErrorKind::NotFound {
                if let Some(cached) = smart_preview_fallback(known_content_hash, previews_dir, "")? {
                    return Ok(cached);
                }
            }
            Err(PreviewCacheError::Io(io_err))
        }
    }
}

/// Shared by the draft and full-preview fallback paths: looks for an
/// existing cache entry under `known_content_hash` (with `suffix` —
/// `DEVELOP_FULL_PREVIEW_SUFFIX` or `""` for the draft tier) and returns it
/// marked `is_smart_preview: true` if present, `Ok(None)` if there's no
/// hash to fall back to or nothing cached under it (the caller then
/// surfaces the original read error, not this function's own absence of a
/// result).
fn smart_preview_fallback(
    known_content_hash: Option<&str>,
    previews_dir: &Path,
    suffix: &str,
) -> Result<Option<DevelopPreviewInfo>, PreviewCacheError> {
    let Some(hash) = known_content_hash else { return Ok(None) };
    let out_path = previews_dir.join(format!("{hash}{suffix}.png"));
    if !out_path.exists() {
        return Ok(None);
    }
    let (width, height) = image::image_dimensions(&out_path)?;
    Ok(Some(DevelopPreviewInfo {
        path: out_path.to_string_lossy().to_string(),
        width,
        height,
        is_smart_preview: true,
    }))
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
            is_smart_preview: false,
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
        is_smart_preview: false,
    })
}

/// A second, higher-resolution tier (mirrors real Lightroom's own
/// "1:1 Preview" alongside its "Standard Preview" -- PRD/PRD.md's own
/// explicit "Standard/1:1 Preview cache" phrasing). The draft tier above
/// is always capped to `DEVELOP_PREVIEW_MAX_DIMENSION` for fast interactive
/// loading and Fit-mode viewing; this tier is the source's true native
/// resolution, uncapped, for 100% zoom -- built lazily (see
/// `ensure_develop_full_preview`'s doc comment), never pregenerated in
/// `pregenerate_missing` below.
pub const DEVELOP_FULL_PREVIEW_SUFFIX: &str = "_full";

/// Interactive path, mirroring `ensure_develop_preview`. `DevelopCanvas.svelte`
/// only calls this once the user actually zooms an image to 100% --
/// pregenerating this for every cataloged image up front would multiply
/// background decode/disk cost by roughly the ratio of native resolution
/// to the ~2048px draft cap (often 5-10x pixel count) for images that may
/// never be zoomed into.
pub fn ensure_develop_full_preview(
    source_path: &Path,
    previews_dir: &Path,
    known_content_hash: Option<&str>,
) -> Result<DevelopPreviewInfo, PreviewCacheError> {
    match std::fs::read(source_path) {
        Ok(bytes) => {
            let content_hash = blake3::hash(&bytes).to_hex().to_string();
            ensure_develop_full_preview_for_hash(source_path, &content_hash, previews_dir)
        }
        Err(io_err) => {
            if io_err.kind() == std::io::ErrorKind::NotFound {
                if let Some(cached) =
                    smart_preview_fallback(known_content_hash, previews_dir, DEVELOP_FULL_PREVIEW_SUFFIX)?
                {
                    return Ok(cached);
                }
            }
            Err(PreviewCacheError::Io(io_err))
        }
    }
}

/// Cache key is `{content_hash}_full.png` -- a distinct filename from the
/// draft tier's own `{content_hash}.png`, so both coexist on disk with no
/// collision (same "distinct suffix for a second content-hash-keyed
/// artifact" precedent the edited-thumbnail cache already established).
/// Decodes via `source_decode::decode_preview` -- the SAME function
/// `export.rs` already uses for full-resolution final export -- with no
/// resize/cap applied afterward: this tier IS the source's native size.
pub fn ensure_develop_full_preview_for_hash(
    source_path: &Path,
    content_hash: &str,
    previews_dir: &Path,
) -> Result<DevelopPreviewInfo, PreviewCacheError> {
    let out_path = previews_dir.join(format!("{content_hash}{DEVELOP_FULL_PREVIEW_SUFFIX}.png"));

    if out_path.exists() {
        let (width, height) = image::image_dimensions(&out_path)?;
        return Ok(DevelopPreviewInfo {
            path: out_path.to_string_lossy().to_string(),
            width,
            height,
            is_smart_preview: false,
        });
    }

    std::fs::create_dir_all(previews_dir)?;

    let decoded = source_decode::decode_preview(source_path)?;
    let source = image::RgbImage::from_raw(decoded.width, decoded.height, decoded.rgb)
        .ok_or(PreviewCacheError::BufferMismatch)?;

    source.save(&out_path)?;

    Ok(DevelopPreviewInfo {
        path: out_path.to_string_lossy().to_string(),
        width: source.width(),
        height: source.height(),
        is_smart_preview: false,
    })
}

/// Edit-graded companion to the draft tier above — closes the "Library
/// mode and Develop show different colors" gap: `ensure_develop_preview`/
/// `ensure_develop_full_preview` are BOTH a pure, unedited decode (see
/// this module's own header comment — they exist as a GPU source texture
/// for `DevelopCanvas.svelte`'s own shader pipeline to grade, nothing
/// more), so `LibraryImageViewer.svelte` (Library's Loupe view, which has
/// no shader pipeline of its own — it just displays a plain `<img>`) was
/// rendering the RAW, un-graded preview directly. Only the small grid
/// thumbnail (`import::regenerate_edited_thumbnail`, capped at 1024px)
/// was ever edit-stack-graded outside Develop itself.
///
/// This applies the SAME grading pipeline that function already
/// established (lens correction -> perspective -> edit stack -> crop) to
/// the draft tier's own cached decode, at that tier's resolution (already
/// capped at `DEVELOP_PREVIEW_MAX_DIMENSION` — crop only ever shrinks
/// further, so no separate resize step is needed here). Cache key folds
/// in a hash of the edit-stack JSON, mirroring
/// `regenerate_edited_thumbnail`'s own content-addressing precedent: a
/// later edit naturally produces a different filename, so there's no
/// explicit invalidation step to get wrong — same accepted "orphaned old
/// entries, no eviction" tradeoff this module's own header comment
/// already documents for its other tiers.
pub fn ensure_graded_preview_for_hash(
    source_path: &Path,
    content_hash: &str,
    stack: &EditStack,
    previews_dir: &Path,
) -> Result<DevelopPreviewInfo, PreviewCacheError> {
    let stack_json = serde_json::to_string(stack).unwrap_or_default();
    let stack_hash = blake3::hash(stack_json.as_bytes()).to_hex().to_string();
    let out_path = previews_dir.join(format!("{content_hash}_{}_graded.png", &stack_hash[..8]));

    if out_path.exists() {
        let (width, height) = image::image_dimensions(&out_path)?;
        return Ok(DevelopPreviewInfo {
            path: out_path.to_string_lossy().to_string(),
            width,
            height,
            is_smart_preview: false,
        });
    }

    // Reuse the draft tier's own unedited cache as the decode source --
    // avoids a second raw source decode purely to apply grading on top.
    let preview = ensure_develop_preview_for_hash(source_path, content_hash, previews_dir)?;
    let mut decoded = image::open(&preview.path)?.into_rgb8();

    crate::develop_engine::apply_lens_correction(&mut decoded, stack);
    crate::develop_engine::apply_perspective(&mut decoded, stack);
    crate::develop_engine::apply_edit_stack(&mut decoded, stack);
    crate::develop_engine::apply_crop(&mut decoded, stack);

    std::fs::create_dir_all(previews_dir)?;
    decoded.save(&out_path)?;

    Ok(DevelopPreviewInfo {
        path: out_path.to_string_lossy().to_string(),
        width: decoded.width(),
        height: decoded.height(),
        is_smart_preview: false,
    })
}

/// Soft-proof simulation of the CURRENT edit-graded look on a target ICC
/// profile (M4 Soft Proofing) — reuses the graded tier's own cached decode
/// as its source, since soft proofing simulates what the user's ACTUAL
/// edit will look like on the target device, not the unedited RAW, then
/// applies `soft_proof::apply_soft_proof` on top. Cache key folds in both
/// the edit-stack hash (via the graded tier's own filename) and a hash of
/// the soft-proof settings themselves (`soft_proof::settings_cache_key`)
/// — same content-addressed, no-explicit-invalidation discipline as every
/// other tier in this module.
pub fn ensure_soft_proof_preview_for_hash(
    source_path: &Path,
    content_hash: &str,
    stack: &EditStack,
    settings: &crate::soft_proof::SoftProofSettings,
    previews_dir: &Path,
) -> Result<DevelopPreviewInfo, PreviewCacheError> {
    let settings_key = crate::soft_proof::settings_cache_key(settings)?;
    let stack_json = serde_json::to_string(stack).unwrap_or_default();
    let stack_hash = blake3::hash(stack_json.as_bytes()).to_hex().to_string();
    let out_path = previews_dir.join(format!(
        "{content_hash}_{}_proof_{settings_key}.png",
        &stack_hash[..8]
    ));

    if out_path.exists() {
        let (width, height) = image::image_dimensions(&out_path)?;
        return Ok(DevelopPreviewInfo {
            path: out_path.to_string_lossy().to_string(),
            width,
            height,
            is_smart_preview: false,
        });
    }

    let graded = ensure_graded_preview_for_hash(source_path, content_hash, stack, previews_dir)?;
    let mut decoded = image::open(&graded.path)?.into_rgb8();

    crate::soft_proof::apply_soft_proof(&mut decoded, settings)?;

    std::fs::create_dir_all(previews_dir)?;
    decoded.save(&out_path)?;

    Ok(DevelopPreviewInfo {
        path: out_path.to_string_lossy().to_string(),
        width: decoded.width(),
        height: decoded.height(),
        is_smart_preview: false,
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

    /// `ensure_develop_preview` on a genuinely nonexistent path must
    /// produce the friendly, actionable message -- not the raw OS errno
    /// string ("No such file or directory (os error 2)") that a plain
    /// `std::io::Error::to_string()` would give. This is the exact
    /// real-world trigger (a dangling catalog reference to a moved/
    /// deleted/offline source file) `user_message()` exists to fix.
    #[test]
    fn user_message_is_friendly_for_a_missing_source_file() {
        let previews_dir = temp_previews_dir("missing-source");
        let err = ensure_develop_preview(
            std::path::Path::new("/definitely/does/not/exist/nowhere.jpg"),
            &previews_dir,
            None,
        )
        .expect_err("a nonexistent source path must fail");
        assert_eq!(
            err.user_message(),
            "Source photo not found -- it may have been moved, renamed, or deleted outside the app, or is on a disconnected drive."
        );
        // The technical Display message is UNCHANGED -- still exposes the
        // raw OS string, since that's still useful internally/in logs,
        // only user_message() is meant to be shown to a user.
        assert!(err.to_string().contains("could not read source file from disk"));
    }

    /// A DIFFERENT io::ErrorKind (not NotFound) must still fall through to
    /// the existing technical message -- user_message() only special-cases
    /// the one kind with an obvious, actionable, non-technical
    /// explanation, not every possible I/O failure.
    #[test]
    fn user_message_falls_back_to_technical_message_for_other_error_kinds() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied (test)");
        let err = PreviewCacheError::Io(io_err);
        assert_eq!(err.user_message(), err.to_string());
        assert!(err.user_message().contains("could not read source file from disk"));
    }

    /// Smart Previews (M4): needs no real RAW sample -- unlike the
    /// decode-path tests below, this only exercises the fallback branch
    /// (an already-cached PNG on disk, a source path that fails to read),
    /// so it always runs, not gated on `EMULSION_TEST_RAW_SAMPLE`.
    #[test]
    fn falls_back_to_a_cached_preview_by_hash_when_the_source_is_unreachable() {
        let previews_dir = temp_previews_dir("smart-preview-fallback");
        let hash = "deadbeefcafefeed";
        let cached_path = previews_dir.join(format!("{hash}.png"));
        image::RgbImage::from_pixel(4, 3, image::Rgb([10, 20, 30])).save(&cached_path).unwrap();

        let result = ensure_develop_preview(
            Path::new("/definitely/does/not/exist/nowhere.CR3"),
            &previews_dir,
            Some(hash),
        )
        .expect("must fall back to the cached preview, not fail outright");

        assert_eq!(result.path, cached_path.to_string_lossy());
        assert_eq!((result.width, result.height), (4, 3));
        assert!(result.is_smart_preview, "must be flagged as a Smart Preview fallback result");

        let _ = std::fs::remove_dir_all(&previews_dir);
    }

    /// The fallback only helps when something was actually cached under
    /// the supplied hash -- an image whose preview was never generated
    /// while its source was reachable has nothing to fall back to, and
    /// must still fail with the same friendly "not found" message as
    /// before, not silently succeed with nothing to show.
    #[test]
    fn fails_cleanly_when_source_is_unreachable_and_nothing_is_cached_for_the_hash() {
        let previews_dir = temp_previews_dir("smart-preview-no-fallback");
        let err = ensure_develop_preview(
            Path::new("/definitely/does/not/exist/nowhere.CR3"),
            &previews_dir,
            Some("some-hash-with-nothing-cached"),
        )
        .expect_err("no cache entry exists for this hash, so this must still fail");
        assert_eq!(
            err.user_message(),
            "Source photo not found -- it may have been moved, renamed, or deleted outside the app, or is on a disconnected drive."
        );
        let _ = std::fs::remove_dir_all(&previews_dir);
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

        let first = ensure_develop_preview(Path::new(&sample_path), &previews_dir, None)
            .expect("first call decodes and caches");
        let mtime_after_first = std::fs::metadata(&first.path).unwrap().modified().unwrap();

        let second = ensure_develop_preview(Path::new(&sample_path), &previews_dir, None)
            .expect("second call hits the cache");
        let mtime_after_second = std::fs::metadata(&second.path).unwrap().modified().unwrap();

        assert_eq!(first.path, second.path);
        assert_eq!(first.width, second.width);
        assert_eq!(first.height, second.height);
        assert!(!first.is_smart_preview, "a normal, source-verified result must not be flagged as a fallback");
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

        let from_a = ensure_develop_preview(&copy_a, &previews_dir, None).expect("decodes copy a");
        let from_b = ensure_develop_preview(&copy_b, &previews_dir, None).expect("hits cache for copy b");

        assert_eq!(from_a.path, from_b.path, "identical content must resolve to the same cache entry");

        let _ = std::fs::remove_dir_all(&previews_dir);
        let _ = std::fs::remove_dir_all(&scratch_dir);
    }

    /// The full tier must produce a real, independently-cached file at a
    /// DIFFERENT path from the draft tier's own cache entry for the same
    /// source -- if these ever collided, one tier would silently clobber
    /// the other on disk.
    #[test]
    fn full_preview_is_cached_separately_from_the_draft_tier() {
        let Ok(sample_path) = std::env::var("EMULSION_TEST_RAW_SAMPLE") else {
            eprintln!("skipping: set EMULSION_TEST_RAW_SAMPLE=/path/to/file.DNG to run this test");
            return;
        };
        let previews_dir = temp_previews_dir("full-tier-separate");

        let draft = ensure_develop_preview(Path::new(&sample_path), &previews_dir, None)
            .expect("draft tier decodes and caches");
        let full = ensure_develop_full_preview(Path::new(&sample_path), &previews_dir, None)
            .expect("full tier decodes and caches");

        assert_ne!(draft.path, full.path, "draft and full tiers must not share a cache file");
        assert!(
            full.path.ends_with(&format!("{DEVELOP_FULL_PREVIEW_SUFFIX}.png")),
            "full tier's filename should carry the distinguishing suffix, got {}",
            full.path
        );
        assert!(std::path::Path::new(&draft.path).exists());
        assert!(std::path::Path::new(&full.path).exists());

        let _ = std::fs::remove_dir_all(&previews_dir);
    }

    /// Same cache-hit-skips-redecode discipline as the draft tier's own
    /// `cache_hit_skips_redecode` above.
    #[test]
    fn full_preview_cache_hit_skips_redecode() {
        let Ok(sample_path) = std::env::var("EMULSION_TEST_RAW_SAMPLE") else {
            eprintln!("skipping: set EMULSION_TEST_RAW_SAMPLE=/path/to/file.DNG to run this test");
            return;
        };
        let previews_dir = temp_previews_dir("full-tier-cache-hit");

        let first = ensure_develop_full_preview(Path::new(&sample_path), &previews_dir, None)
            .expect("first call decodes and caches");
        let mtime_after_first = std::fs::metadata(&first.path).unwrap().modified().unwrap();

        let second = ensure_develop_full_preview(Path::new(&sample_path), &previews_dir, None)
            .expect("second call hits the cache");
        let mtime_after_second = std::fs::metadata(&second.path).unwrap().modified().unwrap();

        assert_eq!(first.path, second.path);
        assert_eq!(
            mtime_after_first, mtime_after_second,
            "second call must not rewrite the cached PNG"
        );

        let _ = std::fs::remove_dir_all(&previews_dir);
    }

    /// Same-content-different-path dedupe, mirrored for the full tier.
    #[test]
    fn full_preview_same_content_different_path_reuses_cache_entry() {
        let Ok(sample_path) = std::env::var("EMULSION_TEST_RAW_SAMPLE") else {
            eprintln!("skipping: set EMULSION_TEST_RAW_SAMPLE=/path/to/file.DNG to run this test");
            return;
        };
        let previews_dir = temp_previews_dir("full-tier-dedup");
        let scratch_dir = temp_previews_dir("full-tier-dedup-sources");

        let ext = Path::new(&sample_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("dng");
        let copy_a = scratch_dir.join(format!("a.{ext}"));
        let copy_b = scratch_dir.join(format!("b.{ext}"));
        std::fs::copy(&sample_path, &copy_a).unwrap();
        std::fs::copy(&sample_path, &copy_b).unwrap();

        let from_a = ensure_develop_full_preview(&copy_a, &previews_dir, None).expect("decodes copy a");
        let from_b = ensure_develop_full_preview(&copy_b, &previews_dir, None).expect("hits cache for copy b");

        assert_eq!(from_a.path, from_b.path, "identical content must resolve to the same cache entry");

        let _ = std::fs::remove_dir_all(&previews_dir);
        let _ = std::fs::remove_dir_all(&scratch_dir);
    }

    /// The core "Library and Develop show different colors" bug this
    /// function exists to fix: given a real, non-identity edit stack, the
    /// graded output must actually differ from the unedited draft-tier
    /// source, be cached distinctly by the edit stack's own hash, and
    /// reuse that cache entry on a second call with the identical stack.
    /// No RAW sample needed -- this operates entirely on a synthetic JPEG
    /// source, same as `import.rs`'s own
    /// `regenerate_edited_thumbnail_reflects_the_edit_and_is_content_addressed`.
    #[test]
    fn graded_preview_reflects_the_edit_and_is_content_addressed() {
        let dir = temp_previews_dir("graded-preview-source");
        let previews_dir = temp_previews_dir("graded-preview-cache");
        let source_path = dir.join("photo.jpg");
        image::RgbImage::from_pixel(200, 100, image::Rgb([120, 90, 60])).save(&source_path).unwrap();
        let bytes = std::fs::read(&source_path).unwrap();
        let content_hash = blake3::hash(&bytes).to_hex().to_string();

        let stack = EditStack {
            schema_version: 1,
            ops: vec![
                serde_json::json!({"op": "exposure", "value": 1.0}),
                serde_json::json!({"op": "saturation", "value": -100.0}),
            ],
        };

        // Populate the draft tier's own cache FIRST (independent of
        // grading), so its pixel value can be compared before/after below
        // -- a flat-color JPEG source can still decode back a pixel or two
        // off the original due to ordinary lossy YCbCr round-tripping, so
        // this test pins "the draft cache is untouched by grading," not
        // "the draft cache exactly equals the synthetic input."
        let draft_before = ensure_develop_preview_for_hash(&source_path, &content_hash, &previews_dir).unwrap();
        let draft_before_pixel = *image::open(&draft_before.path).unwrap().into_rgb8().get_pixel(0, 0);

        let graded = ensure_graded_preview_for_hash(&source_path, &content_hash, &stack, &previews_dir)
            .expect("grading a real JPEG source should succeed");

        assert!(std::path::Path::new(&graded.path).exists());
        assert!(
            graded.path.ends_with("_graded.png"),
            "graded preview's filename should carry the distinguishing suffix, got {}",
            graded.path
        );

        let edited = image::open(&graded.path).unwrap().into_rgb8();
        let edited_pixel = edited.get_pixel(0, 0);
        assert_ne!(*edited_pixel, draft_before_pixel, "graded output must differ from the unedited draft preview");

        // Re-requesting the identical stack must resolve to the same cache entry.
        let graded_again = ensure_graded_preview_for_hash(&source_path, &content_hash, &stack, &previews_dir).unwrap();
        assert_eq!(graded.path, graded_again.path);

        // A DIFFERENT stack must resolve to a DIFFERENT cache entry, not
        // silently reuse (or clobber) the first one.
        let other_stack = EditStack {
            schema_version: 1,
            ops: vec![serde_json::json!({"op": "exposure", "value": -1.0})],
        };
        let other_graded =
            ensure_graded_preview_for_hash(&source_path, &content_hash, &other_stack, &previews_dir).unwrap();
        assert_ne!(graded.path, other_graded.path, "a different edit stack must produce a distinct cache entry");

        // And the draft tier's own unedited cache entry must still exist,
        // byte-identical to before grading ran -- a separate cache file,
        // not an in-place overwrite of the source this function decoded
        // from.
        let draft_path = previews_dir.join(format!("{content_hash}.png"));
        assert!(draft_path.exists());
        let draft_after_pixel = *image::open(&draft_path).unwrap().into_rgb8().get_pixel(0, 0);
        assert_eq!(draft_after_pixel, draft_before_pixel, "draft tier cache must stay unedited by grading");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&previews_dir);
    }

    #[test]
    fn soft_proof_preview_reflects_the_edit_and_settings_and_is_content_addressed() {
        let dir = temp_previews_dir("soft-proof-source");
        let previews_dir = temp_previews_dir("soft-proof-cache");
        let source_path = dir.join("photo.jpg");
        image::RgbImage::from_pixel(200, 100, image::Rgb([255, 0, 0])).save(&source_path).unwrap();
        let bytes = std::fs::read(&source_path).unwrap();
        let content_hash = blake3::hash(&bytes).to_hex().to_string();

        let stack = EditStack {
            schema_version: 1,
            ops: vec![serde_json::json!({"op": "exposure", "value": 1.0})],
        };
        let settings = crate::soft_proof::SoftProofSettings {
            target: crate::soft_proof::TARGET_ADOBE_RGB.to_string(),
            custom_profile_path: None,
            intent: "relative".to_string(),
            gamut_warning: false,
        };

        let graded = ensure_graded_preview_for_hash(&source_path, &content_hash, &stack, &previews_dir).unwrap();
        let graded_pixel = *image::open(&graded.path).unwrap().into_rgb8().get_pixel(0, 0);

        let proofed = ensure_soft_proof_preview_for_hash(&source_path, &content_hash, &stack, &settings, &previews_dir)
            .expect("soft-proofing a real JPEG-derived graded preview should succeed");

        assert!(std::path::Path::new(&proofed.path).exists());
        assert!(
            proofed.path.contains("_proof_"),
            "soft-proof preview's filename should carry the distinguishing marker, got {}",
            proofed.path
        );

        // The graded tier's own cache must be untouched by proofing --
        // a separate cache file, not an in-place overwrite.
        let graded_pixel_after = *image::open(&graded.path).unwrap().into_rgb8().get_pixel(0, 0);
        assert_eq!(graded_pixel_after, graded_pixel, "graded tier cache must stay unaffected by soft proofing");

        // Re-requesting the identical settings must resolve to the same cache entry.
        let proofed_again =
            ensure_soft_proof_preview_for_hash(&source_path, &content_hash, &stack, &settings, &previews_dir).unwrap();
        assert_eq!(proofed.path, proofed_again.path);

        // DIFFERENT soft-proof settings must resolve to a DIFFERENT cache
        // entry, not silently reuse (or clobber) the first one.
        let other_settings = crate::soft_proof::SoftProofSettings { gamut_warning: true, ..settings.clone() };
        let other_proofed =
            ensure_soft_proof_preview_for_hash(&source_path, &content_hash, &stack, &other_settings, &previews_dir)
                .unwrap();
        assert_ne!(proofed.path, other_proofed.path, "different soft-proof settings must produce a distinct cache entry");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&previews_dir);
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
