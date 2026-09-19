use super::*;

/// Internal to the Rust command layer only (thumbnail regeneration) --
/// never returned to the frontend, so no Serialize/Deserialize.
#[derive(Debug, Clone)]
pub struct VersionSource {
    pub image_id: i64,
    pub path: String,
    pub content_hash: Option<String>,
}

impl Catalog {
    /// Resolves a version_id to what thumbnail regeneration needs: the
    /// parent image's id (thumbnails are keyed by image_id, not
    /// version_id), source path, and content_hash (to reuse the Develop
    /// preview cache instead of a fresh decode). A real JOIN -- unlike
    /// `get_edit_stack` above, which is JOIN-free only because
    /// `edit_stack_json` happens to live directly on `image_versions`.
    pub fn get_version_source(&self, version_id: i64) -> Result<VersionSource> {
        self.conn.query_row(
            "SELECT i.id, i.path, i.content_hash FROM image_versions v
             JOIN images i ON i.id = v.image_id
             WHERE v.id = ?1",
            params![version_id],
            |row| {
                Ok(VersionSource {
                    image_id: row.get(0)?,
                    path: row.get(1)?,
                    content_hash: row.get(2)?,
                })
            },
        )
    }

    pub fn set_rating(&self, version_id: i64, rating: u8) -> Result<()> {
        self.conn.execute(
            "UPDATE image_versions SET rating = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![version_id, rating],
        )?;
        Ok(())
    }

    pub fn set_flag(&self, version_id: i64, flag: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE image_versions SET flag = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![version_id, flag],
        )?;
        Ok(())
    }

    pub fn set_color_label(&self, version_id: i64, color_label: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE image_versions SET color_label = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![version_id, color_label],
        )?;
        Ok(())
    }

    /// IPTC (M2 Slice 2). `caption` is per-version (see `ImageSummary`'s
    /// doc comment for why it's split from copyright/contact below).
    pub fn set_caption(&self, version_id: i64, caption: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE image_versions SET caption = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![version_id, caption],
        )?;
        Ok(())
    }

    pub fn set_copyright(&self, image_id: i64, copyright: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE images SET copyright = ?2 WHERE id = ?1",
            params![image_id, copyright],
        )?;
        Ok(())
    }

    pub fn set_contact(&self, image_id: i64, contact: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE images SET contact = ?2 WHERE id = ?1",
            params![image_id, contact],
        )?;
        Ok(())
    }

    /// Everything `metadata_writer` can embed for one version: the image's
    /// EXIF-derived columns plus the IPTC fields and keywords. Keyed by
    /// version (not image) because `caption` is per-version. `None` if the
    /// version doesn't exist.
    pub fn get_export_metadata(&self, version_id: i64) -> Result<Option<crate::metadata_writer::ExportMetadata>> {
        let row = self
            .conn
            .query_row(
                "SELECT i.id, i.camera_make, i.camera_model, i.lens_model, i.iso, i.aperture,
                        i.shutter_speed, i.focal_length, i.exposure_bias, i.captured_at,
                        i.latitude, i.longitude, i.altitude, v.caption, i.copyright, i.contact
                 FROM image_versions v JOIN images i ON i.id = v.image_id
                 WHERE v.id = ?1",
                params![version_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        crate::metadata_writer::ExportMetadata {
                            camera_make: row.get(1)?,
                            camera_model: row.get(2)?,
                            lens_model: row.get(3)?,
                            iso: row.get(4)?,
                            aperture: row.get(5)?,
                            shutter_speed: row.get(6)?,
                            focal_length: row.get(7)?,
                            exposure_bias: row.get(8)?,
                            captured_at: row.get(9)?,
                            latitude: row.get(10)?,
                            longitude: row.get(11)?,
                            altitude: row.get(12)?,
                            caption: row.get(13)?,
                            copyright: row.get(14)?,
                            contact: row.get(15)?,
                            keywords: Vec::new(),
                        },
                    ))
                },
            )
            .optional()?;
        let Some((image_id, mut metadata)) = row else {
            return Ok(None);
        };
        metadata.keywords = self.get_image_keywords(image_id)?.into_iter().map(|k| k.name).collect();
        Ok(Some(metadata))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_version_source_resolves_the_parent_images_row() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let image_id = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-a", 100, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        let images = catalog.list_images().unwrap();
        let version_id = images[0].version_id;

        let source = catalog.get_version_source(version_id).unwrap();

        assert_eq!(source.image_id, image_id);
        assert_eq!(source.path, "/a.CR3");
        assert_eq!(source.content_hash.as_deref(), Some("hash-a"));
    }

    #[test]
    fn rejects_out_of_range_rating_and_invalid_flag() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        assert!(catalog.set_rating(version_id, 6).is_err());
        assert!(catalog.set_flag(version_id, "not-a-real-flag").is_err());
        assert!(catalog.set_color_label(version_id, "not-a-real-color").is_err());
    }

    #[test]
    fn set_caption_copyright_and_contact_round_trip() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let image_id = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-iptc", 4096, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        let images = catalog.list_images().unwrap();
        let version_id = images[0].version_id;

        catalog.set_caption(version_id, "A quiet morning in Kyoto").unwrap();
        catalog.set_copyright(image_id, "© 2026 Alfred Wei").unwrap();
        catalog.set_contact(image_id, "alfred@example.com").unwrap();

        let images = catalog.list_images().unwrap();
        assert_eq!(images[0].caption.as_deref(), Some("A quiet morning in Kyoto"));
        assert_eq!(images[0].copyright.as_deref(), Some("© 2026 Alfred Wei"));
        assert_eq!(images[0].contact.as_deref(), Some("alfred@example.com"));
    }

    #[test]
    fn get_export_metadata_joins_exif_iptc_and_keywords_for_a_version() {
        let catalog = Catalog::open_in_memory().unwrap();
        let metadata = crate::metadata::ImageMetadata {
            camera_make: Some("Make".into()),
            iso: Some(200),
            latitude: Some(1.5),
            longitude: Some(2.5),
            ..Default::default()
        };
        let image_id = catalog.add_image_with_edit_stack("/a.CR3", "hash-exp", 10, &EditStack::empty(), &metadata).unwrap();
        let version_id = catalog.list_images().unwrap()[0].version_id;
        catalog.set_caption(version_id, "cap").unwrap();
        catalog.set_copyright(image_id, "(c)").unwrap();
        catalog.assign_keyword_path(&[image_id], &["nature".to_string(), "owl".to_string()]).unwrap();

        let got = catalog.get_export_metadata(version_id).unwrap().unwrap();
        assert_eq!(got.camera_make.as_deref(), Some("Make"));
        assert_eq!(got.iso, Some(200));
        assert_eq!((got.latitude, got.longitude), (Some(1.5), Some(2.5)));
        assert_eq!(got.caption.as_deref(), Some("cap"));
        assert_eq!(got.copyright.as_deref(), Some("(c)"));
        assert_eq!(got.keywords, vec!["owl".to_string()]);
        assert!(catalog.get_export_metadata(9999).unwrap().is_none());
    }
}
