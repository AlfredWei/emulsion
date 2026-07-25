//! RAW decoding (M0 spike) — see docs/adr/ADR-0003.
//!
//! Thin wrapper around `rsraw` (vendored LibRaw). M0 scope is proving the
//! FFI boundary works end-to-end: open a file, decode, get a pixel buffer
//! + dimensions back. Color-profile handling, 16-bit/linear output, and
//! downsampling-to-preview-resolution (ADR-0004) are M1 work — this is
//! deliberately the simplest possible path through the FFI boundary.

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

/// Decode a RAW file to an 8-bit RGB preview buffer.
///
/// This exists to validate the FFI boundary (ADR-0003) for M0, not as the
/// final decode path — M1's import pipeline needs the linear/high-bit-depth
/// output and the downsample-to-preview step described in ADR-0004.
pub fn decode_preview(path: &Path) -> Result<DecodedPreview, DecodeError> {
    let bytes = std::fs::read(path)?;

    let mut image = RawImage::open(&bytes).map_err(|e| DecodeError::LibRaw(e.to_string()))?;
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

#[cfg(test)]
mod tests {
    use super::*;

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
