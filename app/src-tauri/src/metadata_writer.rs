//! EXIF and IPTC writing for exported JPEGs.
//!
//! Catalog-free like `export.rs`: callers resolve an `ExportMetadata` from
//! the catalog under their own lock and hand it in. The user chooses per
//! export whether EXIF and/or IPTC is written (`MetadataWriteOptions`), and
//! separately whether GPS is included within EXIF -- location is the one
//! field that can leak something the user didn't mean to share.
//!
//! Split of responsibility, so each toggle means one thing:
//! - EXIF: camera/lens/exposure settings, capture time, GPS.
//! - IPTC: caption, copyright, contact, keywords.
//!
//! Deliberately not written: pixel dimensions and orientation (the export
//! renders upright at its own size), metering mode/flash (stored as
//! display strings, not the original enum codes), and XMP (IPTC-IIM is what
//! the catalog's fields map onto directly; XMP is a possible follow-up).

use little_exif::exif_tag::ExifTag;
use little_exif::filetype::FileExtension;
use little_exif::metadata::Metadata;
use little_exif::rational::{iR64, uR64};
use serde::Deserialize;

#[derive(Debug, Clone, Default)]
pub struct ExportMetadata {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub iso: Option<u32>,
    pub aperture: Option<f32>,
    pub shutter_speed: Option<f32>,
    pub focal_length: Option<f32>,
    pub exposure_bias: Option<f32>,
    /// Catalog form, e.g. `2024-03-15T10:30:00`.
    pub captured_at: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude: Option<f32>,
    pub caption: Option<String>,
    pub copyright: Option<String>,
    pub contact: Option<String>,
    pub keywords: Vec<String>,
}

/// All-false by default so a caller that omits the field keeps the
/// pre-existing "bare JPEG" behavior.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct MetadataWriteOptions {
    #[serde(default)]
    pub exif: bool,
    #[serde(default)]
    pub iptc: bool,
    /// Only consulted when `exif` is true.
    #[serde(default)]
    pub gps: bool,
}

