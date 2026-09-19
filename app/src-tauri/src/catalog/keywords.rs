use super::*;

/// A keyword assigned to a specific image (M2 Slice 4). `path` is the
/// joined ancestor chain (e.g. "nature / birds / owl") -- built once here
/// so the frontend doesn't need its own copy of the tree just to show a
/// chip tooltip for one image's keywords.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeywordRef {
    pub id: i64,
    pub name: String,
    pub path: String,
}

/// One node of the full keyword tree, flat (M2 Slice 4). The frontend
/// walks `parent_id` links client-side to build full paths for
/// autocomplete suggestions -- no recursive SQL, matching this file's
/// existing no-CTE style.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeywordNode {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
}

/// One image-keyword assignment (M2 Slice 5) -- the flat shape
/// `list_all_image_keywords` returns to back Smart Collections' keyword
/// rules client-side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageKeywordAssignment {
    pub image_id: i64,
    pub keyword_id: i64,
}

impl Catalog {
    /// Resolves a hierarchical keyword path (e.g. `["nature","birds","owl"]`)
    /// to a leaf keyword id, creating any level that doesn't exist yet, and
    /// assigns that leaf to every image in `image_ids`. One transaction for
    /// the whole operation (matching `add_image_with_edit_stack`'s pattern).
    ///
    /// Find-or-create per level rather than a `UNIQUE(parent_id, name)`
    /// constraint (see the schema comment in `migrate()` for why that
    /// constraint wouldn't actually work) -- safe under this app's
    /// concurrency model since `AppState.catalog` is one
    /// `Arc<Mutex<Catalog>>`, so no other writer can interleave between a
    /// level's SELECT and its INSERT within this transaction.
    ///
    /// `INSERT OR IGNORE` against the `image_keywords` composite primary
    /// key makes assignment idempotent -- re-assigning an already-assigned
    /// keyword to an image is a silent no-op, not an error, matching
    /// `remove_images`'s "unknown/duplicate converges quietly" discipline.
    pub fn assign_keyword_path(&self, image_ids: &[i64], path_segments: &[String]) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;

        let mut parent_id: Option<i64> = None;
        for segment in path_segments {
            let existing: Option<i64> = tx
                .query_row(
                    "SELECT id FROM keywords WHERE name = ?1 AND parent_id IS ?2",
                    params![segment, parent_id],
                    |row| row.get(0),
                )
                .optional()?;
            parent_id = Some(match existing {
                Some(id) => id,
                None => {
                    tx.execute(
                        "INSERT INTO keywords (name, parent_id) VALUES (?1, ?2)",
                        params![segment, parent_id],
                    )?;
                    tx.last_insert_rowid()
                }
            });
        }
        let leaf_id = parent_id.expect("path_segments must be non-empty");

        for &image_id in image_ids {
            tx.execute(
                "INSERT OR IGNORE INTO image_keywords (image_id, keyword_id) VALUES (?1, ?2)",
                params![image_id, leaf_id],
            )?;
        }

