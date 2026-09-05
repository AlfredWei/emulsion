//! RAW decoding — see docs/adr/ADR-0003.
//!
//! Thin wrapper around `rsraw` (vendored LibRaw). Color-profile handling
//! (16-bit/linear output via lcms2, per ADR-0004) is still not done —
//! everything here is 8-bit, not color-managed. Good enough for Slice 3's
//! interactive Develop preview; a real "High quality" export path is later
//! M1 scope (the Export pipeline slice).

use crate::source_decode::DecodedPreview;
use rsraw::{RawImage, BIT_DEPTH_16, BIT_DEPTH_8};
use std::path::Path;
use std::sync::Mutex;

/// LibRaw is not safely reentrant across threads: parts of its internal
/// processing (inherited from the legacy dcraw core it wraps) rely on
/// non-thread-local state that concurrent decodes can corrupt. Found via
/// a real, reproducible Windows CI failure -- `cargo test`'s default
/// parallel execution ran two of this module's tests' decodes at the
/// same time, and one's output buffer came back at exactly half the
/// correct size (the other decode's `output_bps` setting had leaked
/// across). Every real LibRaw call anywhere in this crate -- open
/// through process/extract_thumbs -- must take this lock, not just the
/// two functions below: `import.rs`'s embedded-thumbnail extraction
/// during import runs on its own `spawn_blocking` task and can race a
/// `decode()`/`decode_linear()` call from a concurrent Develop preview,
/// HDR merge, or (since this project's on-demand priority-thumbnail
/// feature) an `ensure_thumbnail` call deliberately racing a background
/// backfill pass.
pub(crate) static LIBRAW_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("could not read RAW file from disk: {0}")]
    Io(#[from] std::io::Error),
    // rsraw v0.1.1 keeps its `Error` type in a private module and does not
    // re-export it (`rsraw::Error` does not resolve) — its own methods
    // return it, but callers outside the crate can't name the type. It
    // does implement `Display`, so we convert to a string at the call
    // site instead of using `#[from]`. Worth revisiting if rsraw fixes
    // this (see ADR-0003's open Windows/MSVC question too).
    #[error("LibRaw could not decode this file: {0}")]
    LibRaw(String),
}

fn decode(path: &Path, half_size: bool) -> Result<DecodedPreview, DecodeError> {
    let bytes = std::fs::read(path)?;
    let _guard = LIBRAW_LOCK.lock().unwrap();

    let mut image = RawImage::open(&bytes).map_err(|e| DecodeError::LibRaw(e.to_string()))?;
    image.set_half_size(half_size);
    image.unpack().map_err(|e| DecodeError::LibRaw(e.to_string()))?;
    let processed = image
        .process::<BIT_DEPTH_8>()
        .map_err(|e| DecodeError::LibRaw(e.to_string()))?;

    Ok(DecodedPreview {
        width: processed.width(),
        height: processed.height(),
        rgb: processed.to_vec(),
    })
}

/// Decode a RAW file to a full-resolution 8-bit RGB buffer -- the Export
/// pipeline's real caller (final-quality output, M1 Slice 5, see
/// export.rs). Slice 3's interactive Develop preview uses
/// `decode_develop_preview` instead, which is faster and small enough to
/// move around cheaply.
pub fn decode_preview(path: &Path) -> Result<DecodedPreview, DecodeError> {
    decode(path, false)
}

/// Decode a RAW file for the interactive Develop canvas, requesting LibRaw's
/// fast Bayer pixel-binning half-size decode -- matches the "Draft quality"
/// mode concept in docs/ux/UX-DESIGN.md §5. **This is a best-effort
/// optimization, not a size guarantee**: confirmed empirically that it has
/// no effect on files with no Bayer mosaic left to bin (e.g. a "linear
/// DNG"), so callers that need a bounded preview size must still resize
/// explicitly afterward (the Tauri command layer in lib.rs does this) --
/// don't assume this alone keeps the buffer small. Full-resolution decode
/// for "Standard"/"High" quality modes is real PRD scope, not implemented
/// yet.
pub fn decode_develop_preview(path: &Path) -> Result<DecodedPreview, DecodeError> {
    decode(path, true)
}

