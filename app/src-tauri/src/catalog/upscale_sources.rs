//! AI enlargement in the catalog (M6 slice 4c, RFC-0029 §3.1): the enlargement is a new photo (an `images` row
//! with an empty edit stack, like an HDR or panorama result) plus one `upscale_sources` row saying where it
//! came from. This module adds that pair in one transaction, copies what the RFC says is inherited, and reads
//! the link back from both sides ("Enlarged x4 from ..." / "1 enlargement: x4"), including whether the
//! original has been edited since.

use super::*;

/// What `add_enlargement` needs to know; the file has already been written.
#[allow(dead_code)] // built by the enlarge job command of slice 4d
#[derive(Debug, Clone)]
pub struct EnlargementInput {
    /// The version (of the original) that was enlarged: its edit stack was baked into the pixels.
    pub source_version_id: i64,
    pub path: String,
    pub content_hash: String,
    pub file_size: i64,
    pub width: u32,
    pub height: u32,
    /// The effective factor (output long edge over source long edge), for the "x4" chip.
    pub scale: f64,
    pub strength: u8,
    pub model_id: String,
    /// `edit_stack_hash` of the original's stack at the time of the job.
    pub source_stack_hash: String,
    /// RFC-0029 §7.4: off by default; copies the original's (non-smart) collection memberships.
    pub add_to_collections: bool,
}

/// An enlargement as seen from the original's side.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EnlargementLink {
    pub result_image_id: i64,
    pub scale: f64,
    pub strength: u8,
    pub model_id: String,
    pub created_at: String,
    /// The original's current stack differs from the one that was enlarged. `None` when the version it was
    /// taken from no longer exists, so there is nothing to compare with.
    pub edited_since: Option<bool>,
}

/// The original as seen from an enlargement. `source_image_id` is `None` once the original left the catalog.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EnlargementOrigin {
    pub source_image_id: Option<i64>,
    pub source_version_id: Option<i64>,
    pub scale: f64,
    pub strength: u8,
    pub model_id: String,
    pub created_at: String,
    pub edited_since: Option<bool>,
}

/// A stable fingerprint of what a stack renders: blake3 of its JSON with ops ordered by name (stable, so
/// same-named ops keep their order). The renderer finds each op by name, so the array order carries no
/// meaning, and the Develop store re-appends a touched op on every change; hashing the raw array would call a
/// slider moved away and back an edit.
#[allow(dead_code)] // used by the enlarge job command of slice 4d
pub fn edit_stack_hash(stack: &EditStack) -> String {
    let mut ops = stack.ops.clone();
    ops.sort_by(|a, b| a.get("op").and_then(|v| v.as_str()).unwrap_or("").cmp(b.get("op").and_then(|v| v.as_str()).unwrap_or("")));
    let canonical = EditStack { schema_version: stack.schema_version, ops };
    blake3::hash(serde_json::to_string(&canonical).expect("EditStack is always serializable").as_bytes()).to_hex().to_string()
}

