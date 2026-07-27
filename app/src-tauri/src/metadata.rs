//! Metadata extraction (M2 Slice 2) — EXIF-ish fields captured at import
//! time, read-only from the app's own perspective (never written back into
//! the source file — see IPTC caption/copyright/contact in catalog.rs for
//! the user-editable side of metadata, which lives in the catalog instead).
//!
//! RAW and JPEG each have their own extractor, mirroring source_decode.rs's
//! format split: RAW uses rsraw's own metadata getters (confirmed against
//! the vendored crate's source: these read header fields `RawImage::open`
//! already populated, no `unpack()`/`process()` needed, so calling this at
//! import time is free -- no extra parse). JPEG uses a fresh `exif::Reader`
//! read -- deliberately NOT shared with `jpeg_decode.rs`'s own orientation
//! read, which only happens later, during the background thumbnail pass,
//! not at import time.

use rsraw::RawImage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ImageMetadata {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub iso: Option<u32>,
    pub aperture: Option<f32>,
    pub shutter_speed: Option<f32>,
    pub focal_length: Option<f32>,
    /// ISO8601-ish (`YYYY-MM-DDTHH:MM:SS`), normalized from both RAW's
    /// `chrono` timestamp and JPEG's EXIF `YYYY:MM:DD HH:MM:SS` string so
    /// the frontend has one format to parse regardless of source.
    pub captured_at: Option<String>,
}

