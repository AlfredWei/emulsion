//! Stored results of object removal (M7, RFC-0032 §12, ADR-0010 §4): one row per removal, the patch as a PNG blob.
//! The `remove` op of an edit stack names patches by id; nothing here knows about the stack.

use super::*;

/// A removal's stored result: where it goes in the photograph's full decode and the RGBA PNG itself.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredPatch {
    pub id: i64,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    /// The size of the full decode the position above refers to.
    pub src_w: u32,
    pub src_h: u32,
    pub engine: String,
    pub png: Vec<u8>,
}

impl Catalog {
    /// Stores a patch for `version_id` and returns its id (the `id` of `patch` is ignored).
    pub fn add_removal_patch(&self, version_id: i64, patch: &StoredPatch) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO removal_patches (version_id, x, y, w, h, src_w, src_h, engine, png) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![version_id, patch.x, patch.y, patch.w, patch.h, patch.src_w, patch.src_h, patch.engine, patch.png],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// The patches named by `ids` that belong to `version_id`, in the order of `ids`; an id that is unknown (or belongs
    /// to another photograph) is left out, which the caller shows as "needs regenerating".
    #[allow(dead_code)] // read by the baked source of slice D2d
    pub fn get_removal_patches(&self, version_id: i64, ids: &[i64]) -> Result<Vec<StoredPatch>> {
        let mut stmt = self.conn.prepare("SELECT x, y, w, h, src_w, src_h, engine, png FROM removal_patches WHERE id = ?1 AND version_id = ?2")?;
        let mut out = Vec::new();
        for &id in ids {
            if let Some(p) = stmt
                .query_row(params![id, version_id], |r| {
                    Ok(StoredPatch { id, x: r.get(0)?, y: r.get(1)?, w: r.get(2)?, h: r.get(3)?, src_w: r.get(4)?, src_h: r.get(5)?, engine: r.get(6)?, png: r.get(7)? })
                })
                .optional()?
            {
                out.push(p);
            }
        }
        Ok(out)
    }

    /// Deletes the patches of `version_id` that are not in `keep`. For a sweep after history has been pruned; the
    /// caller works out `keep` from the current stack and every history row, since undo can bring any of them back.
    #[allow(dead_code)] // the sweep is not wired yet (RFC-0032 §12.2)
    pub fn delete_removal_patches_except(&self, version_id: i64, keep: &[i64]) -> Result<usize> {
        let ids: Vec<i64> = {
            let mut stmt = self.conn.prepare("SELECT id FROM removal_patches WHERE version_id = ?1")?;
            let rows = stmt.query_map(params![version_id], |r| r.get(0))?;
            rows.collect::<Result<_>>()?
        };
        let mut n = 0;
        for id in ids.into_iter().filter(|id| !keep.contains(id)) {
            n += self.conn.execute("DELETE FROM removal_patches WHERE id = ?1", params![id])?;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(tag: u8) -> StoredPatch {
        StoredPatch { id: 0, x: 10, y: 20, w: 30, h: 40, src_w: 4000, src_h: 3000, engine: "lama-512-places2".into(), png: vec![tag; 5] }
    }

    #[test]
    fn a_stored_patch_comes_back_whole_and_in_the_order_asked() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image = catalog.add_image("/a.jpg").unwrap();
        let version = catalog.add_edit_stack(image, &EditStack::empty()).unwrap();
        let (a, b) = (catalog.add_removal_patch(version, &patch(1)).unwrap(), catalog.add_removal_patch(version, &patch(2)).unwrap());
        assert_ne!(a, b);
        let got = catalog.get_removal_patches(version, &[b, a]).unwrap();
        assert_eq!(got.iter().map(|p| p.id).collect::<Vec<_>>(), vec![b, a]);
        assert_eq!(got[1], StoredPatch { id: a, ..patch(1) });
    }

    #[test]
    fn an_unknown_patch_and_another_photographs_patch_are_left_out() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (i1, i2) = (catalog.add_image("/a.jpg").unwrap(), catalog.add_image("/b.jpg").unwrap());
        let (v1, v2) = (catalog.add_edit_stack(i1, &EditStack::empty()).unwrap(), catalog.add_edit_stack(i2, &EditStack::empty()).unwrap());
        let mine = catalog.add_removal_patch(v1, &patch(1)).unwrap();
        assert!(catalog.get_removal_patches(v2, &[mine, 9999]).unwrap().is_empty());
        assert_eq!(catalog.get_removal_patches(v1, &[9999, mine]).unwrap().len(), 1);
    }

    #[test]
    fn removing_the_photograph_removes_its_patches_and_a_sweep_keeps_what_it_is_told_to() {
        let catalog = Catalog::open_in_memory().unwrap();
        let image = catalog.add_image("/a.jpg").unwrap();
        let version = catalog.add_edit_stack(image, &EditStack::empty()).unwrap();
        let ids: Vec<i64> = (1..=3).map(|t| catalog.add_removal_patch(version, &patch(t)).unwrap()).collect();
        assert_eq!(catalog.delete_removal_patches_except(version, &[ids[1]]).unwrap(), 2);
        assert_eq!(catalog.get_removal_patches(version, &ids).unwrap().len(), 1);
        catalog.remove_images(&[image]).unwrap();
        let left: i64 = catalog.conn.query_row("SELECT count(*) FROM removal_patches", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 0, "the cascade took the rest");
    }

    #[test]
    fn a_catalog_backup_carries_the_patches() {
        // ADR-0010 §4: the reason the patch is a blob. The backup is SQLite's own copy, so a patch is in it.
        let (app_data, dest) = {
            let base = std::env::temp_dir().join(format!("emulsion-removal-backup-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&base);
            std::fs::create_dir_all(base.join("app")).unwrap();
            std::fs::create_dir_all(base.join("dest")).unwrap();
            (base.join("app"), base.join("dest"))
        };
        let catalog = Catalog::open(app_data.join("catalog.sqlite")).unwrap();
        let image = catalog.add_image("/a.jpg").unwrap();
        let version = catalog.add_edit_stack(image, &EditStack::empty()).unwrap();
        let id = catalog.add_removal_patch(version, &patch(7)).unwrap();
        let outcome = catalog.perform_backup(&dest, &app_data, true, true).unwrap();
        let copy = Connection::open(&outcome.path).unwrap();
        let png: Vec<u8> = copy.query_row("SELECT png FROM removal_patches WHERE id = ?1", params![id], |r| r.get(0)).unwrap();
        assert_eq!(png, vec![7; 5]);
        let _ = std::fs::remove_dir_all(app_data.parent().unwrap());
    }
}
