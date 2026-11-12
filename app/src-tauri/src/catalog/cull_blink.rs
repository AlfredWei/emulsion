//! Blink evidence rows (M7, RFC-0031 §8.15): what the eye check said of each face of a photograph.

use super::*;
use crate::blink_pass::{counts, BlinkCounts, FaceBlink};

impl Catalog {
    pub fn set_cull_blink(&self, image_id: i64, faces: &[FaceBlink], algo: i64) -> Result<()> {
        let c = counts(faces);
        let detail = serde_json::to_string(faces).unwrap_or_else(|_| "[]".to_string());
        self.conn.execute(
            "INSERT OR REPLACE INTO cull_blink (image_id, faces, judged, closed, one_closed, detail_json, algo) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![image_id, c.faces, c.judged, c.closed, c.one_closed, detail, algo],
        )?;
        Ok(())
    }

    /// The stored rows of rules version `algo` for those of `image_ids` that have one (the rest are simply absent).
    pub fn get_cull_blink(&self, image_ids: &[i64], algo: i64) -> Result<Vec<(i64, BlinkCounts, Vec<FaceBlink>)>> {
        let mut stmt = self.conn.prepare("SELECT faces, judged, closed, one_closed, detail_json FROM cull_blink WHERE image_id = ?1 AND algo = ?2")?;
        let mut out = Vec::new();
        for &id in image_ids {
            let row = stmt
                .query_row(params![id, algo], |r| {
                    Ok((
                        BlinkCounts { faces: r.get(0)?, judged: r.get(1)?, closed: r.get(2)?, one_closed: r.get(3)? },
                        r.get::<_, String>(4)?,
                    ))
                })
                .optional()?;
            if let Some((c, detail)) = row {
                out.push((id, c, serde_json::from_str(&detail).unwrap_or_default()));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blink_pass::EyesWord;

    #[test]
    fn rows_are_stored_per_photo_and_read_back_by_version() {
        let catalog = Catalog::open_in_memory().unwrap();
        let id = catalog.add_image("/photos/a.jpg").unwrap();
        let faces = vec![
            FaceBlink { x: 0.1, y: 0.2, w: 0.1, h: 0.1, eyes: EyesWord::Closed },
            FaceBlink { x: 0.5, y: 0.2, w: 0.1, h: 0.1, eyes: EyesWord::CannotTell },
        ];
        catalog.set_cull_blink(id, &faces, 1).unwrap();
        let got = catalog.get_cull_blink(&[id, id + 99], 1).unwrap();
        assert_eq!(got.len(), 1, "a photo with no row is absent");
        assert_eq!(got[0].1, BlinkCounts { faces: 2, judged: 1, closed: 1, one_closed: 0 });
        assert_eq!(got[0].2, faces);
        assert!(catalog.get_cull_blink(&[id], 2).unwrap().is_empty(), "another rules version is not served");
        catalog.set_cull_blink(id, &[], 1).unwrap();
        assert_eq!(catalog.get_cull_blink(&[id], 1).unwrap()[0].1, BlinkCounts::default(), "a photo with no faces is a row too");
    }
}
