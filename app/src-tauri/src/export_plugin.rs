//! Export-plugin hook (M5, RFC-0006) -- the v0 extensibility surface.
//!
//! Deliberately the narrowest useful hook: after a file is exported, hand
//! its path off to a user-configured external command, fire-and-forget
//! (RFC-0006 §3.3). No shell is ever invoked -- `std::process::Command`
//! receives the resolved argument list directly, so a filename or
//! configured argument containing shell metacharacters can't be
//! reinterpreted. This is a correctness property (arguments arrive exactly
//! as configured), not a sandboxing claim: the invoked command runs with
//! this app's own OS privileges, unconstrained, because the user is the
//! one who configured it (RFC-0006 §2/§5) -- real sandboxing/signing is
//! explicitly deferred to M8's hardening pass.

use crate::catalog::ExportPlugin;
use std::path::Path;

/// Substitutes the fixed three-placeholder set (RFC-0006 §3.2) against one
/// exported file's path. Plain string replacement, no shell parsing, no
/// globbing -- an argument with no placeholder passes through unchanged
/// (e.g. a fixed flag like `--upload`), and a placeholder may appear more
/// than once in the same argument.
pub fn resolve_args(template: &[String], output_path: &Path) -> Vec<String> {
    let path = output_path.to_string_lossy();
    let dir = output_path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let filename = output_path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();

    template
        .iter()
        .map(|arg| arg.replace("{path}", &path).replace("{dir}", &dir).replace("{filename}", &filename))
        .collect()
}

/// Spawns `plugin.command` against `output_path`, resolved via
/// [`resolve_args`]. Deliberately `.spawn()`, never `.output()`/`.status()`
/// -- the export flow never waits on the child process, so a slow or
/// hanging plugin can't stall or fail an export (RFC-0006 §3.3). Returns
/// the `spawn()` error, if any, for the caller to surface as
/// `ExportResult::plugin_error` -- kept separate from an export failure,
/// since a plugin misconfiguration must never make an otherwise-successful
/// export look like one.
pub fn invoke(plugin: &ExportPlugin, output_path: &Path) -> std::io::Result<()> {
    let args = resolve_args(&plugin.args_template, output_path);
    std::process::Command::new(&plugin.command).args(args).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn out_path() -> PathBuf {
        PathBuf::from("/exports/vacation/photo-1.jpg")
    }

    #[test]
    fn resolve_args_substitutes_path_dir_and_filename() {
        let template = vec!["--path".to_string(), "{path}".to_string(), "--in".to_string(), "{dir}".to_string(), "--name".to_string(), "{filename}".to_string()];

        let resolved = resolve_args(&template, &out_path());

        assert_eq!(
            resolved,
            vec![
                "--path",
                "/exports/vacation/photo-1.jpg",
                "--in",
                "/exports/vacation",
                "--name",
                "photo-1.jpg",
            ]
        );
    }

    #[test]
    fn resolve_args_passes_through_arguments_with_no_placeholder() {
        let template = vec!["--upload".to_string(), "--quiet".to_string()];

        let resolved = resolve_args(&template, &out_path());

        assert_eq!(resolved, vec!["--upload", "--quiet"]);
    }

    #[test]
    fn resolve_args_substitutes_the_same_placeholder_more_than_once() {
        let template = vec!["{filename} -> {filename}".to_string()];

        let resolved = resolve_args(&template, &out_path());

        assert_eq!(resolved, vec!["photo-1.jpg -> photo-1.jpg"]);
    }

    #[test]
    fn invoke_spawns_a_real_command_that_touches_a_marker_file() {
        if cfg!(windows) {
            eprintln!("skipping: this test's marker command (touch) is Unix-only");
            return;
        }
        let marker = std::env::temp_dir().join("emulsion-export-plugin-test-marker");
        let _ = std::fs::remove_file(&marker);
        let plugin = ExportPlugin {
            id: 1,
            name: "Marker".to_string(),
            command: "touch".to_string(),
            args_template: vec!["{path}".to_string()],
            created_at: String::new(),
        };

        invoke(&plugin, &marker).expect("spawning a real, present command succeeds");
        // spawn() only starts the process -- give it a moment to actually
        // run before asserting on its effect. A generous, one-shot sleep
        // (not a poll loop) is fine for a `touch` invocation this trivial.
        std::thread::sleep(std::time::Duration::from_millis(200));

        assert!(marker.exists(), "touch should have created the marker file");
        let _ = std::fs::remove_file(&marker);
    }

    #[test]
    fn invoke_returns_a_clean_error_for_a_nonexistent_command() {
        let plugin = ExportPlugin {
            id: 1,
            name: "Bad".to_string(),
            command: "/nonexistent/not-a-real-binary".to_string(),
            args_template: vec![],
            created_at: String::new(),
        };

        let result = invoke(&plugin, &out_path());

        assert!(result.is_err());
    }
}
