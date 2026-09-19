use super::*;

/// A user-configured export-plugin hook (M5, RFC-0006). `args_template` is
/// stored as JSON in the DB but kept as a plain `Vec<String>` here -- the
/// same "Rust never interprets the payload beyond round-tripping it"
/// boundary `PresetEntry.edit_stack` doesn't quite need (this one really is
/// opaque strings, substituted by `export_plugin.rs` at invocation time,
/// never inspected by the catalog layer itself).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportPlugin {
    pub id: i64,
    pub name: String,
    pub command: String,
    pub args_template: Vec<String>,
    pub created_at: String,
}

impl Catalog {
    /// Export plugins (M5, RFC-0006 §3.1). Unlike presets (create/list/
    /// delete only -- a preset is saved once and re-applied, not edited in
    /// place), a plugin's command/args are expected to get tweaked as the
    /// user iterates on getting the invocation right, so this also gets an
    /// `update`, matching `update_backup_settings`'s "settings the user
    /// revises" shape rather than presets' "named snapshot" one.
    pub fn add_export_plugin(&self, name: &str, command: &str, args_template: &[String]) -> Result<ExportPlugin> {
        let json = serde_json::to_string(args_template).expect("Vec<String> is always serializable");
        self.conn.execute(
            "INSERT INTO export_plugins (name, command, args_template_json) VALUES (?1, ?2, ?3)",
            params![name, command, json],
        )?;
        let id = self.conn.last_insert_rowid();
        let created_at: String = self.conn.query_row(
            "SELECT created_at FROM export_plugins WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        Ok(ExportPlugin {
            id,
            name: name.to_string(),
            command: command.to_string(),
            args_template: args_template.to_vec(),
            created_at,
        })
    }

    pub fn list_export_plugins(&self) -> Result<Vec<ExportPlugin>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, command, args_template_json, created_at FROM export_plugins ORDER BY id ASC")?;
        let rows = stmt.query_map([], |row| {
            let json: String = row.get(3)?;
            let args_template: Vec<String> =
                serde_json::from_str(&json).expect("stored args templates are always valid JSON");
            Ok(ExportPlugin {
                id: row.get(0)?,
                name: row.get(1)?,
                command: row.get(2)?,
                args_template,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    pub fn update_export_plugin(&self, id: i64, name: &str, command: &str, args_template: &[String]) -> Result<()> {
        let json = serde_json::to_string(args_template).expect("Vec<String> is always serializable");
        self.conn.execute(
            "UPDATE export_plugins SET name = ?1, command = ?2, args_template_json = ?3 WHERE id = ?4",
            params![name, command, json, id],
        )?;
        Ok(())
    }

    pub fn delete_export_plugin(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM export_plugins WHERE id = ?1", params![id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- M5 Export plugins (RFC-0006) ---------------------------------

    #[test]
    fn add_export_plugin_round_trips_name_command_and_args() {
        let catalog = Catalog::open_in_memory().unwrap();

        let plugin = catalog
            .add_export_plugin("Notify", "/usr/bin/notify", &["--file".to_string(), "{path}".to_string()])
            .unwrap();

        assert_eq!(plugin.name, "Notify");
        assert_eq!(plugin.command, "/usr/bin/notify");
        assert_eq!(plugin.args_template, vec!["--file", "{path}"]);
        assert!(plugin.id > 0);
    }

    #[test]
    fn list_export_plugins_orders_oldest_first_and_includes_every_row() {
        let catalog = Catalog::open_in_memory().unwrap();
        catalog.add_export_plugin("First", "/bin/true", &[]).unwrap();
        catalog.add_export_plugin("Second", "/bin/true", &[]).unwrap();

        let plugins = catalog.list_export_plugins().unwrap();

        assert_eq!(plugins.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["First", "Second"]);
    }

    #[test]
    fn update_export_plugin_overwrites_name_command_and_args_in_place() {
        let catalog = Catalog::open_in_memory().unwrap();
        let plugin = catalog.add_export_plugin("Old", "/bin/old", &["--old".to_string()]).unwrap();

        catalog
            .update_export_plugin(plugin.id, "New", "/bin/new", &["--new".to_string(), "{filename}".to_string()])
            .unwrap();

        let plugins = catalog.list_export_plugins().unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "New");
        assert_eq!(plugins[0].command, "/bin/new");
        assert_eq!(plugins[0].args_template, vec!["--new", "{filename}"]);
    }

    #[test]
    fn delete_export_plugin_removes_it() {
        let catalog = Catalog::open_in_memory().unwrap();
        let plugin = catalog.add_export_plugin("Temp", "/bin/true", &[]).unwrap();

        catalog.delete_export_plugin(plugin.id).unwrap();

        assert_eq!(catalog.list_export_plugins().unwrap(), vec![]);
    }
}
