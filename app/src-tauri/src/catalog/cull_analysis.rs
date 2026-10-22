//! Culling analysis rows (M7, RFC-0031 slice A): the sharpness score and perceptual hash measured once per photo.

use super::*;
use crate::cull::CullMetrics;
use crate::cull_groups::GroupInput;

impl Catalog {
    pub fn set_cull_analysis(&self, image_id: i64, metrics: &CullMetrics, algo: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO cull_analysis (image_id, sharpness, sharpness_tile, dhash, algo)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![image_id, metrics.sharpness, metrics.sharpness_tile, &metrics.dhash[..], algo],
        )?;
        Ok(())
    }

    #[allow(dead_code)] // read by the grouping of slice B
    pub fn get_cull_analysis(&self, image_id: i64) -> Result<Option<CullMetrics>> {
        self.conn
            .query_row(
                "SELECT sharpness, sharpness_tile, dhash FROM cull_analysis WHERE image_id = ?1",
                params![image_id],
                |row| {
                    let blob: Vec<u8> = row.get(2)?;
                    let dhash: [u8; 32] = blob.try_into().map_err(|_| rusqlite::Error::InvalidColumnType(2, "dhash".into(), rusqlite::types::Type::Blob))?;
                    Ok(CullMetrics { sharpness: row.get(0)?, sharpness_tile: row.get(1)?, dhash })
                },
            )
            .optional()
    }

    /// Everything the grouping needs, for every photo measured with metric version `algo`: the primary (first
    /// non-virtual-copy) version, the camera, the capture time and the stored measurements.
    pub fn similar_group_inputs(&self, algo: i64) -> Result<Vec<GroupInput>> {
        let mut stmt = self.conn.prepare(
            "SELECT i.id,
                    (SELECT id FROM image_versions WHERE image_id = i.id AND is_virtual_copy = 0 ORDER BY id ASC LIMIT 1),
                    trim(coalesce(i.camera_make, '') || ' ' || coalesce(i.camera_model, '')),
                    i.captured_at, c.sharpness_tile, c.dhash
             FROM images i JOIN cull_analysis c ON c.image_id = i.id
             WHERE c.algo = ?1",
        )?;
        let rows = stmt.query_map(params![algo], |row| {
            let version_id: Option<i64> = row.get(1)?;
            let blob: Vec<u8> = row.get(5)?;
            Ok((version_id, row.get::<_, i64>(0)?, row.get::<_, String>(2)?, row.get::<_, Option<String>>(3)?, row.get::<_, f64>(4)?, blob))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (version_id, image_id, camera, captured_at, sharpness_tile, blob) = row?;
            // A photo with no primary version cannot be shown; one with a damaged hash is left out, not fatal.
            let (Some(version_id), Ok(dhash)) = (version_id, <[u8; 32]>::try_from(blob)) else { continue };
            out.push(GroupInput { image_id, version_id, camera, captured_at, sharpness_tile, dhash });
        }
        Ok(out)
    }

    /// (photos measured with `algo`, all photos): the "Analysing n / m" figure.
    pub fn cull_analysis_counts(&self, algo: i64) -> Result<(i64, i64)> {
        let analysed = self.conn.query_row("SELECT count(*) FROM cull_analysis WHERE algo = ?1", params![algo], |r| r.get(0))?;
        let total = self.conn.query_row("SELECT count(*) FROM images", [], |r| r.get(0))?;
        Ok((analysed, total))
    }

    /// Photos with no analysis row of version `algo` (never measured, or measured by an older metric), oldest first.
    pub fn images_needing_cull_analysis(&self, algo: i64) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT i.id, i.path FROM images i
             LEFT JOIN cull_analysis c ON c.image_id = i.id
             WHERE c.image_id IS NULL OR c.algo <> ?1
             ORDER BY i.id",
        )?;
        let rows = stmt.query_map(params![algo], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(n: u8) -> CullMetrics {
        CullMetrics { sharpness: n as f64, sharpness_tile: n as f64 * 2.0, dhash: [n; 32] }
    }

    #[test]
    fn a_row_round_trips_and_leaves_the_need_list() {
        let catalog = Catalog::open_in_memory().unwrap();
        let a = catalog.add_image("/a.jpg").unwrap();
        let b = catalog.add_image("/b.jpg").unwrap();
        assert_eq!(catalog.images_needing_cull_analysis(1).unwrap().len(), 2);

        catalog.set_cull_analysis(a, &metrics(7), 1).unwrap();
        assert_eq!(catalog.get_cull_analysis(a).unwrap(), Some(metrics(7)));
        assert_eq!(catalog.get_cull_analysis(b).unwrap(), None);
        assert_eq!(catalog.images_needing_cull_analysis(1).unwrap(), vec![(b, "/b.jpg".to_string())]);
        // A newer metric version wants everything again.
        assert_eq!(catalog.images_needing_cull_analysis(2).unwrap().len(), 2);
    }

    #[test]
    fn writing_again_replaces_and_removing_the_photo_removes_the_row() {
        let catalog = Catalog::open_in_memory().unwrap();
        let a = catalog.add_image("/a.jpg").unwrap();
        catalog.set_cull_analysis(a, &metrics(1), 1).unwrap();
        catalog.set_cull_analysis(a, &metrics(2), 1).unwrap();
        assert_eq!(catalog.get_cull_analysis(a).unwrap(), Some(metrics(2)));
        catalog.conn.execute("DELETE FROM images WHERE id = ?1", params![a]).unwrap();
        assert_eq!(catalog.get_cull_analysis(a).unwrap(), None);
    }

    #[test]
    fn group_inputs_carry_the_primary_version_camera_and_time_for_current_rows_only() {
        let catalog = Catalog::open_in_memory().unwrap();
        let a = catalog.add_image("/a.jpg").unwrap();
        let first = catalog.add_edit_stack(a, &EditStack::empty()).unwrap();
        let b = catalog.add_image("/b.jpg").unwrap();
        catalog.add_edit_stack(b, &EditStack::empty()).unwrap();
        catalog
            .conn
            .execute("UPDATE images SET camera_make = 'Canon', camera_model = 'R5', captured_at = '2026-03-10T10:00:00' WHERE id = ?1", params![a])
            .unwrap();
        catalog.set_cull_analysis(a, &metrics(3), 1).unwrap();
        catalog.set_cull_analysis(b, &metrics(4), 0).unwrap(); // an older metric: left out

        let inputs = catalog.similar_group_inputs(1).unwrap();
        assert_eq!(inputs.len(), 1);
        assert_eq!((inputs[0].image_id, inputs[0].version_id), (a, first));
        assert_eq!(inputs[0].camera, "Canon R5");
        assert_eq!(inputs[0].captured_at.as_deref(), Some("2026-03-10T10:00:00"));
        assert_eq!(inputs[0].dhash, [3; 32]);
        assert_eq!(catalog.cull_analysis_counts(1).unwrap(), (1, 2));
    }
}
