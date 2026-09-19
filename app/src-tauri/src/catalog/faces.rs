use super::*;

/// One detected face (RFC-0005 §3.4/§3.6), with its person's name already
/// joined in. `person_id`/`person_name` are both `None` for a detection
/// that hasn't been clustered/named yet -- the frontend's "Who is this?"
/// state. Bbox fields are normalized `[0, 1]` fractions of the image, same
/// convention as the `faces` table itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaceRow {
    pub id: i64,
    pub image_id: i64,
    pub person_id: Option<i64>,
    pub person_name: Option<String>,
    pub bbox_x: f32,
    pub bbox_y: f32,
    pub bbox_w: f32,
    pub bbox_h: f32,
}

/// One row of the People grid (RFC-0005 §3.6): `name` is `None` until the
/// user names it (shown as "Person N" in the UI, same as the reviewed
/// mockup), `cover_face_id` is whichever face seeded this person's
/// cluster, `photo_count` counts distinct images, not faces (the same
/// person appearing twice in one photo still counts once).
///
/// `cover_image_path`/`cover_bbox_*` resolve the cover face's own photo
/// and crop in the same query (a `LEFT JOIN` through `faces`/`images`, not
/// a second round trip) -- there is no other command that maps a bare
/// `cover_face_id` back to an image path, so the grid's avatar crop would
/// otherwise be unreachable from the frontend. `None` only if the cover
/// face (or its image) has since been removed out from under a stale
/// `cover_face_id` -- the grid falls back to a placeholder avatar then.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonRow {
    pub id: i64,
    pub name: Option<String>,
    pub cover_face_id: Option<i64>,
    pub photo_count: i64,
    pub cover_image_path: Option<String>,
    pub cover_bbox_x: Option<f32>,
    pub cover_bbox_y: Option<f32>,
    pub cover_bbox_w: Option<f32>,
    pub cover_bbox_h: Option<f32>,
}

impl Catalog {
    /// Inserts one detected face (RFC-0005 §3.4), `person_id` NULL and
    /// `excluded` false until clustering or the user's own correction
    /// says otherwise. `embedding` is stored as raw native-endian `f32`
    /// bytes via `bytemuck` -- every platform this app targets (x86_64,
    /// arm64) is little-endian, so this matches the schema's own
    /// "little-endian" comment without a manual byte-swap.
    pub fn add_face(&self, image_id: i64, bbox: (f32, f32, f32, f32), embedding: &[f32]) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO faces (image_id, bbox_x, bbox_y, bbox_w, bbox_h, embedding) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![image_id, bbox.0, bbox.1, bbox.2, bbox.3, bytemuck::cast_slice::<f32, u8>(embedding)],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn set_face_person(&self, face_id: i64, person_id: Option<i64>) -> Result<()> {
        self.conn.execute("UPDATE faces SET person_id = ?1 WHERE id = ?2", params![person_id, face_id])?;
        Ok(())
    }

    /// The "not a face" correction (RFC-0005 §3.6): also clears any
    /// existing `person_id` -- an excluded row must never keep
    /// contributing to a person's centroid or stay visible as one of
    /// their photos.
    pub fn set_face_excluded(&self, face_id: i64, excluded: bool) -> Result<()> {
        self.conn.execute("UPDATE faces SET excluded = ?1, person_id = NULL WHERE id = ?2", params![excluded as i64, face_id])?;
        Ok(())
    }

    /// Marks an image as having had face detection attempted, whether or
    /// not it found any faces -- see `faces_scanned`'s own schema comment.
    /// Called once per image at the end of `face_pipeline::detect_faces_for_batch`'s
    /// per-image loop iteration, including on a decode failure/missing
    /// file (same "don't retry forever on a permanently bad file" framing
    /// as import's own skip-not-fatal handling).
    pub fn mark_faces_scanned(&self, image_id: i64) -> Result<()> {
        self.conn.execute("UPDATE images SET faces_scanned = 1 WHERE id = ?1", params![image_id])?;
        Ok(())
    }

