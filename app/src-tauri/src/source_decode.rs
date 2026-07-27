//! Format dispatch (M2 Slice 1) — the one place that knows which source
//! formats this app supports and which decoder handles each. `raw_decode.rs`
//! (RAW via rsraw/LibRaw) and `jpeg_decode.rs` (JPEG via the `image` crate)
//! both sit *below* this module; this module doesn't know their internals,
//! it just picks one by file extension and normalizes their output/errors.
//!
//! `DecodedPreview` lives here (not in `raw_decode.rs`, where it originally
//! lived before JPEG support existed) specifically to avoid a module cycle:
//! this module already depends on `raw_decode`/`jpeg_decode` for dispatch,
//! so they can't also depend on this module just to name their return type.

use crate::{jpeg_decode, raw_decode};
use std::path::Path;

#[derive(Debug)]
pub struct DecodedPreview {
    pub width: u32,
    pub height: u32,
    /// Interleaved RGB, one byte per channel. Not color-managed yet for
    /// either format (ADR-0004 owns the input-profile -> linear-working-
    /// space step, still unwired).
    pub rgb: Vec<u8>,
}

const RAW_EXTENSIONS: &[&str] = &[
    "cr2", "cr3", "nef", "arw", "dng", "raf", "orf", "rw2", "pef", "srw", "x3f", "3fr", "erf",
    "kdc", "mrw", "raw", "rwl",
];
const JPEG_EXTENSIONS: &[&str] = &["jpg", "jpeg"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Raw,
    Jpeg,
}

impl ImageFormat {
    /// Not exhaustive of everything LibRaw/the `image` crate could
    /// plausibly handle — a first filter, matching `import.rs`'s own prior
    /// caveat about `RAW_EXTENSIONS`. `RawImage::open`/`jpeg_decode::decode`
    /// still have the final say per file.
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        if RAW_EXTENSIONS.contains(&ext.as_str()) {
            Some(Self::Raw)
        } else if JPEG_EXTENSIONS.contains(&ext.as_str()) {
            Some(Self::Jpeg)
        } else {
            None
        }
    }

    /// Backs the `get_supported_extensions` Tauri command -- the frontend's
    /// file-picker filter is populated from this at runtime instead of
    /// hand-duplicating the list into JS, where it could silently drift
    /// from this one as more formats are added later.
    pub fn all_supported_extensions() -> Vec<String> {
        RAW_EXTENSIONS
            .iter()
            .chain(JPEG_EXTENSIONS.iter())
            .map(|s| s.to_string())
            .collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("unsupported file format: {0}")]
    UnsupportedFormat(String),
    #[error(transparent)]
    Raw(#[from] raw_decode::DecodeError),
    #[error(transparent)]
    Jpeg(#[from] jpeg_decode::DecodeError),
}

fn unsupported(path: &Path) -> DecodeError {
    DecodeError::UnsupportedFormat(path.to_string_lossy().to_string())
}

/// Full-resolution decode -- the Export pipeline's caller.
pub fn decode_preview(path: &Path) -> Result<DecodedPreview, DecodeError> {
    match ImageFormat::from_path(path) {
        Some(ImageFormat::Raw) => Ok(raw_decode::decode_preview(path)?),
        Some(ImageFormat::Jpeg) => Ok(jpeg_decode::decode(path)?),
        None => Err(unsupported(path)),
    }
}

/// Develop-canvas decode. For JPEG this is identical to `decode_preview` --
/// there's no demosaic step to skip, so no meaningful draft-vs-full
/// distinction exists the way RAW's (best-effort, not guaranteed-smaller
/// per Slice 3/4's own finding) half-size request has.
pub fn decode_develop_preview(path: &Path) -> Result<DecodedPreview, DecodeError> {
    match ImageFormat::from_path(path) {
        Some(ImageFormat::Raw) => Ok(raw_decode::decode_develop_preview(path)?),
        Some(ImageFormat::Jpeg) => Ok(jpeg_decode::decode(path)?),
        None => Err(unsupported(path)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatches_by_extension_case_insensitively() {
        assert_eq!(ImageFormat::from_path(Path::new("a.CR3")), Some(ImageFormat::Raw));
        assert_eq!(ImageFormat::from_path(Path::new("a.dng")), Some(ImageFormat::Raw));
        assert_eq!(ImageFormat::from_path(Path::new("a.JPG")), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_path(Path::new("a.jpeg")), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_path(Path::new("a.txt")), None);
        assert_eq!(ImageFormat::from_path(Path::new("no-extension")), None);
    }

    #[test]
    fn unsupported_format_returns_a_clean_error_not_a_panic() {
        let result = decode_preview(Path::new("/nonexistent/not-an-image.txt"));
        assert!(matches!(result, Err(DecodeError::UnsupportedFormat(_))));
    }
}
