//! Culling analysis rows (M7, RFC-0031 slice A): the sharpness score and perceptual hash measured once per photo.

use super::*;
use crate::cull::CullMetrics;

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
}