impl MetadataWriteOptions {
    pub fn any(&self) -> bool {
        self.exif || self.iptc
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MetadataWriteError {
    #[error("EXIF write failed: {0}")]
    Exif(String),
    #[error("IPTC block too large to embed in a JPEG segment")]
    IptcTooLarge,
    #[error("not a JPEG (missing SOI marker)")]
    NotJpeg,
}

/// Returns `jpeg` with the requested metadata embedded. A no-op (returns
/// the input unchanged) when nothing was requested or there is nothing to
/// write for the requested groups.
pub fn apply(
    mut jpeg: Vec<u8>,
    meta: &ExportMetadata,
    options: &MetadataWriteOptions,
) -> Result<Vec<u8>, MetadataWriteError> {
    if !jpeg.starts_with(&[0xFF, 0xD8]) {
        return Err(MetadataWriteError::NotJpeg);
    }
    if options.exif {
        if let Some(exif) = build_exif(meta, options.gps) {
            exif.write_to_vec(&mut jpeg, FileExtension::JPEG)
                .map_err(|e| MetadataWriteError::Exif(e.to_string()))?;
        }
    }
    if options.iptc {
        let iim = build_iim(meta);
        if !iim.is_empty() {
            insert_app13(&mut jpeg, &iim)?;
        }
    }
    Ok(jpeg)
}

fn rational(value: f64, denominator: u32) -> uR64 {
    uR64 {
        nominator: (value * denominator as f64).round() as u32,
        denominator,
    }
}

fn dms(value: f64) -> Vec<uR64> {
    let abs = value.abs();
    let degrees = abs.floor();
    let minutes_full = (abs - degrees) * 60.0;
    let minutes = minutes_full.floor();
    let seconds = (minutes_full - minutes) * 60.0;
    vec![
        uR64 { nominator: degrees as u32, denominator: 1 },
        uR64 { nominator: minutes as u32, denominator: 1 },
        rational(seconds, 10_000),
    ]
}

/// `2024-03-15T10:30:00` -> `2024:03:15 10:30:00`; `None` if the catalog
/// string isn't in that shape (never write a malformed EXIF date).
fn exif_datetime(captured_at: &str) -> Option<String> {
    let (date, time) = captured_at.split_once('T')?;
    if date.len() != 10 || time.len() < 8 {
        return None;
    }
    Some(format!("{} {}", date.replace('-', ":"), &time[..8]))
}

/// `None` when there is nothing to write.
fn build_exif(meta: &ExportMetadata, include_gps: bool) -> Option<Metadata> {
    let mut tags: Vec<ExifTag> = Vec::new();
    if let Some(v) = &meta.camera_make {
        tags.push(ExifTag::Make(v.clone()));
    }
    if let Some(v) = &meta.camera_model {
        tags.push(ExifTag::Model(v.clone()));
    }
    if let Some(v) = &meta.lens_model {
        tags.push(ExifTag::LensModel(v.clone()));
    }
    if let Some(v) = meta.iso {
        tags.push(ExifTag::ISO(vec![u16::try_from(v).unwrap_or(u16::MAX)]));
    }
    if let Some(v) = meta.aperture {
        tags.push(ExifTag::FNumber(vec![rational(v as f64, 100)]));
    }
    if let Some(v) = meta.shutter_speed.filter(|s| *s > 0.0) {
        let time = if v < 1.0 {
            uR64 { nominator: 1, denominator: (1.0 / v as f64).round().max(1.0) as u32 }
        } else {
            rational(v as f64, 10)
        };
        tags.push(ExifTag::ExposureTime(vec![time]));
    }
    if let Some(v) = meta.focal_length {
        tags.push(ExifTag::FocalLength(vec![rational(v as f64, 10)]));
    }
    if let Some(v) = meta.exposure_bias {
        tags.push(ExifTag::ExposureCompensation(vec![iR64 {
            nominator: (v as f64 * 100.0).round() as i32,
            denominator: 100,
        }]));
    }
    if let Some(v) = meta.captured_at.as_deref().and_then(exif_datetime) {
        tags.push(ExifTag::DateTimeOriginal(v));
    }
    if include_gps {
        if let (Some(lat), Some(lng)) = (meta.latitude, meta.longitude) {
            tags.push(ExifTag::GPSLatitudeRef(if lat < 0.0 { "S" } else { "N" }.to_string()));
            tags.push(ExifTag::GPSLatitude(dms(lat)));
            tags.push(ExifTag::GPSLongitudeRef(if lng < 0.0 { "W" } else { "E" }.to_string()));
            tags.push(ExifTag::GPSLongitude(dms(lng)));
            if let Some(alt) = meta.altitude {
                tags.push(ExifTag::GPSAltitudeRef(vec![u8::from(alt < 0.0)]));
                tags.push(ExifTag::GPSAltitude(vec![rational(alt.abs() as f64, 100)]));
            }
        }
    }
    if tags.is_empty() {
        return None;
    }
    let mut exif = Metadata::new();
    for tag in tags {
        exif.set_tag(tag);
    }
    Some(exif)
}

// IPTC-IIM field size limits (IPTC-NAA Information Interchange Model v4).
const CAPTION_MAX: usize = 2000;
const COPYRIGHT_MAX: usize = 128;
const CONTACT_MAX: usize = 128;
const KEYWORD_MAX: usize = 64;
// A JPEG segment holds at most 65533 payload bytes; leave room for the
// Photoshop resource-block framing.
const IIM_BUDGET: usize = 60_000;

/// Longest prefix of `s` within `max` bytes, cut on a char boundary.
fn truncate_bytes(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn push_dataset(out: &mut Vec<u8>, record: u8, dataset: u8, data: &[u8]) {
    out.push(0x1C);
    out.push(record);
    out.push(dataset);
    out.extend_from_slice(&(data.len() as u16).to_be_bytes());
    out.extend_from_slice(data);
}

/// Empty when there is nothing to write, so callers skip the segment.
fn build_iim(meta: &ExportMetadata) -> Vec<u8> {
    let non_empty = |o: &Option<String>| o.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
    let caption = non_empty(&meta.caption);
    let copyright = non_empty(&meta.copyright);
    let contact = non_empty(&meta.contact);
    let keywords: Vec<&str> = meta.keywords.iter().map(|k| k.trim()).filter(|k| !k.is_empty()).collect();
    if caption.is_none() && copyright.is_none() && contact.is_none() && keywords.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    // 1:90 CodedCharacterSet = ESC % G, i.e. UTF-8, so non-ASCII captions
    // and keywords survive.
    push_dataset(&mut out, 1, 90, &[0x1B, 0x25, 0x47]);
    // 2:0 RecordVersion = 2.
    push_dataset(&mut out, 2, 0, &[0x00, 0x02]);
    if let Some(v) = &caption {
        push_dataset(&mut out, 2, 120, truncate_bytes(v, CAPTION_MAX).as_bytes());
    }
    if let Some(v) = &copyright {
        push_dataset(&mut out, 2, 116, truncate_bytes(v, COPYRIGHT_MAX).as_bytes());
    }
    if let Some(v) = &contact {
        push_dataset(&mut out, 2, 118, truncate_bytes(v, CONTACT_MAX).as_bytes());
    }
    for keyword in keywords {
        let keyword = truncate_bytes(keyword, KEYWORD_MAX);
        if out.len() + keyword.len() + 5 > IIM_BUDGET {
            break;
        }
        push_dataset(&mut out, 2, 25, keyword.as_bytes());
    }
    out
}

/// Wraps IIM data in a Photoshop 3.0 image-resource block (resource 0x0404)
/// inside an APP13 segment and inserts it after the leading APP0/APP1
/// segments, before any table/frame segment.
fn insert_app13(jpeg: &mut Vec<u8>, iim: &[u8]) -> Result<(), MetadataWriteError> {
    let mut payload = Vec::with_capacity(iim.len() + 32);
    payload.extend_from_slice(b"Photoshop 3.0\0");
    payload.extend_from_slice(b"8BIM");
    payload.extend_from_slice(&0x0404u16.to_be_bytes());
    payload.extend_from_slice(&[0x00, 0x00]); // empty Pascal name, padded to even
    payload.extend_from_slice(&(iim.len() as u32).to_be_bytes());
    payload.extend_from_slice(iim);
    if iim.len() % 2 == 1 {
        payload.push(0x00);
    }

    let segment_len = payload.len() + 2;
    if segment_len > u16::MAX as usize {
        return Err(MetadataWriteError::IptcTooLarge);
    }
    let mut segment = vec![0xFF, 0xED];
    segment.extend_from_slice(&(segment_len as u16).to_be_bytes());
    segment.extend_from_slice(&payload);

    let mut pos = 2;
    while pos + 4 <= jpeg.len() && jpeg[pos] == 0xFF && matches!(jpeg[pos + 1], 0xE0 | 0xE1) {
        pos += 2 + u16::from_be_bytes([jpeg[pos + 2], jpeg[pos + 3]]) as usize;
    }
    let pos = pos.min(jpeg.len());
    jpeg.splice(pos..pos, segment);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank_jpeg() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbImage::from_pixel(32, 16, image::Rgb([90, 120, 60]))
            .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Jpeg)
            .unwrap();
        bytes
    }

    fn full_metadata() -> ExportMetadata {
        ExportMetadata {
            camera_make: Some("Emulsion Co".into()),
            camera_model: Some("Model 9".into()),
            lens_model: Some("50mm Test".into()),
            iso: Some(800),
            aperture: Some(2.8),
            shutter_speed: Some(1.0 / 250.0),
            focal_length: Some(50.0),
            exposure_bias: Some(-0.67),
            captured_at: Some("2024-03-15T10:30:00".into()),
            latitude: Some(-33.8568),
            longitude: Some(151.2153),
            altitude: Some(12.5),
            caption: Some("Opera house at dusk — 夕暮れ".into()),
            copyright: Some("(c) Alfred".into()),
            contact: Some("alfred@example.com".into()),
            keywords: vec!["Sydney".into(), "架橋".into(), "  ".into()],
        }
    }

    /// Minimal IIM reader for tests: (record, dataset, bytes) triples out
    /// of the APP13 segment.
    fn read_iptc(jpeg: &[u8]) -> Vec<(u8, u8, Vec<u8>)> {
        let mut pos = 2;
        while pos + 4 <= jpeg.len() && jpeg[pos] == 0xFF {
            let marker = jpeg[pos + 1];
            let len = u16::from_be_bytes([jpeg[pos + 2], jpeg[pos + 3]]) as usize;
            if marker == 0xED {
                let body = &jpeg[pos + 4..pos + 2 + len];
                let header = b"Photoshop 3.0\0";
                assert!(body.starts_with(header));
                let mut p = header.len();
                assert_eq!(&body[p..p + 4], b"8BIM");
                assert_eq!(&body[p + 4..p + 6], &[0x04, 0x04]);
                p += 8; // 8BIM + id + empty name
                let size = u32::from_be_bytes(body[p..p + 4].try_into().unwrap()) as usize;
                p += 4;
                let iim = &body[p..p + size];
                let mut out = Vec::new();
                let mut i = 0;
                while i < iim.len() {
                    assert_eq!(iim[i], 0x1C);
                    let n = u16::from_be_bytes([iim[i + 3], iim[i + 4]]) as usize;
                    out.push((iim[i + 1], iim[i + 2], iim[i + 5..i + 5 + n].to_vec()));
                    i += 5 + n;
                }
                return out;
            }
            if marker == 0xDA {
                break;
            }
            pos += 2 + len;
        }
        Vec::new()
    }

    #[test]
    fn exif_round_trips_through_the_import_side_parser_including_gps() {
        let out = apply(
            blank_jpeg(),
            &full_metadata(),
            &MetadataWriteOptions { exif: true, iptc: false, gps: true },
        )
        .unwrap();
        let back = crate::metadata::extract_from_jpeg(&out);
        assert_eq!(back.camera_make.as_deref(), Some("Emulsion Co"));
        assert_eq!(back.camera_model.as_deref(), Some("Model 9"));
        assert_eq!(back.lens_model.as_deref(), Some("50mm Test"));
        assert_eq!(back.iso, Some(800));
        assert_eq!(back.aperture, Some(2.8));
        assert_eq!(back.shutter_speed, Some(1.0 / 250.0));
        assert_eq!(back.focal_length, Some(50.0));
        assert_eq!(back.captured_at.as_deref(), Some("2024-03-15T10:30:00"));
        assert!((back.latitude.unwrap() - -33.8568).abs() < 1e-5);
        assert!((back.longitude.unwrap() - 151.2153).abs() < 1e-5);
        assert!((back.altitude.unwrap() - 12.5).abs() < 0.01);
        // EXIF-only export writes no IPTC segment.
        assert!(read_iptc(&out).is_empty());
    }

    #[test]
    fn gps_toggle_off_omits_location_but_keeps_the_rest_of_exif() {
        let out = apply(
            blank_jpeg(),
            &full_metadata(),
            &MetadataWriteOptions { exif: true, iptc: false, gps: false },
        )
        .unwrap();
        let back = crate::metadata::extract_from_jpeg(&out);
        assert_eq!(back.camera_make.as_deref(), Some("Emulsion Co"));
        assert_eq!(back.latitude, None);
        assert_eq!(back.longitude, None);
        assert_eq!(back.altitude, None);
    }

    #[test]
    fn iptc_writes_utf8_caption_copyright_contact_and_keywords() {
        let out = apply(
            blank_jpeg(),
            &full_metadata(),
            &MetadataWriteOptions { exif: false, iptc: true, gps: false },
        )
        .unwrap();
        let sets = read_iptc(&out);
        let get = |r: u8, d: u8| -> Vec<String> {
            sets.iter()
                .filter(|(rr, dd, _)| *rr == r && *dd == d)
                .map(|(_, _, v)| String::from_utf8(v.clone()).unwrap())
                .collect()
        };
        assert_eq!(sets[0], (1, 90, vec![0x1B, 0x25, 0x47]));
        assert_eq!(get(2, 120), vec!["Opera house at dusk — 夕暮れ"]);
        assert_eq!(get(2, 116), vec!["(c) Alfred"]);
        assert_eq!(get(2, 118), vec!["alfred@example.com"]);
        // Blank keyword dropped, others kept in order.
        assert_eq!(get(2, 25), vec!["Sydney", "架橋"]);
        // IPTC-only export writes no EXIF.
        let back = crate::metadata::extract_from_jpeg(&out);
        assert_eq!(back.camera_make, None);
        assert_eq!(back.latitude, None);
    }

    #[test]
    fn output_is_still_a_decodable_jpeg_with_both_groups_written() {
        let out = apply(
            blank_jpeg(),
            &full_metadata(),
            &MetadataWriteOptions { exif: true, iptc: true, gps: true },
        )
        .unwrap();
        let decoded = image::load_from_memory_with_format(&out, image::ImageFormat::Jpeg).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (32, 16));
        assert!(!read_iptc(&out).is_empty());
        assert_eq!(crate::metadata::extract_from_jpeg(&out).camera_model.as_deref(), Some("Model 9"));
    }