        tx.commit()?;
        Ok(leaf_id)
    }

    /// Anchor-only removal (M2 Slice 4) -- matches the IPTC caption/
    /// copyright/contact precedent: multi-select display/edit stays
    /// scoped to the anchor image, only *assignment* batches across a
    /// selection.
    pub fn remove_keyword_from_image(&self, image_id: i64, keyword_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM image_keywords WHERE image_id = ?1 AND keyword_id = ?2",
            params![image_id, keyword_id],
        )?;
        Ok(())
    }

    /// The keywords assigned to one image, each with its full ancestor
    /// path built via a simple parent-walk loop -- bounded by (keywords on
    /// this image) x (tree depth), not by catalog or keyword-tree size, so
    /// this is cheap even called once per image selection.
    pub fn get_image_keywords(&self, image_id: i64) -> Result<Vec<KeywordRef>> {
        let mut stmt = self.conn.prepare(
            "SELECT k.id, k.name FROM image_keywords ik
             JOIN keywords k ON k.id = ik.keyword_id
             WHERE ik.image_id = ?1
             ORDER BY k.name",
        )?;
        let leaves: Vec<(i64, String)> = stmt
            .query_map(params![image_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_>>()?;

        leaves
            .into_iter()
            .map(|(id, name)| {
                let path = self.keyword_path(id)?;
                Ok(KeywordRef { id, name, path })
            })
            .collect()
    }

    /// Walks `parent_id` links up from a keyword to build its full
    /// "grandparent / parent / name" display path.
    fn keyword_path(&self, keyword_id: i64) -> Result<String> {
        let mut segments = Vec::new();
        let mut current = Some(keyword_id);
        while let Some(id) = current {
            let (name, parent_id): (String, Option<i64>) = self.conn.query_row(
                "SELECT name, parent_id FROM keywords WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            segments.push(name);
            current = parent_id;
        }
        segments.reverse();
        Ok(segments.join(" / "))
    }

    /// The full keyword tree, flat -- the frontend walks `parent_id` links
    /// client-side to build display paths for autocomplete suggestions.
    pub fn list_keywords(&self) -> Result<Vec<KeywordNode>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, parent_id FROM keywords ORDER BY name")?;
        let rows = stmt.query_map([], |row| {
            Ok(KeywordNode {
                id: row.get(0)?,
                name: row.get(1)?,
                parent_id: row.get(2)?,
            })
        })?;
        rows.collect()
    }

    /// Every image-keyword assignment in the catalog, flat -- backs
    /// Smart Collections' "has keyword" / "untagged" rules (M2 Slice 5).
    /// One query, no joins, independent of `list_images()`/`ImageSummary`
    /// (which deliberately carries no keyword data -- see that struct's
    /// doc comment); fetched once by the frontend, not per image.
    pub fn list_all_image_keywords(&self) -> Result<Vec<ImageKeywordAssignment>> {
        let mut stmt = self
            .conn
            .prepare("SELECT image_id, keyword_id FROM image_keywords")?;
        let rows = stmt.query_map([], |row| {
            Ok(ImageKeywordAssignment {
                image_id: row.get(0)?,
                keyword_id: row.get(1)?,
            })
        })?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::test_support::*;

    #[test]
    fn assign_keyword_path_creates_nested_hierarchy_and_reuses_existing_segments() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-a", 1, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();

        let path = ["nature".to_string(), "birds".to_string(), "owl".to_string()];
        let leaf_id = catalog.assign_keyword_path(&[image_id], &path).unwrap();

        let keywords = catalog.get_image_keywords(image_id).unwrap();
        assert_eq!(keywords.len(), 1);
        assert_eq!(keywords[0].id, leaf_id);
        assert_eq!(keywords[0].name, "owl");
        assert_eq!(keywords[0].path, "nature / birds / owl");

        let all = catalog.list_keywords().unwrap();
        assert_eq!(all.len(), 3, "one keyword row created per path segment");

        // Re-assigning the same path (e.g. a second image tagged similarly)
        // must reuse the existing levels, not create duplicates.
        let image_b = catalog
            .add_image_with_edit_stack("/b.CR3", "hash-b", 2, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        let leaf_id_again = catalog.assign_keyword_path(&[image_b], &path).unwrap();
        assert_eq!(leaf_id_again, leaf_id, "the same path must resolve to the same leaf id");
        assert_eq!(catalog.list_keywords().unwrap().len(), 3, "no duplicate keyword rows created");
    }

    #[test]
    fn assign_keyword_path_batches_across_multiple_images_and_is_idempotent() {
        let catalog = Catalog::open_in_memory().unwrap();
        let a = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-a", 1, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        let b = catalog
            .add_image_with_edit_stack("/b.CR3", "hash-b", 2, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();

        let path = ["kyoto".to_string()];
        catalog.assign_keyword_path(&[a, b], &path).unwrap();
        // Re-assigning to the same set must be a silent no-op, not an error.
        catalog.assign_keyword_path(&[a, b], &path).unwrap();

        assert_eq!(catalog.get_image_keywords(a).unwrap().len(), 1);
        assert_eq!(catalog.get_image_keywords(b).unwrap().len(), 1);
    }

    #[test]
    fn remove_keyword_from_image_is_anchor_only() {
        let catalog = Catalog::open_in_memory().unwrap();
        let a = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-a", 1, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        let b = catalog
            .add_image_with_edit_stack("/b.CR3", "hash-b", 2, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        let leaf_id = catalog.assign_keyword_path(&[a, b], &["kyoto".to_string()]).unwrap();

        catalog.remove_keyword_from_image(a, leaf_id).unwrap();

        assert!(catalog.get_image_keywords(a).unwrap().is_empty());
        assert_eq!(catalog.get_image_keywords(b).unwrap().len(), 1, "removal must not affect other images");
    }

    /// Same "explicit child-delete, don't rely on CASCADE" discipline as
    /// `remove_images_deletes_both_rows_and_makes_the_file_reimportable`'s
    /// orphan check for image_versions -- confirms no image_keywords rows
    /// survive a removed image.
    #[test]
    fn remove_images_deletes_orphaned_keyword_assignments() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image_id = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-a", 1, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        catalog.assign_keyword_path(&[image_id], &["kyoto".to_string()]).unwrap();

        catalog.remove_images(&[image_id]).unwrap();

        let orphans: i64 = catalog
            .conn
            .query_row(
                "SELECT count(*) FROM image_keywords WHERE image_id = ?1",
                params![image_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(orphans, 0);
    }

    #[test]
    fn list_all_image_keywords_is_flat_and_independent_of_list_images() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (a, b) = two_test_images(&catalog);
        let leaf = catalog.assign_keyword_path(&[a], &["kyoto".to_string()]).unwrap();
        catalog.assign_keyword_path(&[b], &["kyoto".to_string()]).unwrap();

        let all = catalog.list_all_image_keywords().unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().all(|assignment| assignment.keyword_id == leaf));
    }
}
