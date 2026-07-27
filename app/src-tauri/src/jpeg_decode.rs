//! JPEG decoding (M2 Slice 1) — via the `image` crate, not `rsraw`/LibRaw.
//! No demosaic step exists for JPEG, so there's no draft-vs-full distinction
//! the way RAW has (see source_decode.rs) -- one function serves both the
//! Develop-preview and full-resolution-export callers.

use crate::source_decode::DecodedPreview;
use image::imageops;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("could not read file from disk: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not decode JPEG: {0}")]
    Image(#[from] image::ImageError),
}

/// EXIF orientation values 1-8 per the spec; anything else (including no
/// tag at all) is treated as 1 (no transform). The `image` crate's own
/// decode does not auto-apply this -- RAW files don't have this problem
/// (LibRaw already applies camera rotation internally), but an untouched
/// JPEG would render sideways for any portrait/rotated shot, a visible
/// correctness bug distinct from "EXIF fields aren't displayed yet" (which
/// is real, but later M2 scope).
fn read_orientation(path: &Path) -> u32 {
    let Ok(file) = std::fs::File::open(path) else { return 1 };
    let mut reader = std::io::BufReader::new(file);
    let Ok(exif) = exif::Reader::new().read_from_container(&mut reader) else { return 1 };
    exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
        .filter(|v| (1..=8).contains(v))
        .unwrap_or(1)
}

fn apply_orientation(image: image::RgbImage, orientation: u32) -> image::RgbImage {
    match orientation {
        2 => imageops::flip_horizontal(&image),
        3 => imageops::rotate180(&image),
        4 => imageops::flip_vertical(&image),
        5 => imageops::flip_horizontal(&imageops::rotate90(&image)),
        6 => imageops::rotate90(&image),
        7 => imageops::flip_horizontal(&imageops::rotate270(&image)),
        8 => imageops::rotate270(&image),
        _ => image,
    }
}

pub fn decode(path: &Path) -> Result<DecodedPreview, DecodeError> {
    let orientation = read_orientation(path);
    let rgb = image::open(path)?.to_rgb8();
    let rgb = apply_orientation(rgb, orientation);

    Ok(DecodedPreview {
        width: rgb.width(),
        height: rgb.height(),
        rgb: rgb.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("emulsion-jpeg-decode-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// No sample needed -- writes a real, valid small JPEG via the `image`
    /// crate's own encoder, decodes it back, confirms round-trip dimensions.
    #[test]
    fn decodes_a_real_jpeg_with_no_orientation_tag() {
        let dir = temp_dir("no-orientation");
        let path = dir.join("plain.jpg");
        let img = image::RgbImage::from_pixel(40, 20, image::Rgb([200, 100, 50]));
        img.save(&path).unwrap();

        let decoded = decode(&path).expect("a real JPEG should decode successfully");
        assert_eq!(decoded.width, 40);
        assert_eq!(decoded.height, 20);
        assert_eq!(decoded.rgb.len(), 40 * 20 * 3);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_orientation_6_rotates_90_degrees() {
        // A 40x20 image with orientation 6 (rotate 90 CW) should come out 20x40.
        let img = image::RgbImage::from_pixel(40, 20, image::Rgb([1, 2, 3]));
        let rotated = apply_orientation(img, 6);
        assert_eq!(rotated.width(), 20);
        assert_eq!(rotated.height(), 40);
    }

    #[test]
    fn apply_orientation_1_is_a_passthrough() {
        let img = image::RgbImage::from_pixel(40, 20, image::Rgb([1, 2, 3]));
        let unchanged = apply_orientation(img, 1);
        assert_eq!((unchanged.width(), unchanged.height()), (40, 20));
    }

    #[test]
    fn nonexistent_file_returns_a_clean_error_not_a_panic() {
        let result = decode(Path::new("/nonexistent/not-a-real-jpeg.jpg"));
        assert!(result.is_err());
    }

    #[test]
    fn non_jpeg_file_returns_a_clean_error_not_a_panic() {
        let dir = temp_dir("not-a-jpeg");
        let path = dir.join("not-a-jpeg.jpg");
        std::fs::write(&path, b"this is definitely not a real JPEG").unwrap();

        let result = decode(&path);

        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_err());
    }
}
