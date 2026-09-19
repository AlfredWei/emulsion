use super::*;

/// One row for the Library grid: an image plus its primary (non-virtual-copy)
/// version's culling state. Virtual-copy-aware listing is M2+ scope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageSummary {
    pub image_id: i64,
    pub version_id: i64,
    pub path: String,
    pub thumbnail_path: Option<String>,
    pub rating: u8,
    pub flag: String,
    pub color_label: String,
    pub added_at: String,
    /// Which `import_paths` call brought this image in (M4 Library slice) --
    /// shared by every image imported in the same folder/files operation,
    /// so the frontend's "Last Import" source can filter by
    /// `import_batch == max(import_batch)` without a separate batch table.
    /// `None` for rows inserted before this column existed.
    pub import_batch: Option<i64>,
    /// Nullable in Rust to match the nullable `images.content_hash`
    /// column, even though `add_image_with_metadata` always sets it for
    /// real imports. Lets `preview_cache::pregenerate_missing` key the
    /// Develop preview cache without re-reading+re-hashing every file.
    pub content_hash: Option<String>,
    /// EXIF, read-only, captured at import time (M2 Slice 2) -- never
    /// user-edited, so no setter exists for these.
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub iso: Option<u32>,
    pub aperture: Option<f32>,
    pub shutter_speed: Option<f32>,
    pub focal_length: Option<f32>,
    pub exposure_bias: Option<f32>,
    pub metering_mode: Option<String>,
    pub flash: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude: Option<f32>,
    pub file_size: Option<i64>,
    pub captured_at: Option<String>,
    /// IPTC (M2 Slice 2), user-editable via `set_caption`/`set_copyright`/
    /// `set_contact`. Deliberately split across tables, not uniform:
    /// `caption` lives on `image_versions` (plausibly diverges per virtual
    /// copy, matching the character of `rating`/`flag`/`color_label`
    /// there); `copyright`/`contact` live on `images` (identify the
    /// photographer/owner -- invariant across every copy of a given
    /// original, closer in spirit to EXIF than to per-copy culling state).
    /// A deliberate asymmetry, not an oversight.
    pub caption: Option<String>,
    pub copyright: Option<String>,
    pub contact: Option<String>,
    /// Whether face detection has ever been attempted for this image --
    /// see `faces_scanned`'s own schema comment. Lets the frontend decide
    /// "does this photo/folder still need on-demand detection" without a
    /// separate query.
    pub faces_scanned: bool,
}

/// What `remove_images` hands back per deleted row, so the command layer
/// can clean up the app-owned derived files (thumbnail JPEG, content-hash-
/// keyed Develop preview PNG) after the transaction commits. Never
/// includes the source path -- removal must not even be *handed* the means
/// to touch an original.
pub struct RemovedImage {
    pub id: i64,
    pub content_hash: Option<String>,
}

/// Internal to the Rust command layer only (HDR merge, M5) -- never
/// returned to the frontend. See `get_image_exposure_info`.
#[derive(Debug, Clone)]
pub struct ImageExposureInfo {
    pub path: String,
    pub iso: Option<u32>,
    pub aperture: Option<f32>,
    pub shutter_speed: Option<f32>,
}