impl Catalog {
    /// Adds the enlargement as a new photo linked to its original. One transaction: the `images` row (the
    /// original's capture metadata, GPS, copyright and contact; the new size and file), an empty-stack version
    /// with the original version's caption, the keywords, optionally the collections, and the provenance row.
    /// Rating, flag and colour label are **not** inherited (they judged the original). Returns the new image id.
    #[allow(dead_code)] // called by the enlarge job command of slice 4d
    pub fn add_enlargement(&self, input: &EnlargementInput) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        let source_image_id: i64 = tx.query_row("SELECT image_id FROM image_versions WHERE id = ?1", params![input.source_version_id], |r| r.get(0))?;
        tx.execute(
            "INSERT INTO images (
                path, content_hash, file_size, width, height,
                camera_make, camera_model, lens_model, iso, aperture, shutter_speed, focal_length,
                exposure_bias, metering_mode, flash, latitude, longitude, altitude, captured_at,
                copyright, contact
            ) SELECT ?1, ?2, ?3, ?4, ?5,
                camera_make, camera_model, lens_model, iso, aperture, shutter_speed, focal_length,
                exposure_bias, metering_mode, flash, latitude, longitude, altitude, captured_at,
                copyright, contact
              FROM images WHERE id = ?6",
            params![input.path, input.content_hash, input.file_size, input.width, input.height, source_image_id],
        )?;
        let result_id = tx.last_insert_rowid();

        let empty = serde_json::to_string(&EditStack::empty()).expect("EditStack is always serializable");
        tx.execute(
            "INSERT INTO image_versions (image_id, edit_stack_json, caption)
             SELECT ?1, ?2, caption FROM image_versions WHERE id = ?3",
            params![result_id, empty, input.source_version_id],
        )?;
        tx.execute(
            "INSERT INTO image_keywords (image_id, keyword_id) SELECT ?1, keyword_id FROM image_keywords WHERE image_id = ?2",
            params![result_id, source_image_id],
        )?;
        if input.add_to_collections {
            tx.execute(
                "INSERT OR IGNORE INTO collection_images (collection_id, image_id)
                 SELECT ci.collection_id, ?1 FROM collection_images ci
                 JOIN collections c ON c.id = ci.collection_id
                 WHERE ci.image_id = ?2 AND c.is_smart = 0",
                params![result_id, source_image_id],
            )?;
        }
        tx.execute(
            "INSERT INTO upscale_sources (result_image_id, source_image_id, source_version_id, scale, strength, model_id, source_stack_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![result_id, source_image_id, input.source_version_id, input.scale, input.strength, input.model_id, input.source_stack_hash],
        )?;
        tx.commit()?;
        Ok(result_id)
    }

    /// Whether the stack now stored for `version_id` differs from the one hashed as `stored_hash`; `None` when
    /// the version is gone (or was never recorded).
    fn edited_since(&self, version_id: Option<i64>, stored_hash: &str) -> Result<Option<bool>> {
        let Some(version_id) = version_id else { return Ok(None) };
        let json: Option<String> = self
            .conn
            .query_row("SELECT edit_stack_json FROM image_versions WHERE id = ?1", params![version_id], |r| r.get(0))
            .optional()?;
        Ok(json.and_then(|j| serde_json::from_str::<EditStack>(&j).ok()).map(|stack| edit_stack_hash(&stack) != stored_hash))
    }

    /// Where this photo came from, if it is an enlargement.
    pub fn get_enlargement_origin(&self, result_image_id: i64) -> Result<Option<EnlargementOrigin>> {
        let row = self
            .conn
            .query_row(
                "SELECT source_image_id, source_version_id, scale, strength, model_id, created_at, source_stack_hash
                 FROM upscale_sources WHERE result_image_id = ?1",
                params![result_image_id],
                |r| Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get::<_, String>(6)?)),
            )
            .optional()?;
        let Some((source_image_id, source_version_id, scale, strength, model_id, created_at, hash)) = row else { return Ok(None) };
        let edited_since = self.edited_since(source_version_id, &hash)?;
        Ok(Some(EnlargementOrigin { source_image_id, source_version_id, scale, strength, model_id, created_at, edited_since }))
    }

    /// Every enlargement made from this photo, newest first.
    pub fn get_enlargements_of(&self, source_image_id: i64) -> Result<Vec<EnlargementLink>> {
        let mut stmt = self.conn.prepare(
            "SELECT result_image_id, source_version_id, scale, strength, model_id, created_at, source_stack_hash
             FROM upscale_sources WHERE source_image_id = ?1 ORDER BY created_at DESC, result_image_id DESC",
        )?;
        let rows = stmt
            .query_map(params![source_image_id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get::<_, String>(6)?))
            })?
            .collect::<Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(result_image_id, version, scale, strength, model_id, created_at, hash)| {
                Ok(EnlargementLink { result_image_id, scale, strength, model_id, created_at, edited_since: self.edited_since(version, &hash)? })
            })
            .collect()
    }

    /// Whether this photo is a model enlargement (RFC-0029 §7.6: Enlarge is disabled on one, in the first cut).
    #[allow(dead_code)] // used by the Enlarge menu entry of slice 4e
    pub fn is_enlargement(&self, image_id: i64) -> Result<bool> {
        self.conn.query_row("SELECT EXISTS(SELECT 1 FROM upscale_sources WHERE result_image_id = ?1)", params![image_id], |r| r.get(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn meta() -> ImageMetadata {
        ImageMetadata {
            camera_make: Some("Canon".into()),
            camera_model: Some("EOS R5".into()),
            lens_model: Some("RF 50mm".into()),
            iso: Some(400),
            aperture: Some(1.8),
            focal_length: Some(50.0),
            width: Some(6000),
            height: Some(4000),
            latitude: Some(1.29),
            longitude: Some(103.85),
            captured_at: Some("2026-05-01T10:00:00".into()),
            ..Default::default()
        }
    }

    /// An original with a rating, a flag, a label, a caption, copyright, two keywords and two collections.
    fn original(catalog: &Catalog) -> (i64, i64) {
        let stack = EditStack { schema_version: 1, ops: vec![json!({"op": "exposure", "value": 0.4})] };
        let image = catalog.add_image_with_edit_stack("/orig.jpg", "hash-orig", 5000, &stack, &meta()).unwrap();
        let version = catalog.list_images().unwrap().iter().find(|i| i.image_id == image).unwrap().version_id;
        catalog.conn.execute("UPDATE images SET copyright = 'C Alfred', contact = 'a@b.c' WHERE id = ?1", params![image]).unwrap();
        catalog.conn.execute("UPDATE image_versions SET caption = 'Bridge', rating = 4, flag = 'pick', color_label = 'red' WHERE id = ?1", params![version]).unwrap();
        catalog.assign_keyword_path(&[image], &["Places".into(), "Singapore".into()]).unwrap();
        let manual = catalog.create_collection("Trip").unwrap();
        catalog.add_images_to_collection(manual, &[image]).unwrap();
        catalog.conn.execute("INSERT INTO collections (name, is_smart, rules_json) VALUES ('Smart', 1, '{}')", []).unwrap();
        (image, version)
    }

    fn input(version: i64, stack: &EditStack, collections: bool) -> EnlargementInput {
        EnlargementInput {
            source_version_id: version,
            path: "/orig-Enlarged-4x.jpg".into(),
            content_hash: "hash-big".into(),
            file_size: 90_000,
            width: 24_000,
            height: 16_000,
            scale: 4.0,
            strength: 45,
            model_id: "realesr_general_x4v3".into(),
            source_stack_hash: edit_stack_hash(stack),
            add_to_collections: collections,
        }
    }

    #[test]
    fn the_grid_listing_carries_the_chip_and_the_link_glyph_data() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (orig, version) = original(&catalog);
        let stack = catalog.get_edit_stack(version).unwrap();
        let big = catalog.add_enlargement(&input(version, &stack, false)).unwrap();
        let mut again = input(version, &stack, false);
        again.path = "/orig-Enlarged-4x-2.jpg".into();
        again.content_hash = "hash-big-2".into();
        again.scale = 2.5;
        let second = catalog.add_enlargement(&again).unwrap();
        let rows = catalog.list_images().unwrap();
        let row = |id: i64| rows.iter().find(|i| i.image_id == id).unwrap();
        assert_eq!((row(orig).enlarged_scale, row(orig).enlargement_count), (None, 2), "the original: no chip, two enlargements");
        assert_eq!((row(big).enlarged_scale, row(big).enlargement_count, row(big).enlarged_from), (Some(4.0), 0, Some(orig)));
        assert_eq!(row(orig).enlarged_from, None);
        assert_eq!((row(second).enlarged_scale, row(second).enlargement_count), (Some(2.5), 0));
        catalog.remove_images(&[orig]).unwrap();
        let rows = catalog.list_images().unwrap();
        let after = rows.iter().find(|i| i.image_id == big).unwrap();
        assert_eq!((after.enlarged_scale, after.enlarged_from), (Some(4.0), None), "the chip survives the original's removal; the link reads 'original removed'");
    }

    #[test]
    fn the_hash_ignores_op_order_but_not_values() {
        let a = EditStack { schema_version: 1, ops: vec![json!({"op": "exposure", "value": 1}), json!({"op": "contrast", "value": 2})] };
        let b = EditStack { schema_version: 1, ops: vec![json!({"op": "contrast", "value": 2}), json!({"op": "exposure", "value": 1})] };
        let c = EditStack { schema_version: 1, ops: vec![json!({"op": "exposure", "value": 1}), json!({"op": "contrast", "value": 3})] };
        assert_eq!(edit_stack_hash(&a), edit_stack_hash(&b));
        assert_ne!(edit_stack_hash(&a), edit_stack_hash(&c));
        assert_ne!(edit_stack_hash(&a), edit_stack_hash(&EditStack::empty()));
    }

    #[test]
    fn an_enlargement_is_a_new_photo_with_an_empty_stack_that_inherits_what_the_rfc_says() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (orig, version) = original(&catalog);
        let stack = catalog.get_edit_stack(version).unwrap();
        let big = catalog.add_enlargement(&input(version, &stack, false)).unwrap();
        assert_ne!(big, orig);

        let summary = catalog.list_images().unwrap().into_iter().find(|i| i.image_id == big).expect("visible in the grid");
        assert_eq!((summary.width, summary.height), (Some(24_000), Some(16_000)));
        assert_eq!(summary.camera_model.as_deref(), Some("EOS R5"));
        assert_eq!(catalog.get_edit_stack(summary.version_id).unwrap(), EditStack::empty(), "the original's edits are baked in");
        // Not inherited: rating, flag, colour label.
        assert_eq!((summary.rating, summary.flag.as_str(), summary.color_label.as_str()), (0, "none", "none"));
        // Inherited: caption, copyright, contact, GPS, date, keywords.
        let (copyright, contact, lat, captured): (Option<String>, Option<String>, Option<f64>, Option<String>) =
            catalog.conn.query_row("SELECT copyright, contact, latitude, captured_at FROM images WHERE id = ?1", params![big], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap();
        assert_eq!((copyright.as_deref(), contact.as_deref(), lat, captured.as_deref()), (Some("C Alfred"), Some("a@b.c"), Some(1.29), Some("2026-05-01T10:00:00")));
        let caption: Option<String> = catalog.conn.query_row("SELECT caption FROM image_versions WHERE id = ?1", params![summary.version_id], |r| r.get(0)).unwrap();
        assert_eq!(caption.as_deref(), Some("Bridge"));
        let kw = |id: i64| -> i64 { catalog.conn.query_row("SELECT count(*) FROM image_keywords WHERE image_id = ?1", params![id], |r| r.get(0)).unwrap() };
        assert_eq!(kw(big), kw(orig));
        assert!(kw(big) > 0);
        // The original is untouched.
        let o = catalog.list_images().unwrap().into_iter().find(|i| i.image_id == orig).unwrap();
        assert_eq!((o.rating, o.flag.as_str()), (4, "pick"));
    }

    #[test]
    fn collections_are_copied_only_when_asked_and_never_the_smart_ones() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (_, version) = original(&catalog);
        let stack = catalog.get_edit_stack(version).unwrap();
        let count = |id: i64| -> i64 { catalog.conn.query_row("SELECT count(*) FROM collection_images WHERE image_id = ?1", params![id], |r| r.get(0)).unwrap() };
        let off = catalog.add_enlargement(&input(version, &stack, false)).unwrap();
        assert_eq!(count(off), 0, "off by default");
        let mut second = input(version, &stack, true);
        second.path = "/orig-Enlarged-4x-2.jpg".into();
        second.content_hash = "hash-big-2".into();
        let on = catalog.add_enlargement(&second).unwrap();
        assert_eq!(count(on), 1, "the manual collection only");
    }

    #[test]
    fn links_read_back_from_both_sides_and_say_when_the_original_was_edited_since() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (orig, version) = original(&catalog);
        let stack = catalog.get_edit_stack(version).unwrap();
        let big = catalog.add_enlargement(&input(version, &stack, false)).unwrap();

        let from = catalog.get_enlargement_origin(big).unwrap().expect("an enlargement has an origin");
        assert_eq!((from.source_image_id, from.source_version_id, from.scale, from.strength), (Some(orig), Some(version), 4.0, 45));
        assert_eq!(from.edited_since, Some(false));
        let made = catalog.get_enlargements_of(orig).unwrap();
        assert_eq!(made.len(), 1);
        assert_eq!((made[0].result_image_id, made[0].edited_since), (big, Some(false)));
        assert!(catalog.get_enlargement_origin(orig).unwrap().is_none(), "an ordinary photo has no origin");
        assert!(catalog.is_enlargement(big).unwrap() && !catalog.is_enlargement(orig).unwrap());

        // Edit the original: both sides say so.
        let edited = EditStack { schema_version: 1, ops: vec![json!({"op": "exposure", "value": 0.9})] };
        catalog.conn.execute("UPDATE image_versions SET edit_stack_json = ?2 WHERE id = ?1", params![version, serde_json::to_string(&edited).unwrap()]).unwrap();
        assert_eq!(catalog.get_enlargement_origin(big).unwrap().unwrap().edited_since, Some(true));
        assert_eq!(catalog.get_enlargements_of(orig).unwrap()[0].edited_since, Some(true));
        // Moving the slider away and back is not an edit.
        let back = EditStack { schema_version: 1, ops: vec![json!({"op": "contrast", "value": 0}), json!({"op": "exposure", "value": 0.4})] };
        let before = EditStack { schema_version: 1, ops: vec![json!({"op": "exposure", "value": 0.4}), json!({"op": "contrast", "value": 0})] };
        assert_eq!(edit_stack_hash(&back), edit_stack_hash(&before));
    }

    #[test]
    fn enlarging_again_makes_another_connected_photo_and_never_touches_the_first() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (orig, version) = original(&catalog);
        let stack = catalog.get_edit_stack(version).unwrap();
        let first = catalog.add_enlargement(&input(version, &stack, false)).unwrap();
        let mut again = input(version, &stack, false);
        again.path = "/orig-Enlarged-4x-2.jpg".into();
        again.content_hash = "hash-big-2".into();
        let second = catalog.add_enlargement(&again).unwrap();
        let made: Vec<i64> = catalog.get_enlargements_of(orig).unwrap().iter().map(|l| l.result_image_id).collect();
        assert_eq!(made.len(), 2);
        assert!(made.contains(&first) && made.contains(&second));
    }

    #[test]
    fn removing_the_original_keeps_the_enlargement_and_blanks_the_link_and_removing_the_result_drops_the_row() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (orig, version) = original(&catalog);
        let stack = catalog.get_edit_stack(version).unwrap();
        let big = catalog.add_enlargement(&input(version, &stack, false)).unwrap();
        let mut again = input(version, &stack, false);
        again.path = "/orig-Enlarged-4x-2.jpg".into();
        again.content_hash = "hash-big-2".into();
        let other = catalog.add_enlargement(&again).unwrap();

        catalog.remove_images(&[orig]).unwrap();
        assert!(catalog.list_images().unwrap().iter().any(|i| i.image_id == big), "the enlargement is its own photo");
        let origin = catalog.get_enlargement_origin(big).unwrap().expect("the provenance row stays");
        assert_eq!((origin.source_image_id, origin.source_version_id, origin.edited_since), (None, None, None), "'original removed'");

        // A new photo that happens to reuse the removed id's slot must not be taken for the original.
        let newcomer = catalog.add_image_with_edit_stack("/new.jpg", "hash-new", 1, &EditStack::empty(), &ImageMetadata::default()).unwrap();
        assert!(catalog.get_enlargements_of(newcomer).unwrap().is_empty());

        catalog.remove_images(&[big]).unwrap();
        assert!(catalog.get_enlargement_origin(big).unwrap().is_none());
        assert!(catalog.get_enlargement_origin(other).unwrap().is_some(), "the other enlargement is unaffected");
    }

    #[test]
    fn a_missing_source_version_is_an_error_and_adds_nothing() {
        let catalog = Catalog::open_in_memory().unwrap();
        let before = catalog.list_images().unwrap().len();
        assert!(catalog.add_enlargement(&input(9999, &EditStack::empty(), false)).is_err());
        assert_eq!(catalog.list_images().unwrap().len(), before);
    }

    #[test]
    fn a_path_that_already_exists_rolls_everything_back() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (orig, version) = original(&catalog);
        let stack = catalog.get_edit_stack(version).unwrap();
        let mut clash = input(version, &stack, true);
        clash.path = "/orig.jpg".into(); // UNIQUE(path)
        assert!(catalog.add_enlargement(&clash).is_err());
        assert!(catalog.get_enlargements_of(orig).unwrap().is_empty());
        assert_eq!(catalog.list_images().unwrap().len(), 1);
    }
}