    /// Every non-excluded face on one image, with its person's name
    /// (`NULL` if unclustered) already joined in -- what the People
    /// module's per-photo tagging view (RFC-0005 §3.6) reads directly,
    /// no separate `list_people` round-trip needed to label each box.
    pub fn get_faces_for_image(&self, image_id: i64) -> Result<Vec<FaceRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.image_id, f.person_id, p.name, f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h
             FROM faces f LEFT JOIN people p ON p.id = f.person_id
             WHERE f.image_id = ?1 AND f.excluded = 0
             ORDER BY f.id",
        )?;
        let rows = stmt
            .query_map(params![image_id], |row| {
                Ok(FaceRow {
                    id: row.get(0)?,
                    image_id: row.get(1)?,
                    person_id: row.get(2)?,
                    person_name: row.get(3)?,
                    bbox_x: row.get(4)?,
                    bbox_y: row.get(5)?,
                    bbox_w: row.get(6)?,
                    bbox_h: row.get(7)?,
                })
            })?
            .collect();
        rows
    }

    /// Every non-excluded face's id, current `person_id` (`None` if not
    /// yet clustered), and decoded embedding -- the raw material
    /// `face_pipeline.rs` needs to rebuild cluster centroids (grouping by
    /// `person_id`) before feeding new faces through
    /// `face_cluster::assign_face`, or to recompute clustering from
    /// scratch for the explicit "Find People" action.
    pub fn get_clusterable_faces(&self) -> Result<Vec<(i64, Option<i64>, Vec<f32>)>> {
        let mut stmt = self.conn.prepare("SELECT id, person_id, embedding FROM faces WHERE excluded = 0 ORDER BY id")?;
        let rows = stmt
            .query_map([], |row| {
                let id: i64 = row.get(0)?;
                let person_id: Option<i64> = row.get(1)?;
                let blob: Vec<u8> = row.get(2)?;
                Ok((id, person_id, bytemuck::cast_slice::<u8, f32>(&blob).to_vec()))
            })?
            .collect();
        rows
    }

    /// Every distinct image with at least one non-excluded face assigned
    /// to this person -- backs the Library rail's "double-click a person
    /// to filter" action, the same `Vec<image_id>` -> `Set` membership
    /// shape `manualMembership` already uses for a manual collection
    /// (frontend caches this per person id rather than re-querying on
    /// every filter switch).
    pub fn get_image_ids_for_person(&self, person_id: i64) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT image_id FROM faces WHERE person_id = ?1 AND excluded = 0")?;
        let rows = stmt.query_map(params![person_id], |row| row.get(0))?.collect();
        rows
    }

    /// Creates a new person for a brand-new cluster -- `cover_face_id` is
    /// the face that seeded it, shown as that person's grid thumbnail
    /// until the user picks a different one (not yet exposed as its own
    /// action -- RFC-0005 §3.6 only names basic correction).
    pub fn create_person(&self, cover_face_id: i64) -> Result<i64> {
        self.conn.execute("INSERT INTO people (cover_face_id) VALUES (?1)", params![cover_face_id])?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn rename_person(&self, person_id: i64, name: Option<&str>) -> Result<()> {
        self.conn.execute("UPDATE people SET name = ?1 WHERE id = ?2", params![name, person_id])?;
        Ok(())
    }

    /// One row per person with at least one non-excluded face, cover
    /// crop + name-or-`None` (the frontend shows "Person N" for `None`,
    /// same as the reviewed mockup) + photo count, most photos first.
    /// A person that ends up with zero faces (every one of them
    /// reassigned/excluded away) is a real but expected state -- left in
    /// the table rather than auto-deleted, since a stray empty person
    /// with a name the user already typed shouldn't silently vanish; it
    /// just sorts last and shows a 0 count.
    pub fn list_people(&self) -> Result<Vec<PersonRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.name, p.cover_face_id, COUNT(DISTINCT f.image_id),
                    ci.path, cf.bbox_x, cf.bbox_y, cf.bbox_w, cf.bbox_h
             FROM people p
             LEFT JOIN faces f ON f.person_id = p.id AND f.excluded = 0
             LEFT JOIN faces cf ON cf.id = p.cover_face_id
             LEFT JOIN images ci ON ci.id = cf.image_id
             GROUP BY p.id
             ORDER BY COUNT(DISTINCT f.image_id) DESC, p.id ASC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(PersonRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    cover_face_id: row.get(2)?,
                    photo_count: row.get(3)?,
                    cover_image_path: row.get(4)?,
                    cover_bbox_x: row.get(5)?,
                    cover_bbox_y: row.get(6)?,
                    cover_bbox_w: row.get(7)?,
                    cover_bbox_h: row.get(8)?,
                })
            })?
            .collect();
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// M5 Slice 6 (People view): the grid's avatar crop needs the cover
    /// face's own photo path + bbox, not just its bare id -- there is no
    /// other command that resolves `cover_face_id` back to an image, so
    /// `list_people` must do it itself via the `faces`/`images` join.
    #[test]
    fn list_people_resolves_the_cover_faces_photo_and_bbox() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let image_id = catalog.add_image("/a.jpg").unwrap();
        let face_id = catalog.add_face(image_id, (0.1, 0.2, 0.3, 0.4), &[1.0, 0.0]).unwrap();
        let person_id = catalog.create_person(face_id).unwrap();
        catalog.set_face_person(face_id, Some(person_id)).unwrap();

        let people = catalog.list_people().unwrap();
        assert_eq!(people.len(), 1);
        let person = &people[0];
        assert_eq!(person.cover_face_id, Some(face_id));
        assert_eq!(person.cover_image_path.as_deref(), Some("/a.jpg"));
        assert_eq!(person.cover_bbox_x, Some(0.1));
        assert_eq!(person.cover_bbox_y, Some(0.2));
        assert_eq!(person.cover_bbox_w, Some(0.3));
        assert_eq!(person.cover_bbox_h, Some(0.4));
    }

    /// Library rail's "double-click a person to filter" action: only
    /// images with a face actually ASSIGNED to this person should come
    /// back -- an unclustered or excluded face on some other image must
    /// never leak in, and a person with two faces on the same image must
    /// not return that image id twice (the `DISTINCT` earns its keep).
    #[test]
    fn get_image_ids_for_person_returns_only_that_persons_assigned_non_excluded_faces() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let image_a = catalog.add_image("/a.jpg").unwrap();
        let image_b = catalog.add_image("/b.jpg").unwrap();
        let image_c = catalog.add_image("/c.jpg").unwrap();

        // Two faces of the same person on image_a -- must not double-count.
        let face_a1 = catalog.add_face(image_a, (0.0, 0.0, 0.1, 0.1), &[1.0, 0.0]).unwrap();
        let face_a2 = catalog.add_face(image_a, (0.2, 0.2, 0.1, 0.1), &[1.0, 0.0]).unwrap();
        let person = catalog.create_person(face_a1).unwrap();
        catalog.set_face_person(face_a1, Some(person)).unwrap();
        catalog.set_face_person(face_a2, Some(person)).unwrap();

        // A second person's face on image_b -- must not leak in.
        let face_b = catalog.add_face(image_b, (0.0, 0.0, 0.1, 0.1), &[0.0, 1.0]).unwrap();
        let other_person = catalog.create_person(face_b).unwrap();
        catalog.set_face_person(face_b, Some(other_person)).unwrap();

        // This person's face on image_c, but excluded -- must not count.
        let face_c = catalog.add_face(image_c, (0.0, 0.0, 0.1, 0.1), &[1.0, 0.0]).unwrap();
        catalog.set_face_person(face_c, Some(person)).unwrap();
        catalog.set_face_excluded(face_c, true).unwrap();

        let ids = catalog.get_image_ids_for_person(person).unwrap();
        assert_eq!(ids, vec![image_a]);
    }

    /// People-tab UX fix (2026-09-16): `faces_scanned` distinguishes
    /// "detection never attempted" from "attempted, found nothing" --
    /// without it, on-demand detection (Tag Faces auto-detect, folder-
    /// scoped "Find People") would either redo work on every call or
    /// never know an image still needs its first scan.
    #[test]
    fn a_freshly_imported_image_defaults_to_faces_not_yet_scanned() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        catalog
            .add_image_with_edit_stack("/a.CR3", "hash-faces-1", 4096, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();

        let images = catalog.list_images().unwrap();
        assert_eq!(images.len(), 1);
        assert!(!images[0].faces_scanned);
    }

    #[test]
    fn mark_faces_scanned_flips_the_flag_for_exactly_that_image() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let scanned_id = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-faces-2", 4096, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        catalog
            .add_image_with_edit_stack("/b.CR3", "hash-faces-3", 4096, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();

        catalog.mark_faces_scanned(scanned_id).unwrap();

        let images = catalog.list_images().unwrap();
        let scanned = images.iter().find(|i| i.image_id == scanned_id).unwrap();
        let unscanned = images.iter().find(|i| i.image_id != scanned_id).unwrap();
        assert!(scanned.faces_scanned);
        assert!(!unscanned.faces_scanned);
    }

    // Face detection / People view schema (RFC-0005 §3.4). No accessor
    // functions exist yet -- these tests exercise the raw schema directly,
    // same as the ALTER-TABLE migration test above does, since the
    // detection/embedding pipeline that will actually populate `faces` is
    // still blocked on RFC-0005 §5's open model/license question.
    #[test]
    fn faces_and_people_tables_exist_with_expected_cascade_behavior() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let image_id = catalog.add_image("/a.jpg").unwrap();
        catalog
            .conn
            .execute("INSERT INTO people (id, name) VALUES (1, 'Alex')", [])
            .unwrap();
        catalog
            .conn
            .execute(
                "INSERT INTO faces (image_id, person_id, bbox_x, bbox_y, bbox_w, bbox_h, embedding)
                 VALUES (?1, 1, 0.1, 0.2, 0.3, 0.4, ?2)",
                params![image_id, vec![0u8, 1, 2, 3]],
            )
            .unwrap();

        // Deleting the person clears the face's person_id but keeps the
        // detection row -- ON DELETE SET NULL, not a cascade delete.
        catalog.conn.execute("DELETE FROM people WHERE id = 1", []).unwrap();
        let person_id: Option<i64> = catalog
            .conn
            .query_row("SELECT person_id FROM faces WHERE image_id = ?1", params![image_id], |r| r.get(0))
            .unwrap();
        assert_eq!(person_id, None, "removing a person must not delete the underlying face detection");

        // Deleting the image cascades to its faces rows -- same shape as
        // hdr_merge_sources/panorama_merge_sources' own image-delete cascade.
        catalog.remove_images(&[image_id]).unwrap();
        let remaining: i64 = catalog
            .conn
            .query_row("SELECT count(*) FROM faces WHERE image_id = ?1", params![image_id], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 0, "removing an image must drop its face detections");
    }
}
