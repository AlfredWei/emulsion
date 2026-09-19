use super::*;

impl Catalog {
    /// Applies one location to many images in a single transaction
    /// (all-or-nothing). Altitude is deliberately left untouched: a
    /// searched or dropped location has none, and zeroing a photo's real
    /// altitude would be data loss.
    pub fn set_geo_location_batch(&self, image_ids: &[i64], latitude: f64, longitude: f64) -> Result<usize> {
        let tx = self.conn.unchecked_transaction()?;
        let mut changed = 0;
        {
            let mut stmt = tx.prepare("UPDATE images SET latitude = ?2, longitude = ?3 WHERE id = ?1")?;
            for id in image_ids {
                changed += stmt.execute(params![id, latitude, longitude])?;
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    /// The user's own Google Maps API key (RFC-0007 §3.1), kept in the
    /// same local `settings` KV as `cache_dir`.
    pub fn get_maps_api_key(&self) -> Result<Option<String>> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = 'maps_api_key'", [], |row| row.get(0))
            .optional()
    }

    /// Raw provider name (`"osm"`/`"google"`), interpreted by
    /// `geocode::Provider::from_setting`; `None` means never chosen.
    pub fn get_geocode_provider(&self) -> Result<Option<String>> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = 'geocode_provider'", [], |row| row.get(0))
            .optional()
    }

    pub fn set_geocode_provider(&self, provider: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES ('geocode_provider', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![provider],
        )?;
        Ok(())
    }

    /// `None` (or a blank string) removes the key.
    pub fn set_maps_api_key(&self, key: Option<&str>) -> Result<()> {
        match key.map(str::trim).filter(|k| !k.is_empty()) {
            Some(v) => self.conn.execute(
                "INSERT INTO settings (key, value) VALUES ('maps_api_key', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![v],
            ),
            None => self.conn.execute("DELETE FROM settings WHERE key = 'maps_api_key'", []),
        }?;
        Ok(())
    }

    /// Set or update the GPS coordinates and altitude for an image.
    pub fn set_geo_location(
        &self,
        image_id: i64,
        latitude: Option<f64>,
        longitude: Option<f64>,
        altitude: Option<f32>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE images SET latitude = ?2, longitude = ?3, altitude = ?4 WHERE id = ?1",
            params![image_id, latitude, longitude, altitude],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_geo_location_round_trips() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog opens");
        let image_id = catalog
            .add_image_with_edit_stack("/a.CR3", "hash-gps", 4096, &EditStack::empty(), &crate::metadata::ImageMetadata::default())
            .unwrap();

        catalog.set_geo_location(image_id, Some(25.033964), Some(121.564468), Some(50.5)).unwrap();

        let images = catalog.list_images().unwrap();
        assert_eq!(images[0].latitude, Some(25.033964));
        assert_eq!(images[0].longitude, Some(121.564468));
        assert_eq!(images[0].altitude, Some(50.5));
    }

    #[test]
    fn set_geo_location_batch_updates_all_targets_and_keeps_altitude() {
        let catalog = Catalog::open_in_memory().unwrap();
        let with_alt = crate::metadata::ImageMetadata {
            latitude: Some(1.0),
            longitude: Some(2.0),
            altitude: Some(123.0),
            ..Default::default()
        };
        let a = catalog.add_image_with_edit_stack("/a.CR3", "h-a", 1, &EditStack::empty(), &with_alt).unwrap();
        let b = catalog.add_image_with_edit_stack("/b.CR3", "h-b", 1, &EditStack::empty(), &Default::default()).unwrap();
        let untouched = catalog.add_image_with_edit_stack("/c.CR3", "h-c", 1, &EditStack::empty(), &Default::default()).unwrap();

        let changed = catalog.set_geo_location_batch(&[a, b, 9999], 25.03, 121.56).unwrap();
        assert_eq!(changed, 2, "a nonexistent id is skipped, not an error");

        let images = catalog.list_images().unwrap();
        let by_id = |id: i64| images.iter().find(|i| i.image_id == id).unwrap();
        assert_eq!((by_id(a).latitude, by_id(a).longitude, by_id(a).altitude), (Some(25.03), Some(121.56), Some(123.0)));
        assert_eq!((by_id(b).latitude, by_id(b).longitude), (Some(25.03), Some(121.56)));
        assert_eq!(by_id(untouched).latitude, None);
    }

    #[test]
    fn maps_api_key_round_trips_and_blank_removes_it() {
        let catalog = Catalog::open_in_memory().unwrap();
        assert_eq!(catalog.get_maps_api_key().unwrap(), None);
        catalog.set_maps_api_key(Some("  abc123  ")).unwrap();
        assert_eq!(catalog.get_maps_api_key().unwrap().as_deref(), Some("abc123"));
        catalog.set_maps_api_key(Some("def")).unwrap();
        assert_eq!(catalog.get_maps_api_key().unwrap().as_deref(), Some("def"));
        catalog.set_maps_api_key(Some("   ")).unwrap();
        assert_eq!(catalog.get_maps_api_key().unwrap(), None);
    }

    #[test]
    fn geocode_provider_setting_round_trips() {
        let catalog = Catalog::open_in_memory().unwrap();
        assert_eq!(catalog.get_geocode_provider().unwrap(), None);
        catalog.set_geocode_provider("google").unwrap();
        assert_eq!(catalog.get_geocode_provider().unwrap().as_deref(), Some("google"));
        catalog.set_geocode_provider("osm").unwrap();
        assert_eq!(catalog.get_geocode_provider().unwrap().as_deref(), Some("osm"));
    }
}
