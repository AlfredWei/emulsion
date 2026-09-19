use super::*;

/// A collection, manual or smart (M2 Slice 5). `rules` is `None` for a
/// manual collection, `Some(...)` for a smart one -- opaque JSON on the
/// Rust side, interpreted only in the frontend (see the schema comment in
/// `migrate()`). `count` is `None` for smart collections; see
/// `list_collections`'s doc comment for why that's a deliberate override,
/// not `COUNT()`'s natural behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionSummary {
    pub id: i64,
    pub name: String,
    pub is_smart: bool,
    pub rules: Option<Vec<serde_json::Value>>,
    pub count: Option<i64>,
}

impl Catalog {
    /// Bare rail "+" with no selection -- an empty manual collection.
    pub fn create_collection(&self, name: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO collections (name, is_smart, rules_json) VALUES (?1, 0, NULL)",
            params![name],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// "Add to Collection… -> New Collection…" from a multi-selection: one
    /// transaction, create-then-populate together (matching
    /// `assign_keyword_path`'s own create-then-assign shape) rather than
    /// two separate calls -- an error between them would otherwise leave
    /// an empty, orphaned, confusingly-named collection with no
    /// indication anything went wrong.
    pub fn create_collection_with_images(&self, name: &str, image_ids: &[i64]) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO collections (name, is_smart, rules_json) VALUES (?1, 0, NULL)",
            params![name],
        )?;
        let collection_id = tx.last_insert_rowid();
        for &image_id in image_ids {
            tx.execute(
                "INSERT OR IGNORE INTO collection_images (collection_id, image_id) VALUES (?1, ?2)",
                params![collection_id, image_id],
            )?;
        }
        tx.commit()?;
        Ok(collection_id)
    }

    /// `rules` is opaque here -- see the schema comment in `migrate()` for
    /// why Rust never interprets it.
    pub fn create_smart_collection(&self, name: &str, rules: &[serde_json::Value]) -> Result<i64> {
        let rules_json = serde_json::to_string(rules).expect("rules are always serializable");
        self.conn.execute(
            "INSERT INTO collections (name, is_smart, rules_json) VALUES (?1, 1, ?2)",
            params![name, rules_json],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_smart_collection_rules(&self, collection_id: i64, rules: &[serde_json::Value]) -> Result<()> {
        let rules_json = serde_json::to_string(rules).expect("rules are always serializable");
        self.conn.execute(
            "UPDATE collections SET rules_json = ?2 WHERE id = ?1",
            params![collection_id, rules_json],
        )?;
        Ok(())
    }

    /// Explicit child-then-parent delete, same "don't rely on CASCADE"
    /// discipline as `remove_images`. Trivially correct for a smart
    /// collection too -- it has zero `collection_images` rows to begin
    /// with, and a DELETE matching nothing is an ordinary no-op (same
    /// behavior `remove_images` already relies on for unknown ids).
    pub fn delete_collection(&self, collection_id: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM collection_images WHERE collection_id = ?1",
            params![collection_id],
        )?;
        tx.execute("DELETE FROM collections WHERE id = ?1", params![collection_id])?;
        tx.commit()?;
        Ok(())
    }

    /// Batch, idempotent (`INSERT OR IGNORE`) -- matches
    /// `assign_keyword_path`'s membership-write idiom.
    pub fn add_images_to_collection(&self, collection_id: i64, image_ids: &[i64]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for &image_id in image_ids {
            tx.execute(
                "INSERT OR IGNORE INTO collection_images (collection_id, image_id) VALUES (?1, ?2)",
                params![collection_id, image_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn remove_images_from_collection(&self, collection_id: i64, image_ids: &[i64]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for &image_id in image_ids {
            tx.execute(
                "DELETE FROM collection_images WHERE collection_id = ?1 AND image_id = ?2",
                params![collection_id, image_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// `count` is `Some(n)` for a manual collection (a real `COUNT`),
    /// `None` for a smart one -- deliberately overridden here rather than
    /// left to whatever the aggregate naturally returns: `COUNT()` over a
    /// `LEFT JOIN` with no matching rows returns `0`, not `NULL`, which
    /// would silently read as "0 matches" instead of "not applicable" and
    /// be invisible in testing (a brand-new smart collection legitimately
    /// has 0 real matches too). The frontend computes a smart collection's
    /// real count client-side from `rules` + the already-loaded catalog.
    pub fn list_collections(&self) -> Result<Vec<CollectionSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.name, c.is_smart, c.rules_json, COUNT(ci.image_id)
             FROM collections c
             LEFT JOIN collection_images ci ON ci.collection_id = c.id
             GROUP BY c.id
             ORDER BY c.name",
        )?;
        let rows = stmt.query_map([], |row| {
            let is_smart: bool = row.get(2)?;
            let rules_json: Option<String> = row.get(3)?;
            let count: i64 = row.get(4)?;
            Ok(CollectionSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                is_smart,
                rules: rules_json.map(|json| {
                    serde_json::from_str(&json).expect("stored rules_json is always valid")
                }),
                count: (!is_smart).then_some(count),
            })
        })?;
        rows.collect()
    }

    /// A manual collection's membership -- fetched once by the frontend
    /// when the collection is clicked in the rail, cached by collection
    /// id there. Meaningless for a smart collection (always empty; smart
    /// membership is computed client-side from `rules`, never queried).
    pub fn list_collection_image_ids(&self, collection_id: i64) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT image_id FROM collection_images WHERE collection_id = ?1")?;
        let rows = stmt.query_map(params![collection_id], |row| row.get(0))?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::test_support::*;

    #[test]
    fn create_collection_with_images_is_atomic_and_populated() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (a, b) = two_test_images(&catalog);

        let collection_id = catalog.create_collection_with_images("Portfolio", &[a, b]).unwrap();

        let members = catalog.list_collection_image_ids(collection_id).unwrap();
        assert_eq!(members.len(), 2);
        assert!(members.contains(&a) && members.contains(&b));

        let collections = catalog.list_collections().unwrap();
        assert_eq!(collections.len(), 1);
        assert_eq!(collections[0].name, "Portfolio");
        assert!(!collections[0].is_smart);
        assert_eq!(collections[0].rules, None);
        assert_eq!(collections[0].count, Some(2));
    }

    #[test]
    fn add_and_remove_images_from_collection_round_trips() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (a, b) = two_test_images(&catalog);
        let collection_id = catalog.create_collection("Trip").unwrap();

        catalog.add_images_to_collection(collection_id, &[a, b]).unwrap();
        assert_eq!(catalog.list_collection_image_ids(collection_id).unwrap().len(), 2);

        // Idempotent: re-adding an already-member image is a silent no-op.
        catalog.add_images_to_collection(collection_id, &[a]).unwrap();
        assert_eq!(catalog.list_collection_image_ids(collection_id).unwrap().len(), 2);

        catalog.remove_images_from_collection(collection_id, &[a]).unwrap();
        let members = catalog.list_collection_image_ids(collection_id).unwrap();
        assert_eq!(members, vec![b]);
    }

    #[test]
    fn smart_collection_rules_round_trip_and_count_is_none_not_zero() {
        let catalog = Catalog::open_in_memory().unwrap();
        let rules = vec![serde_json::json!({"field": "rating", "op": ">=", "value": 4})];

        let collection_id = catalog.create_smart_collection("Best Shots", &rules).unwrap();

        let collections = catalog.list_collections().unwrap();
        let smart = collections.iter().find(|c| c.id == collection_id).unwrap();
        assert!(smart.is_smart);
        assert_eq!(smart.rules, Some(rules));
        assert_eq!(
            smart.count, None,
            "a smart collection's count must be None, not 0 -- 0 would be indistinguishable from a real zero-match count"
        );

        let updated_rules = vec![serde_json::json!({"field": "flag", "op": "==", "value": "pick"})];
        catalog.update_smart_collection_rules(collection_id, &updated_rules).unwrap();
        let collections = catalog.list_collections().unwrap();
        assert_eq!(collections[0].rules, Some(updated_rules));
    }

    #[test]
    fn delete_collection_removes_row_and_membership_including_for_a_smart_collection() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (a, _b) = two_test_images(&catalog);
        let manual_id = catalog.create_collection_with_images("Trip", &[a]).unwrap();
        let smart_id = catalog
            .create_smart_collection("Picks", &[serde_json::json!({"field": "flag", "op": "==", "value": "pick"})])
            .unwrap();

        catalog.delete_collection(manual_id).unwrap();
        catalog.delete_collection(smart_id).unwrap(); // no collection_images rows to begin with -- must not error

        assert_eq!(catalog.list_collections().unwrap().len(), 0);
        let orphans: i64 = catalog
            .conn
            .query_row("SELECT count(*) FROM collection_images", [], |row| row.get(0))
            .unwrap();
        assert_eq!(orphans, 0);
    }

    #[test]
    fn remove_images_deletes_orphaned_collection_membership() {
        let catalog = Catalog::open_in_memory().unwrap();
        let (a, b) = two_test_images(&catalog);
        let collection_id = catalog.create_collection_with_images("Trip", &[a, b]).unwrap();

        catalog.remove_images(&[a]).unwrap();

        let members = catalog.list_collection_image_ids(collection_id).unwrap();
        assert_eq!(members, vec![b], "removing an image must also drop its collection membership");
    }
}
