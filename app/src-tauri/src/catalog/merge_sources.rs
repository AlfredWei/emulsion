use super::*;

impl Catalog {
    /// Records one `hdr_merge_sources` provenance row per bracket member
    /// (RFC-0003 §3.6) -- `sources` is `(source_image_id, ordinal,
    /// ev_offset, dx, dy)`, in the caller's own original bracket order.
    /// All-or-nothing: a partial provenance record for a merge result
    /// would be actively misleading (looks complete, silently isn't), so
    /// this is one transaction rather than best-effort per row.
    pub fn add_hdr_merge_sources(&self, result_image_id: i64, sources: &[(i64, i32, f32, i32, i32)]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for &(source_image_id, ordinal, ev_offset, dx, dy) in sources {
            tx.execute(
                "INSERT INTO hdr_merge_sources (result_image_id, source_image_id, ordinal, ev_offset, dx, dy)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![result_image_id, source_image_id, ordinal, ev_offset, dx, dy],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Reads back the provenance rows `add_hdr_merge_sources` wrote for
    /// one merge result, ordered by `ordinal` -- the natural read
    /// counterpart to that write-only method. Kept as real pub API ready
    /// for a future "Show HDR sources" UI (same "no UI trigger yet"
    /// precedent as `add_image_with_metadata`'s own doc comment) --
    /// exercised today by this file's own test plus `hdr_merge.rs`'s
    /// real-bracket end-to-end test, which (being a different module)
    /// has no access to this one's private `conn` field.
    #[allow(dead_code)]
    pub fn get_hdr_merge_sources(&self, result_image_id: i64) -> Result<Vec<(i64, i32, f32, i32, i32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT source_image_id, ordinal, ev_offset, dx, dy FROM hdr_merge_sources
             WHERE result_image_id = ?1 ORDER BY ordinal",
        )?;
        let rows = stmt
            .query_map(params![result_image_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
            })?
            .collect();
        rows
    }

    /// Records one `panorama_merge_sources` provenance row per stitched
    /// frame (RFC-0004 §3.6) -- `sources` is `(source_image_id, ordinal,
    /// homography_json)`, in the caller's own original selection order.
    /// All-or-nothing, same reasoning as `add_hdr_merge_sources`: a
    /// partial provenance record would be actively misleading.
    pub fn add_panorama_merge_sources(&self, result_image_id: i64, sources: &[(i64, i32, String)]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for (source_image_id, ordinal, homography_json) in sources {
            tx.execute(
                "INSERT INTO panorama_merge_sources (result_image_id, source_image_id, ordinal, homography_json)
                 VALUES (?1, ?2, ?3, ?4)",
                params![result_image_id, source_image_id, ordinal, homography_json],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Reads back the provenance rows `add_panorama_merge_sources` wrote
    /// for one stitch result, ordered by `ordinal` -- same "ready for a
    /// future UI, no trigger yet" precedent as `get_hdr_merge_sources`.
    #[allow(dead_code)]
    pub fn get_panorama_merge_sources(&self, result_image_id: i64) -> Result<Vec<(i64, i32, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT source_image_id, ordinal, homography_json FROM panorama_merge_sources
             WHERE result_image_id = ?1 ORDER BY ordinal",
        )?;
        let rows = stmt
            .query_map(params![result_image_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect();
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_hdr_merge_sources_records_one_row_per_bracket_member() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let a = catalog.add_image("/a.CR3").unwrap();
        let b = catalog.add_image("/b.CR3").unwrap();
        let result = catalog
            .add_image_with_edit_stack("/merged.jpg", "hash-merged", 100, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();

        catalog
            .add_hdr_merge_sources(result, &[(a, 0, 0.0, 0, 0), (b, 1, -1.0, -3, 2)])
            .unwrap();

        let rows = catalog.get_hdr_merge_sources(result).unwrap();
        assert_eq!(rows, vec![(a, 0, 0.0, 0, 0), (b, 1, -1.0, -3, 2)]);
    }

    /// Removing the merge *result* drops its whole provenance row set;
    /// removing one *source* only drops that one row, leaving the result
    /// and its other sources untouched -- see `remove_images`'s own doc
    /// comment on why this is two explicit directions, not a plain
    /// `ON DELETE CASCADE` off of `source_image_id`.
    #[test]
    fn removing_a_merge_result_or_a_source_cleans_up_hdr_merge_sources_correctly() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let a = catalog.add_image("/a.CR3").unwrap();
        let b = catalog.add_image("/b.CR3").unwrap();
        let result = catalog
            .add_image_with_edit_stack("/merged.jpg", "hash-merged", 100, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        catalog
            .add_hdr_merge_sources(result, &[(a, 0, 0.0, 0, 0), (b, 1, -1.0, -3, 2)])
            .unwrap();

        catalog.remove_images(&[a]).unwrap();
        let remaining_after_source_removed: i64 = catalog
            .conn
            .query_row("SELECT count(*) FROM hdr_merge_sources WHERE result_image_id = ?1", params![result], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining_after_source_removed, 1, "removing one source must only drop that source's own row");

        catalog.remove_images(&[result]).unwrap();
        let remaining_after_result_removed: i64 = catalog
            .conn
            .query_row("SELECT count(*) FROM hdr_merge_sources WHERE result_image_id = ?1", params![result], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining_after_result_removed, 0, "removing the result must drop its whole provenance row set");
    }

    #[test]
    fn add_panorama_merge_sources_records_one_row_per_frame() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let a = catalog.add_image("/a.jpg").unwrap();
        let b = catalog.add_image("/b.jpg").unwrap();
        let result = catalog
            .add_image_with_edit_stack("/pano.jpg", "hash-pano", 100, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();

        catalog
            .add_panorama_merge_sources(result, &[(a, 0, "[1,0,0,0,1,0,0,0,1]".to_string()), (b, 1, "[1,0,10,0,1,0,0,0,1]".to_string())])
            .unwrap();

        let rows = catalog.get_panorama_merge_sources(result).unwrap();
        assert_eq!(
            rows,
            vec![(a, 0, "[1,0,0,0,1,0,0,0,1]".to_string()), (b, 1, "[1,0,10,0,1,0,0,0,1]".to_string())]
        );
    }

    /// Same both-directions removal contract as HDR merge's own
    /// `removing_a_merge_result_or_a_source_cleans_up_hdr_merge_sources_correctly`.
    #[test]
    fn removing_a_panorama_result_or_a_source_cleans_up_panorama_merge_sources_correctly() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let a = catalog.add_image("/a.jpg").unwrap();
        let b = catalog.add_image("/b.jpg").unwrap();
        let result = catalog
            .add_image_with_edit_stack("/pano.jpg", "hash-pano", 100, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();
        catalog
            .add_panorama_merge_sources(result, &[(a, 0, "[]".to_string()), (b, 1, "[]".to_string())])
            .unwrap();

        catalog.remove_images(&[a]).unwrap();
        let remaining_after_source_removed: i64 = catalog
            .conn
            .query_row("SELECT count(*) FROM panorama_merge_sources WHERE result_image_id = ?1", params![result], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining_after_source_removed, 1, "removing one source must only drop that source's own row");

        catalog.remove_images(&[result]).unwrap();
        let remaining_after_result_removed: i64 = catalog
            .conn
            .query_row("SELECT count(*) FROM panorama_merge_sources WHERE result_image_id = ?1", params![result], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining_after_result_removed, 0, "removing the result must drop its whole provenance row set");
    }
}