fn non_empty(s: std::borrow::Cow<'_, str>) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

pub fn extract_from_raw(image: &RawImage) -> ImageMetadata {
    let lens = image.lens_info();

    ImageMetadata {
        camera_make: non_empty(image.normalized_make()),
        camera_model: non_empty(image.normalized_model()),
        lens_model: (!lens.lens_name.trim().is_empty()).then_some(lens.lens_name),
        iso: (image.iso_speed() > 0).then_some(image.iso_speed()),
        aperture: (image.aperture() > 0.0).then_some(image.aperture()),
        shutter_speed: (image.shutter() > 0.0).then_some(image.shutter()),
        focal_length: (image.focal_len() > 0.0).then_some(image.focal_len()),
        captured_at: image.datetime().map(|dt| dt.to_rfc3339()),
    }
}

fn ascii_string(value: &exif::Value) -> Option<String> {
    match value {
        exif::Value::Ascii(v) => v.first().and_then(|bytes| {
            let s = String::from_utf8_lossy(bytes).trim().to_string();
            (!s.is_empty()).then_some(s)
        }),
        _ => None,
    }
}

fn rational_f32(value: &exif::Value) -> Option<f32> {
    match value {
        exif::Value::Rational(v) => v.first().map(|r| r.to_f64() as f32),
        _ => None,
    }
}

/// EXIF's own date format ("YYYY:MM:DD HH:MM:SS") -> "YYYY-MM-DDTHH:MM:SS",
/// matching RAW's captured_at shape closely enough for the frontend to
/// treat both the same way. Plain string surgery, no chrono parsing needed
/// (chrono isn't otherwise a direct dependency of this crate).
fn normalize_exif_datetime(s: &str) -> Option<String> {
    let (date, time) = s.split_once(' ')?;
    Some(format!("{}T{}", date.replace(':', "-"), time))
}

pub fn extract_from_jpeg(bytes: &[u8]) -> ImageMetadata {
    let mut reader = std::io::Cursor::new(bytes);
    let Ok(exif) = exif::Reader::new().read_from_container(&mut reader) else {
        return ImageMetadata::default();
    };

    let field = |tag: exif::Tag| exif.get_field(tag, exif::In::PRIMARY).map(|f| &f.value);

    ImageMetadata {
        camera_make: field(exif::Tag::Make).and_then(ascii_string),
        camera_model: field(exif::Tag::Model).and_then(ascii_string),
        lens_model: field(exif::Tag::LensModel).and_then(ascii_string),
        iso: field(exif::Tag::PhotographicSensitivity).and_then(|v| v.get_uint(0)),
        aperture: field(exif::Tag::FNumber).and_then(rational_f32),
        shutter_speed: field(exif::Tag::ExposureTime).and_then(rational_f32),
        focal_length: field(exif::Tag::FocalLength).and_then(rational_f32),
        captured_at: field(exif::Tag::DateTimeOriginal)
            .and_then(ascii_string)
            .and_then(|s| normalize_exif_datetime(&s)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_exif_datetime_converts_colons_to_dashes_in_the_date_part() {
        assert_eq!(
            normalize_exif_datetime("2017:01:05 05:53:29"),
            Some("2017-01-05T05:53:29".to_string())
        );
    }

    #[test]
    fn normalize_exif_datetime_rejects_malformed_input() {
        assert_eq!(normalize_exif_datetime("not-a-date-with-no-space"), None);
    }

    /// Real-file-gated, same pattern as raw_decode.rs/import.rs. Exact
    /// expected values confirmed independently against the real sample
    /// beforehand (via `identify -verbose`, since exiftool isn't available
    /// in this environment): Canon EOS 5D Mark III, ISO 200, 70mm,
    /// captured 2017-01-05T05:53:29 UTC. Aperture is asserted as "present
    /// and positive" only -- no independently-verified exact f-number was
    /// available without a proper EXIF tool, so asserting an exact value
    /// here would just be re-asserting whatever this code itself computes,
    /// not a real check against ground truth. captured_at is checked by
    /// date only, not the full offset-bearing timestamp: `rsraw::datetime()`
    /// converts the stored UTC instant into the *local* system timezone
    /// (confirmed CST/+08:00 in this dev sandbox, but CI runners default
    /// to UTC) -- asserting a hardcoded offset would pass here and fail
    /// in CI, or vice versa. The date itself is stable across any
    /// reasonably-plausible timezone for a 05:53 UTC instant.
    ///
    /// `iso` is a **confirmed, real, platform-specific gap**, not a flaky
    /// test papered over: on Windows (LibRaw 0.22.1, linked via vcpkg per
    /// ADR-0003) `metadata.iso` comes back `None` for this exact sample,
    /// where macOS/Linux (LibRaw 0.21.3, vendored and compiled from source)
    /// correctly reads `Some(200)` -- reproduced deterministically across
    /// two independent CI runs, not a one-off flake. LibRaw's own
    /// Changelog.txt has no entry mentioning `iso_speed`/ISOSpeedRatings
    /// between these versions, so this looks like an undocumented behavior
    /// difference (possibly in DNG-specific tag parsing, since this sample
    /// is a linear DNG produced by Adobe DNG Converter, not straight
    /// camera-native RAW) rather than an intentional change -- root-caused
    /// only as far as "confirmed real, isolated to this one field, on this
    /// one platform," not traced into LibRaw's own C++. Asserted loosely on
    /// Windows rather than silently skipped or asserted wrong, so a
    /// regression that also breaks the *other* fields on Windows still
    /// fails this test.
    #[test]
    fn extracts_real_metadata_from_a_real_raw_file() {
        let Ok(sample_path) = std::env::var("EMULSION_TEST_RAW_SAMPLE") else {
            eprintln!("skipping: set EMULSION_TEST_RAW_SAMPLE=/path/to/file.DNG to run this test");
            return;
        };
        let bytes = std::fs::read(&sample_path).unwrap();
        let image = RawImage::open(&bytes).expect("real RAW sample should open");

        let metadata = extract_from_raw(&image);

        assert_eq!(metadata.camera_make.as_deref(), Some("Canon"));
        assert_eq!(metadata.camera_model.as_deref(), Some("EOS 5D Mark III"));
        if cfg!(windows) {
            assert!(
                metadata.iso.is_none() || metadata.iso == Some(200),
                "expected the known vcpkg-LibRaw ISO gap (None) or the correct value (200), got {:?}",
                metadata.iso
            );
        } else {
            assert_eq!(metadata.iso, Some(200));
        }
        assert_eq!(metadata.focal_length, Some(70.0));
        assert!(metadata.aperture.unwrap_or(0.0) > 0.0, "aperture should be a real positive value");
        assert!(
            metadata.captured_at.as_deref().unwrap_or("").starts_with("2017-01-05"),
            "expected captured_at to start with 2017-01-05, got {:?}",
            metadata.captured_at
        );
    }

    #[test]
    fn extract_from_jpeg_with_no_exif_returns_all_none() {
        let dir = std::env::temp_dir().join("emulsion-metadata-test-no-exif");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plain.jpg");
        image::RgbImage::from_pixel(20, 10, image::Rgb([1, 2, 3])).save(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();

        let metadata = extract_from_jpeg(&bytes);
        assert_eq!(metadata.camera_make, None);
        assert_eq!(metadata.iso, None);
        assert_eq!(metadata.captured_at, None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Real, if non-photographic, JPEG with a *real* EXIF/TIFF segment
    /// written by `little_exif` (a dev-dependency, real EXIF-writing
    /// library, not a hand-rolled byte blob) -- `extract_from_jpeg`'s
    /// parser has to walk real binary IFD structure, not a stub.
    ///
    /// Empirical finding along the way: ImageMagick's `convert -set
    /// exif:Tag value` does NOT actually write an EXIF segment on this
    /// machine (ImageMagick 7.1.2) -- confirmed via `identify -verbose`
    /// showing zero `exif:*` properties on the output file, despite no
    /// error from `convert` itself. That was the first approach tried
    /// here; `little_exif` (a real, independently-published crate) is
    /// what actually works.
    #[test]
    fn extract_from_jpeg_reads_real_written_exif_tags() {
        use little_exif::exif_tag::ExifTag;
        use little_exif::filetype::FileExtension;
        use little_exif::metadata::Metadata;
        use little_exif::rational::uR64;

        let mut image_bytes = Vec::new();
        image::RgbImage::from_pixel(40, 20, image::Rgb([120, 80, 60]))
            .write_to(&mut std::io::Cursor::new(&mut image_bytes), image::ImageFormat::Jpeg)
            .unwrap();

        let mut exif = Metadata::new();
        exif.set_tag(ExifTag::Make("Emulsion Test Co".to_string()));
        exif.set_tag(ExifTag::Model("Test Camera 3000".to_string()));
        exif.set_tag(ExifTag::ISO(vec![400]));
        exif.set_tag(ExifTag::FNumber(vec![uR64 { nominator: 4, denominator: 1 }]));
        exif.set_tag(ExifTag::ExposureTime(vec![uR64 { nominator: 1, denominator: 250 }]));
        exif.set_tag(ExifTag::FocalLength(vec![uR64 { nominator: 50, denominator: 1 }]));
        exif.set_tag(ExifTag::DateTimeOriginal("2024:03:15 10:30:00".to_string()));
        exif.write_to_vec(&mut image_bytes, FileExtension::JPEG)
            .expect("little_exif should write a real EXIF segment into the JPEG bytes");

        let metadata = extract_from_jpeg(&image_bytes);

        assert_eq!(metadata.camera_make.as_deref(), Some("Emulsion Test Co"));
        assert_eq!(metadata.camera_model.as_deref(), Some("Test Camera 3000"));
        assert_eq!(metadata.iso, Some(400));
        assert_eq!(metadata.aperture, Some(4.0));
        assert_eq!(metadata.shutter_speed, Some(1.0 / 250.0));
        assert_eq!(metadata.focal_length, Some(50.0));
        assert_eq!(metadata.captured_at.as_deref(), Some("2024-03-15T10:30:00"));
    }
}