    #[test]
    fn nothing_requested_or_nothing_to_write_leaves_bytes_untouched() {
        let original = blank_jpeg();
        let all_off = MetadataWriteOptions::default();
        assert_eq!(apply(original.clone(), &full_metadata(), &all_off).unwrap(), original);

        let all_on = MetadataWriteOptions { exif: true, iptc: true, gps: true };
        assert_eq!(apply(original.clone(), &ExportMetadata::default(), &all_on).unwrap(), original);
    }

    #[test]
    fn iptc_fields_are_truncated_on_char_boundaries_at_their_iim_limits() {
        let meta = ExportMetadata {
            copyright: Some("é".repeat(100)), // 200 bytes, limit 128
            keywords: vec!["語".repeat(30)],  // 90 bytes, limit 64
            ..Default::default()
        };
        let out = apply(blank_jpeg(), &meta, &MetadataWriteOptions { exif: false, iptc: true, gps: false }).unwrap();
        let sets = read_iptc(&out);
        let copyright = sets.iter().find(|s| s.1 == 116).unwrap();
        assert!(copyright.2.len() <= 128);
        assert!(String::from_utf8(copyright.2.clone()).is_ok());
        let keyword = sets.iter().find(|s| s.1 == 25).unwrap();
        assert!(keyword.2.len() <= 64);
        assert!(String::from_utf8(keyword.2.clone()).is_ok());
    }

    #[test]
    fn non_jpeg_input_is_rejected_not_corrupted() {
        let err = apply(vec![1, 2, 3], &full_metadata(), &MetadataWriteOptions { exif: true, iptc: true, gps: true });
        assert!(matches!(err, Err(MetadataWriteError::NotJpeg)));
    }

    #[test]
    fn malformed_captured_at_is_skipped_rather_than_written() {
        assert_eq!(exif_datetime("2024-03-15T10:30:00"), Some("2024:03:15 10:30:00".into()));
        assert_eq!(exif_datetime("garbage"), None);
        assert_eq!(exif_datetime("2024-03-15"), None);
    }
}