impl Catalog {
    /// Test-only convenience over `add_image_with_metadata`: every real
    /// import always has a content hash and file size available (see
    /// import.rs), so this metadata-less variant has no production caller.
    #[cfg(test)]
    pub fn add_image(&self, path: &str) -> Result<i64> {
        self.conn
            .execute("INSERT INTO images (path) VALUES (?1)", params![path])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Lower-level building block, superseded as the import pipeline's
    /// caller by the atomic `add_image_with_edit_stack` (M1 Slice 6) --
    /// kept as real pub API since catalog.rs's own tests use it directly,
    /// and it's the natural insert-only-the-image half for a future M2
    /// virtual-copy path (a new edit-stack row against an *existing*
    /// image_id, which `add_image_with_edit_stack` doesn't support).
    #[allow(dead_code)]
    pub fn add_image_with_metadata(
        &self,
        path: &str,
        content_hash: &str,
        file_size: i64,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO images (path, content_hash, file_size) VALUES (?1, ?2, ?3)",
            params![path, content_hash, file_size],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Atomically inserts an image row (with its EXIF metadata, M2 Slice 2)
    /// and its initial edit-stack row (M1 Slice 6). The import pipeline
    /// used to do this as two separate auto-commit statements
    /// (`add_image_with_metadata` then `add_edit_stack`) -- a crash between
    /// them left an `images` row with no matching `image_versions` row.
    /// `list_images()`'s join silently *excludes* such a row rather than
    /// erroring, but `find_by_hash` still matches it on every future
    /// import scan, so the file became permanently unimportable through
    /// the normal UI. This is the real import-time caller;
    /// `add_image_with_metadata`/`add_edit_stack` stay available as
    /// lower-level building blocks (tests, and future M2 virtual copies,
    /// which need a new edit-stack row against an *existing* image_id).
    /// EXIF metadata insertion is part of the same transaction for the
    /// same reason the two rows are: a crash partway through import
    /// shouldn't be able to leave metadata inconsistently applied.
    ///
    /// Uses `unchecked_transaction` (rusqlite's `&self`-based transaction
    /// API) rather than `transaction` (`&mut self`) since `Catalog`'s
    /// methods are all `&self`, called through `Arc<Mutex<Catalog>>` --
    /// the Mutex already serializes access, so there's no real nested-
    /// transaction risk to guard against here.
    pub fn add_image_with_edit_stack(
        &self,
        path: &str,
        content_hash: &str,
        file_size: i64,
        stack: &EditStack,
        metadata: &ImageMetadata,
    ) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO images (
                path, content_hash, file_size,
                camera_make, camera_model, lens_model,
                iso, aperture, shutter_speed, focal_length,
                exposure_bias, metering_mode, flash,
                width, height, latitude, longitude, altitude,
                captured_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            params![
                path,
                content_hash,
                file_size,
                metadata.camera_make,
                metadata.camera_model,
                metadata.lens_model,
                metadata.iso,
                metadata.aperture,
                metadata.shutter_speed,
                metadata.focal_length,
                metadata.exposure_bias,
                metadata.metering_mode,
                metadata.flash,
                metadata.width,
                metadata.height,
                metadata.latitude,
                metadata.longitude,
                metadata.altitude,
                metadata.captured_at,
            ],
        )?;
        let image_id = tx.last_insert_rowid();

        let json = serde_json::to_string(stack).expect("EditStack is always serializable");
        tx.execute(
            "INSERT INTO image_versions (image_id, edit_stack_json) VALUES (?1, ?2)",
            params![image_id, json],
        )?;

        tx.commit()?;
        Ok(image_id)
    }

    /// Look up an existing image by content hash, for duplicate detection.
    pub fn find_by_hash(&self, content_hash: &str) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM images WHERE content_hash = ?1",
                params![content_hash],
                |row| row.get(0),
            )
            .optional()
    }

    /// One image's path + exposure fields, resolved by id -- the catalog-
    /// side half of building an `hdr_merge::BracketInput` (RFC-0003 §3.6);
    /// `hdr_merge.rs` itself has no catalog dependency (see that module's
    /// own header comment), so the Tauri command layer (`lib.rs`) is what
    /// bridges the two, calling this once per selected image id.
    pub fn get_image_exposure_info(&self, image_id: i64) -> Result<Option<ImageExposureInfo>> {
        self.conn
            .query_row(
                "SELECT path, iso, aperture, shutter_speed FROM images WHERE id = ?1",
                params![image_id],
                |row| {
                    Ok(ImageExposureInfo {
                        path: row.get(0)?,
                        iso: row.get(1)?,
                        aperture: row.get(2)?,
                        shutter_speed: row.get(3)?,
                    })
                },
            )
            .optional()
    }

    /// One image's path, resolved by id -- the catalog-side half of
    /// building `panorama_merge::stitch`'s own `&[PathBuf]` input (RFC-
    /// 0004 §3.6). Deliberately narrower than `get_image_exposure_info`:
    /// panorama stitching needs no EXIF, unlike HDR merge's EV, so
    /// reusing that method here would be a layering smell (this caller
    /// asking for fields it never looks at).
    pub fn get_image_path(&self, image_id: i64) -> Result<Option<String>> {
        self.conn
            .query_row("SELECT path FROM images WHERE id = ?1", params![image_id], |row| row.get(0))
            .optional()
    }

    /// Non-destructive removal (M2 Slice 3): deletes catalog rows only --
    /// the user's source file is NEVER touched (hard PRD constraint), and
    /// the app-owned derived files (thumbnail, cached Develop preview) are
    /// the *caller's* cleanup concern, which is why each removed image's
    /// `id`/`content_hash` is returned: file-on-disk concerns live in the
    /// command layer, not here, matching how thumbnail writes already sit
    /// outside catalog.rs. The command layer derives every thumbnail/
    /// preview filename an image can have from just these two fields
    /// (`{id}.jpg`/`{id}-*` for thumbnails, `{content_hash}*` for
    /// previews) rather than needing the exact current thumbnail_path.
    ///
    /// One transaction for the whole batch, child rows deleted explicitly
    /// before parents -- deliberately NOT relying on `ON DELETE CASCADE`
    /// even though `enable_foreign_keys` now makes it live (see that
    /// method's comment). Deleting the `images` row is also what makes the
    /// file re-importable afterward: `find_by_hash`'s dedupe check stops
    /// matching -- that's the user-facing meaning of "non-destructive"
    /// (remove, then change your mind and import again).
    ///
    /// Unknown ids are a no-op, not an error: a double-fired removal (or a
    /// removal racing a refresh) should converge on "row is gone", not
    /// fail halfway through a batch.
    pub fn remove_images(&self, image_ids: &[i64]) -> Result<Vec<RemovedImage>> {
        let tx = self.conn.unchecked_transaction()?;
        let mut removed = Vec::with_capacity(image_ids.len());

        for &image_id in image_ids {
            let row: Option<RemovedImage> = tx
                .query_row(
                    "SELECT content_hash FROM images WHERE id = ?1",
                    params![image_id],
                    |row| {
                        Ok(RemovedImage {
                            id: image_id,
                            content_hash: row.get(0)?,
                        })
                    },
                )
                .optional()?;
            let Some(row) = row else { continue };

            tx.execute(
                "DELETE FROM image_versions WHERE image_id = ?1",
                params![image_id],
            )?;
            // M2 Slice 4/5: same "explicit, don't rely on CASCADE"
            // discipline as the image_versions delete above.
            tx.execute(
                "DELETE FROM image_keywords WHERE image_id = ?1",
                params![image_id],
            )?;
            tx.execute(
                "DELETE FROM collection_images WHERE image_id = ?1",
                params![image_id],
            )?;
            // M5 (HDR merge): same explicit discipline -- both directions,
            // since a removed image might be a merge's *result* (drop its
            // whole provenance row set) or one of its *sources* (drop just
            // that one row; the result and its other sources are unaffected).
            tx.execute(
                "DELETE FROM hdr_merge_sources WHERE result_image_id = ?1 OR source_image_id = ?1",
                params![image_id],
            )?;
            // M5 (panorama merge): same both-directions discipline as
            // hdr_merge_sources just above.
            tx.execute(
                "DELETE FROM panorama_merge_sources WHERE result_image_id = ?1 OR source_image_id = ?1",
                params![image_id],
            )?;
            // M5 Slice 6 (face detection, RFC-0005 §3.4): one-directional,
            // same explicit discipline as image_versions/image_keywords/
            // collection_images above -- a face row always belongs to
            // exactly one image, never a merge-style result/source pair.
            tx.execute("DELETE FROM faces WHERE image_id = ?1", params![image_id])?;
            tx.execute("DELETE FROM images WHERE id = ?1", params![image_id])?;
            removed.push(row);
        }

        tx.commit()?;
        Ok(removed)
    }

    pub fn set_thumbnail_path(&self, image_id: i64, thumbnail_path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE images SET thumbnail_path = ?2 WHERE id = ?1",
            params![image_id, thumbnail_path],
        )?;
        Ok(())
    }

    /// Used by the on-demand "jump the queue" thumbnail path
    /// (`import::ensure_thumbnail`) to check, under the lock, whether the
    /// background backfill pass has already generated this image's
    /// thumbnail by the time the request is served -- an indexed
    /// single-row lookup by primary key, not `list_images()`'s full-table
    /// join, since this is a per-click hot path.
    pub fn get_thumbnail_path(&self, image_id: i64) -> Result<Option<String>> {
        self.conn
            .query_row("SELECT thumbnail_path FROM images WHERE id = ?1", params![image_id], |row| row.get(0))
    }

    /// Tags a just-inserted image with the batch id `import_paths` computed
    /// once for the whole import call -- a separate step from
    /// `add_image_with_edit_stack`'s own insert transaction (rather than a
    /// new parameter threaded through it and its ~20 existing test call
    /// sites) since this is bookkeeping for the "Last Import" library source,
    /// not data the atomic image+version+metadata insert needs to guard.
    pub fn set_import_batch(&self, image_id: i64, import_batch: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE images SET import_batch = ?2 WHERE id = ?1",
            params![image_id, import_batch],
        )?;
        Ok(())
    }

    /// Library grid data: one row per image, its primary (first,
    /// non-virtual-copy) version's culling state. Newest imports first.
    pub fn list_images(&self) -> Result<Vec<ImageSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT i.id, v.id, i.path, i.thumbnail_path, v.rating, v.flag, v.color_label, i.added_at, i.content_hash,
                    i.camera_make, i.camera_model, i.lens_model, i.iso, i.aperture, i.shutter_speed, i.focal_length,
                    i.exposure_bias, i.metering_mode, i.flash, i.width, i.height, i.latitude, i.longitude, i.altitude,
                    i.file_size, i.captured_at,
                    v.caption, i.copyright, i.contact, i.import_batch, i.faces_scanned
             FROM images i
             JOIN image_versions v ON v.id = (
                 SELECT id FROM image_versions
                 WHERE image_id = i.id AND is_virtual_copy = 0
                 ORDER BY id ASC LIMIT 1
             )
             ORDER BY i.added_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ImageSummary {
                image_id: row.get(0)?,
                version_id: row.get(1)?,
                path: row.get(2)?,
                thumbnail_path: row.get(3)?,
                rating: row.get(4)?,
                flag: row.get(5)?,
                color_label: row.get(6)?,
                added_at: row.get(7)?,
                content_hash: row.get(8)?,
                camera_make: row.get(9)?,
                camera_model: row.get(10)?,
                lens_model: row.get(11)?,
                iso: row.get(12)?,
                aperture: row.get(13)?,
                shutter_speed: row.get(14)?,
                focal_length: row.get(15)?,
                exposure_bias: row.get(16)?,
                metering_mode: row.get(17)?,
                flash: row.get(18)?,
                width: row.get(19)?,
                height: row.get(20)?,
                latitude: row.get(21)?,
                longitude: row.get(22)?,
                altitude: row.get(23)?,
                file_size: row.get(24)?,
                captured_at: row.get(25)?,
                caption: row.get(26)?,
                copyright: row.get(27)?,
                contact: row.get(28)?,
                import_batch: row.get(29)?,
                faces_scanned: row.get(30)?,
            })
        })?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use crate::catalog::test_support::*;

    /// M0 exit criterion (MILESTONES.md): "Catalog schema v0 ... can store
    /// an image reference + one edit record."
    #[test]
    fn round_trips_one_image_and_one_edit_record() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");

        let image_id = catalog
            .add_image("/Users/alfred/Pictures/2026-Kyoto/IMG_0001.CR3")
            .expect("image insert succeeds");

        let stack = EditStack {
            schema_version: 1,
            ops: vec![json!({"op": "exposure", "value": 0.5})],
        };
        let version_id = catalog
            .add_edit_stack(image_id, &stack)
            .expect("edit stack insert succeeds");

        let round_tripped = catalog
            .get_edit_stack(version_id)
            .expect("edit stack read succeeds");

        assert_eq!(round_tripped, stack);
    }

    #[test]
    fn rejects_duplicate_image_paths() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        catalog.add_image("/a.CR3").expect("first insert succeeds");
        assert!(catalog.add_image("/a.CR3").is_err());
    }

    #[test]
    fn finds_images_by_content_hash_for_dedupe() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        assert_eq!(catalog.find_by_hash("abc123").unwrap(), None);

        let image_id = catalog
            .add_image_with_metadata("/a.CR3", "abc123", 4096)
            .expect("insert with metadata succeeds");

        assert_eq!(catalog.find_by_hash("abc123").unwrap(), Some(image_id));
        assert_eq!(catalog.find_by_hash("does-not-exist").unwrap(), None);
    }

    #[test]
    fn sets_thumbnail_rating_flag_and_color_label() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let image_id = catalog.add_image("/a.CR3").unwrap();
        let version_id = catalog.add_edit_stack(image_id, &EditStack::empty()).unwrap();

        catalog.set_thumbnail_path(image_id, "/thumbs/a.jpg").unwrap();
        catalog.set_rating(version_id, 4).unwrap();
        catalog.set_flag(version_id, "pick").unwrap();
        catalog.set_color_label(version_id, "green").unwrap();

        let images = catalog.list_images().unwrap();
        assert_eq!(images.len(), 1);
        let summary = &images[0];
        assert_eq!(summary.thumbnail_path.as_deref(), Some("/thumbs/a.jpg"));
        assert_eq!(summary.rating, 4);
        assert_eq!(summary.flag, "pick");
        assert_eq!(summary.color_label, "green");
    }

    #[test]
    fn get_image_exposure_info_resolves_path_and_exif_fields() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let metadata = crate::metadata::ImageMetadata {
            iso: Some(400),
            aperture: Some(5.6),
            shutter_speed: Some(1.0 / 250.0),
            ..Default::default()
        };
        let image_id = catalog
            .add_image_with_edit_stack("/bracket-a.CR3", "hash-a", 100, &EditStack::empty(), &metadata)
            .unwrap();

        let info = catalog.get_image_exposure_info(image_id).unwrap().expect("row exists");
        assert_eq!(info.path, "/bracket-a.CR3");
        assert_eq!(info.iso, Some(400));
        assert_eq!(info.aperture, Some(5.6));
        assert_eq!(info.shutter_speed, Some(1.0 / 250.0));

        assert!(catalog.get_image_exposure_info(image_id + 999).unwrap().is_none());
    }

    #[test]
    fn get_image_path_resolves_by_id_and_is_none_for_an_unknown_id() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let image_id = catalog.add_image("/a.jpg").unwrap();

        assert_eq!(catalog.get_image_path(image_id).unwrap(), Some("/a.jpg".to_string()));
        assert!(catalog.get_image_path(image_id + 999).unwrap().is_none());
    }

    #[test]
    fn add_image_with_edit_stack_inserts_both_rows_atomically() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let stack = EditStack {
            schema_version: 1,
            ops: vec![json!({"op": "exposure", "value": 0.3})],
        };

        let image_id = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-a", 4096, &stack, &crate::metadata::ImageMetadata::default())
            .expect("atomic insert succeeds");

        assert_eq!(catalog.find_by_hash("hash-a").unwrap(), Some(image_id));

        let images = catalog.list_images().unwrap();
        assert_eq!(images.len(), 1, "the image row must be visible via list_images (i.e. a matching image_versions row exists)");
        assert_eq!(catalog.get_edit_stack(images[0].version_id).unwrap(), stack);
    }

    #[test]
    fn add_image_with_edit_stack_persists_metadata_atomically() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let metadata = crate::metadata::ImageMetadata {
            camera_make: Some("Canon".to_string()),
            camera_model: Some("EOS 5D Mark III".to_string()),
            lens_model: Some("24-70mm f/2.8".to_string()),
            iso: Some(200),
            aperture: Some(2.8),
            shutter_speed: Some(1.0 / 100.0),
            focal_length: Some(70.0),
            captured_at: Some("2017-01-05T05:53:29+00:00".to_string()),
            ..Default::default()
        };

        catalog
            .add_image_with_edit_stack("/a.CR3", "hash-meta", 4096, &EditStack::empty(), &metadata)
            .expect("insert succeeds");

        let images = catalog.list_images().unwrap();
        assert_eq!(images.len(), 1);
        let summary = &images[0];
        assert_eq!(summary.camera_make, metadata.camera_make);
        assert_eq!(summary.camera_model, metadata.camera_model);
        assert_eq!(summary.lens_model, metadata.lens_model);
        assert_eq!(summary.iso, metadata.iso);
        assert_eq!(summary.aperture, metadata.aperture);
        assert_eq!(summary.shutter_speed, metadata.shutter_speed);
        assert_eq!(summary.focal_length, metadata.focal_length);
        assert_eq!(summary.captured_at, metadata.captured_at);
    }

    /// The user-facing meaning of "non-destructive removal": both rows are
    /// gone atomically, and -- the part a user would actually notice --
    /// the same file becomes importable again because `find_by_hash`'s
    /// dedupe check no longer matches.
    #[test]
    fn remove_images_deletes_both_rows_and_makes_the_file_reimportable() {
        let catalog = Catalog::open_in_memory().unwrap();
        let keep_id = catalog
            .add_image_with_edit_stack("/keep.CR3", "hash-keep", 100, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        let remove_id = catalog
            .add_image_with_edit_stack("/remove.CR3", "hash-remove", 200, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();

        let removed = catalog.remove_images(&[remove_id]).unwrap();

        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].id, remove_id);
        assert_eq!(removed[0].content_hash.as_deref(), Some("hash-remove"));

        let images = catalog.list_images().unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].image_id, keep_id);

        // No orphaned child row left behind (the inert-CASCADE trap this
        // slice's explicit child-delete exists to avoid).
        let orphans: i64 = catalog
            .conn
            .query_row(
                "SELECT count(*) FROM image_versions WHERE image_id = ?1",
                params![remove_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(orphans, 0);

        assert_eq!(catalog.find_by_hash("hash-remove").unwrap(), None, "removed file must be re-importable");
        assert_eq!(catalog.find_by_hash("hash-keep").unwrap(), Some(keep_id));
    }

    #[test]
    fn remove_images_ignores_unknown_ids_and_handles_batches() {
        let catalog = Catalog::open_in_memory().unwrap();
        let a = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-a", 1, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        let b = catalog
            .add_image_with_edit_stack("/b.CR3", "hash-b", 2, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();

        let removed = catalog.remove_images(&[a, 9999, b]).unwrap();

        assert_eq!(removed.len(), 2, "unknown id is a silent no-op, not an error");
        assert!(catalog.list_images().unwrap().is_empty());
    }

    #[test]
    fn list_images_orders_newest_first_and_skips_virtual_copies() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");

        let image_a = catalog.add_image("/a.CR3").unwrap();
        catalog.add_edit_stack(image_a, &EditStack::empty()).unwrap();
        let image_b = catalog.add_image("/b.CR3").unwrap();
        catalog.add_edit_stack(image_b, &EditStack::empty()).unwrap();

        let images = catalog.list_images().unwrap();
        assert_eq!(images.len(), 2);
        // both inserted in the same instant in tests, so just check both
        // images are represented exactly once each, not duplicated.
        let paths: Vec<&str> = images.iter().map(|s| s.path.as_str()).collect();
        assert!(paths.contains(&"/a.CR3"));
        assert!(paths.contains(&"/b.CR3"));
    }

    /// M5 Slice 3 (GPU performance validation): MILESTONES.md's M5 exit
    /// criterion ("...on a 50k-image catalog...") had never been measured
    /// against the SQLite persistence path -- the interactive Develop
    /// render loop itself is architecturally decoupled from catalog size
    /// (ADR-0004: it never touches the catalog), so the real risk to that
    /// criterion is here, not in the shader. `#[ignore]`d (not part of the
    /// default fast suite) since seeding 50k rows takes real wall time;
    /// run explicitly with `cargo test --release -- --ignored --nocapture
    /// catalog_scales_to_50k_images`.
    ///
    /// Seeds a real 50k-image, 50k-version in-memory catalog (one initial
    /// edit-stack version per image, matching real import's own shape --
    /// see `add_image_with_edit_stack`), then times the three operations
    /// the Develop/Library UI actually calls against a live catalog:
    /// `list_images` (Library grid's own full-catalog query),
    /// `record_edit_stack` (every debounced slider-settle flush, always
    /// against ONE version by primary key regardless of catalog size), and
    /// `get_edit_stack` (Develop's own open-image read).
    ///
    /// This DID find a real bug on its first run, before
    /// `idx_image_versions_image_id` existed: `list_images`'s per-row
    /// correlated subquery (`SELECT id FROM image_versions WHERE image_id
    /// = i.id ...`) had no index to satisfy that filter, so SQLite fell
    /// back to a full table scan of `image_versions` for every one of the
    /// 50,000 outer rows -- confirmed via `EXPLAIN QUERY PLAN` (run
    /// separately against a minimal reproduction of this schema, not
    /// inline in this test) showing `SCAN image_versions` inside the
    /// subquery before the index, `SEARCH ... USING INDEX
    /// idx_image_versions_image_id` after. Directly measured at 37.4s
    /// (release build) for one `list_images()` call over this test's real
    /// 50,000-row catalog -- ~75x this test's own 500ms budget for that
    /// operation, and a real regression against PRD §9's own catalog-
    /// open-time target for a 50k-image catalog. Adding the index
    /// (present in `migrate()` as of this slice) dropped it to ~75ms --
    /// re-confirmed by temporarily reverting the index and re-running,
    /// which reproduced the 37.4s result again.
    /// `record_edit_stack`/`get_edit_stack` were already fast before the
    /// fix (both filter `image_versions` by its own indexed `id` primary
    /// key, never by `image_id`) -- included here as a permanent
    /// regression guard for all three, not because the other two were
    /// ever actually at risk.
    #[test]
    #[ignore]
    fn catalog_scales_to_50k_images() {
        use std::time::Instant;

        const N: i64 = 50_000;
        // Generous budgets, not tight ones -- this test's job is to catch
        // a real O(n) or worse regression (like the missing-index bug it
        // already found once), not to enforce the PRD's interactive
        // ≤100ms figure to the millisecond against an in-memory SQLite
        // connection with no real disk I/O.
        const LIST_IMAGES_BUDGET_MS: u128 = 500;
        const SINGLE_ROW_OP_BUDGET_MS: u128 = 50;

        let catalog = Catalog::open_in_memory().unwrap();
        for i in 0..N {
            catalog
                .add_image_with_edit_stack(
                    &format!("/synthetic/img_{i:06}.CR3"),
                    &format!("hash_{i:06}"),
                    20_000_000,
                    &EditStack::empty(),
                    &ImageMetadata::default(),
                )
                .unwrap();
        }

        let started = Instant::now();
        let images = catalog.list_images().unwrap();
        let list_images_ms = started.elapsed().as_millis();
        assert_eq!(images.len(), N as usize);
        eprintln!("list_images() over {N} images: {list_images_ms}ms");
        assert!(
            list_images_ms < LIST_IMAGES_BUDGET_MS,
            "list_images() took {list_images_ms}ms over {N} images, budget is {LIST_IMAGES_BUDGET_MS}ms -- likely a missing index on image_versions(image_id) or images(...) that a query now needs"
        );

        // The last-inserted row -- the worst case for any query that (if
        // it were ever mis-written to) scanned from the front, and the
        // one a real user would actually be editing right after a large
        // import.
        let target_version_id = images.last().unwrap().version_id;

        let started = Instant::now();
        catalog
            .record_edit_stack(target_version_id, &stack_with("exposure", 1.5), Some("Exposure"))
            .unwrap();
        let record_ms = started.elapsed().as_millis();
        eprintln!("record_edit_stack() against a 50k-row catalog: {record_ms}ms");
        assert!(
            record_ms < SINGLE_ROW_OP_BUDGET_MS,
            "record_edit_stack() took {record_ms}ms over {N} images, budget is {SINGLE_ROW_OP_BUDGET_MS}ms"
        );

        let started = Instant::now();
        let stack = catalog.get_edit_stack(target_version_id).unwrap();
        let get_ms = started.elapsed().as_millis();
        eprintln!("get_edit_stack() against a 50k-row catalog: {get_ms}ms");
        assert_eq!(stack, stack_with("exposure", 1.5));
        assert!(
            get_ms < SINGLE_ROW_OP_BUDGET_MS,
            "get_edit_stack() took {get_ms}ms over {N} images, budget is {SINGLE_ROW_OP_BUDGET_MS}ms"
        );
    }
}
