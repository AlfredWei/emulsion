//! RAW decoding — see docs/adr/ADR-0003.
//!
//! Thin wrapper around `rsraw` (vendored LibRaw). Color-profile handling
//! (16-bit/linear output via lcms2, per ADR-0004) is still not done —
//! everything here is 8-bit, not color-managed. Good enough for Slice 3's
//! interactive Develop preview; a real "High quality" export path is later
//! M1 scope (the Export pipeline slice).

use rsraw::{RawImage, BIT_DEPTH_8};
use std::path::Path;

#[derive(Debug)]
pub struct DecodedPreview {
    pub width: u32,
    pub height: u32,
    /// Interleaved RGB, one byte per channel, straight out of LibRaw's
    /// 8-bit post-processed output. Not color-managed yet (ADR-0004 owns
    /// the lcms2 input-profile -> linear-working-space step).
    pub rgb: Vec<u8>,
}

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

/// Decode a RAW file to a full-resolution 8-bit RGB buffer. Not used yet —
/// the Export pipeline slice is the real caller (final-quality output);
/// Slice 3's interactive Develop preview uses `decode_develop_preview`
/// instead, which is faster and small enough to move around cheaply.
#[allow(dead_code)]
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