/// A genuinely linear-light, non-auto-brightened decode -- HDR merge's own
/// input (RFC-0003 §3.1), distinct from every other decode path in this
/// module which produces display-referred, auto-exposed 8-bit output.
/// RAW-only by construction (this function only exists on `raw_decode`,
/// never `jpeg_decode`/`source_decode`) -- HDR merge v1 requires every
/// bracket member to be RAW (see RFC-0003 §2's non-goal on JPEG brackets),
/// so there is no JPEG counterpart to build.
pub struct DecodedLinear {
    pub width: u32,
    pub height: u32,
    /// Interleaved RGB, one f32 per channel, normalized by 65535.0 from
    /// LibRaw's linear 16-bit output. Deliberately NOT clamped to [0, 1] --
    /// a well-exposed bright frame can legitimately read above 1.0 before
    /// hdr_merge.rs scales it by that frame's own exposure ratio; clamping
    /// here would silently discard real highlight data this feature exists
    /// to preserve.
    pub rgb: Vec<f32>,
}

pub fn decode_linear(path: &Path) -> Result<DecodedLinear, DecodeError> {
    let bytes = std::fs::read(path)?;
    let _guard = LIBRAW_LOCK.lock().unwrap();

    let mut image = RawImage::open(&bytes).map_err(|e| DecodeError::LibRaw(e.to_string()))?;
    // Fixed (not auto) white balance so color stays consistent frame-to-
    // frame across a bracket -- auto-WB could legitimately pick a
    // different white point for a much-brighter or much-darker exposure
    // of the same scene, which would corrupt the merge's color accuracy
    // even though each frame's own WB choice would look fine in isolation.
    image.set_use_camera_wb(true);
    image.set_linear_output();
    image.unpack().map_err(|e| DecodeError::LibRaw(e.to_string()))?;
    let processed = image
        .process::<BIT_DEPTH_16>()
        .map_err(|e| DecodeError::LibRaw(e.to_string()))?;

    let width = processed.width();
    let height = processed.height();
    // TEMPORARY diagnostic for a Windows-CI-only buffer-size mismatch
    // (`linear.rgb.len() != width*height*3`) that a LIBRAW_LOCK mutex
    // (ruling out a cross-thread race) did NOT fix -- ProcessedImage's own
    // Debug impl reports LibRaw's raw `colors`/`bits`/`data_size` fields
    // directly, which the public DecodedLinear struct doesn't expose. Ship
    // this in the same commit that removes it once the real cause (a
    // vcpkg-resolved LibRaw version drift is the leading suspect) is
    // confirmed from a real Windows CI run's output.
    eprintln!("EMULSION_DIAG decode_linear: {processed:?}, iter().count()={}", processed.iter().count());
    let rgb = processed.iter().map(|&v| v as f32 / 65535.0).collect();

    Ok(DecodedLinear { width, height, rgb })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real-file decode, gated behind an env var rather than a fixture
    /// checked into the repo: RAW sample files are large (multi-MB) and
    /// of varying/unclear provenance, so they don't belong in git history.
    /// Point `EMULSION_TEST_RAW_SAMPLE` at a real RAW/DNG file locally to
    /// exercise this; CI and default `cargo test` runs skip it cleanly.
    #[test]
    fn decodes_a_real_raw_file_when_a_sample_is_provided() {
        let Ok(sample_path) = std::env::var("EMULSION_TEST_RAW_SAMPLE") else {
            eprintln!(
                "skipping: set EMULSION_TEST_RAW_SAMPLE=/path/to/file.DNG to run this test"
            );
            return;
        };

        let preview = decode_preview(Path::new(&sample_path))
            .expect("a real RAW file should decode successfully");

        assert!(preview.width > 0 && preview.height > 0);
        assert_eq!(
            preview.rgb.len(),
            preview.width as usize * preview.height as usize * 3,
            "8-bit RGB buffer should be exactly width * height * 3 bytes"
        );
    }

    /// `set_half_size` is LibRaw's fast Bayer pixel-binning optimization --
    /// it has no effect on files with no Bayer mosaic left to bin (e.g. a
    /// "linear DNG" from Adobe's DNG Converter, which is already
    /// demosaiced). Confirmed empirically against this project's real test
    /// sample: full and half-size decode produced identical 3960x2640
    /// output, not smaller. So this is a best-effort optimization, not a
    /// guarantee -- the Tauri command layer (lib.rs) is what actually
    /// bounds the preview's final size via an explicit resize, regardless
    /// of whether this helped. This test just confirms the call doesn't
    /// error and still produces a valid buffer either way.
    #[test]
    fn develop_preview_decodes_successfully_with_half_size_requested() {
        let Ok(sample_path) = std::env::var("EMULSION_TEST_RAW_SAMPLE") else {
            eprintln!(
                "skipping: set EMULSION_TEST_RAW_SAMPLE=/path/to/file.DNG to run this test"
            );
            return;
        };

        let half = decode_develop_preview(Path::new(&sample_path)).expect("half-size decode succeeds");
        assert!(half.width > 0 && half.height > 0);
        assert_eq!(half.rgb.len(), half.width as usize * half.height as usize * 3);
    }

    /// RFC-0003's linear decode path -- same real-file/env-var gate as the
    /// two tests above. Confirms the buffer is the right shape and, more
    /// importantly, that `no_auto_bright`/linear `gamm` actually took
    /// effect: a linear decode of a normally-exposed photo should have a
    /// visibly LOWER mean brightness than a standard auto-brightened
    /// decode of the same file (auto-bright's whole job is to brighten a
    /// raw linear signal up to a pleasing display level) -- a real,
    /// specific regression check, not just "did it return without error".
    #[test]
    fn linear_decode_produces_a_darker_unbrightened_buffer_than_standard_decode() {
        let Ok(sample_path) = std::env::var("EMULSION_TEST_RAW_SAMPLE") else {
            eprintln!(
                "skipping: set EMULSION_TEST_RAW_SAMPLE=/path/to/file.DNG to run this test"
            );
            return;
        };
        let path = Path::new(&sample_path);

        let linear = decode_linear(path).expect("linear decode should succeed on a real RAW file");
        assert!(linear.width > 0 && linear.height > 0);
        assert_eq!(linear.rgb.len(), linear.width as usize * linear.height as usize * 3);
        assert!(
            linear.rgb.iter().all(|&v| v.is_finite() && v >= 0.0),
            "linear decode should never produce NaN/negative values"
        );

        let standard = decode_preview(path).expect("standard decode should also succeed");
        let linear_mean = linear.rgb.iter().sum::<f32>() / linear.rgb.len() as f32;
        let standard_mean =
            standard.rgb.iter().map(|&v| v as f32 / 255.0).sum::<f32>() / standard.rgb.len() as f32;
        assert!(
            linear_mean < standard_mean,
            "linear (no_auto_bright, gamm=[1,1]) decode (mean {linear_mean}) should be darker than \
             standard auto-brightened decode (mean {standard_mean}) -- if not, set_linear_output() \
             may not be taking effect"
        );
    }

    /// No sample RAW file is available in this environment yet (see
    /// PROGRESS.md). This test only proves the FFI boundary itself is
    /// sound: LibRaw's own error path for a nonexistent/invalid file
    /// returns a clean `Err`, not a panic or a crash — real-file decode
    /// is validated separately once a sample file is supplied.
    #[test]
    fn nonexistent_file_returns_a_clean_error_not_a_panic() {
        let result = decode_preview(Path::new("/nonexistent/not-a-real-raw-file.CR3"));
        assert!(result.is_err());
    }

    /// A file that exists but clearly isn't a RAW file should also fail
    /// cleanly through LibRaw's own format-detection error path, not panic.
    #[test]
    fn non_raw_file_returns_a_clean_error_not_a_panic() {
        let dir = std::env::temp_dir();
        let path = dir.join("emulsion-m0-not-a-raw-file.txt");
        std::fs::write(&path, b"this is definitely not a RAW file").unwrap();

        let result = decode_preview(&path);

        let _ = std::fs::remove_file(&path);
        assert!(result.is_err());
    }
}
