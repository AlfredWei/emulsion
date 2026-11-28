//! Model editions (RFC-0038 slice 4, ADR-0012): which families of AI model a build may use.
//!
//! An edition is a named **allow-list** of [`ModelFamily`] in `models/bundled-models.json`, so a family added later is
//! never in a restricted edition by accident. Two inputs choose it, and they can only **narrow** each other:
//!
//! - the build's name, `EMULSION_EDITION` at compile time (`option_env!`), default `default`;
//! - `models/bundled-models.edition.json` in the installer's resources, written by `fetch-bundled-models.mjs` for the same name.
//!
//! So a restricted bundle that loses its file still follows the compiled name, and a `--edition` flag passed without the
//! variable still narrows a default binary. An unknown name, or an edition file that cannot be read, includes nothing.
//!
//! An excluded family is never used, **even when a valid copy is already on disk** from an earlier build: its status is
//! `not_in_edition`, nothing is downloaded, imported or seeded, and cached files are left alone. Stored results (patches,
//! denoised copies, enlargements, face rows) are the user's data and stay readable. The module takes no position on which
//! edition any real build should be: that is D7 and D11 in RFC-0038.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// The model families, one per feature that a model drives. `Face` has no bundled file (YuNet and SFace are downloaded).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelFamily {
    Denoise,
    Segment,
    Detect,
    Inpaint,
    Upscale,
    Blink,
    Face,
}

impl ModelFamily {
    pub const ALL: [ModelFamily; 7] = [Self::Denoise, Self::Segment, Self::Detect, Self::Inpaint, Self::Upscale, Self::Blink, Self::Face];

    /// How the feature is named in the one sentence the UI shows.
    pub fn feature(self) -> &'static str {
        match self {
            Self::Denoise => "AI Denoise",
            Self::Segment => "Select Subject",
            Self::Detect => "Person detection",
            Self::Inpaint => "Remove",
            Self::Upscale => "Enlarge (AI)",
            Self::Blink => "Eye check",
            Self::Face => "Face detection",
        }
    }
}

/// What a command returns when it is asked to use a model its edition leaves out.
pub fn refusal(family: ModelFamily) -> String {
    format!("{} is not included in this edition of Emulsion.", family.feature())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edition {
    pub name: String,
    pub include: BTreeSet<ModelFamily>,
}

impl Edition {
    pub fn includes(&self, family: ModelFamily) -> bool {
        self.include.contains(&family)
    }

    /// The families this edition leaves out, in a fixed order.
    pub fn excluded(&self) -> Vec<ModelFamily> {
        ModelFamily::ALL.into_iter().filter(|f| !self.includes(*f)).collect()
    }

    /// Every family: what every build has shipped so far.
    #[cfg(test)]
    pub fn all(name: &str) -> Self {
        Self { name: name.to_string(), include: ModelFamily::ALL.into_iter().collect() }
    }

    fn none(name: &str) -> Self {
        Self { name: name.to_string(), include: BTreeSet::new() }
    }
}

#[derive(Deserialize)]
struct EditionDef {
    include: Vec<ModelFamily>,
}

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    editions: BTreeMap<String, EditionDef>,
}

/// The edition file the fetch script writes into the bundle.
#[derive(Deserialize)]
struct BundledEdition {
    include: Vec<ModelFamily>,
}

/// File name of the generated edition file inside the installer's `models/` resources.
#[cfg_attr(debug_assertions, allow(dead_code))]
pub const EDITION_FILE: &str = "bundled-models.edition.json";

/// The `editions` part of `models/bundled-models.json`, cut out by `build.rs` so the binary does not carry the download
/// URLs of the files list.
const MANIFEST: &str = include_str!(concat!(env!("OUT_DIR"), "/editions.json"));

/// Pure. `compiled` is the name the binary was built for (`None` = `default`); `bundled` is the text of the edition
/// file when the bundle has one. The result is the intersection of both; an unknown name or an unreadable file
/// includes nothing, which is loud (every AI feature says "not included") rather than silently permissive.
pub fn resolve(manifest: &str, compiled: Option<&str>, bundled: Option<&str>) -> Edition {
    let name = compiled.unwrap_or("default");
    let parsed: Manifest = match serde_json::from_str(manifest) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("editions: the model manifest cannot be read ({e}); no model is included");
            return Edition::none(name);
        }
    };
    let Some(def) = parsed.editions.get(name) else {
        eprintln!("editions: unknown edition `{name}`; no model is included");
        return Edition::none(name);
    };
    let mut edition = Edition { name: name.to_string(), include: def.include.iter().copied().collect() };
    if let Some(text) = bundled {
        match serde_json::from_str::<BundledEdition>(text) {
            Ok(b) => edition.include.retain(|f| b.include.contains(f)),
            Err(e) => {
                eprintln!("editions: the bundle's edition file cannot be read ({e}); no model is included");
                edition.include.clear();
            }
        }
    }
    edition
}

static CURRENT: OnceLock<Edition> = OnceLock::new();

/// The Mac App Store build is always `store`, whatever `EMULSION_EDITION` says (ADR-0011).
fn compiled_name() -> Option<&'static str> {
    if cfg!(feature = "channel-mas") {
        return Some("store");
    }
    option_env!("EMULSION_EDITION").filter(|s| !s.is_empty())
}

/// Reads the bundle's edition file (release builds only: a debug build can hold a stale `target/debug/models` folder
/// from an earlier bundled build, so it follows `EMULSION_EDITION` alone) and fixes the edition for the run.
/// Call first in `setup()`, before anything asks [`current`].
pub fn init(app: &tauri::AppHandle) {
    #[cfg(not(debug_assertions))]
    let bundled = {
        use tauri::Manager;
        let path = app.path().resource_dir().ok().map(|d| d.join("models").join(EDITION_FILE));
        match path.map(std::fs::read_to_string) {
            Some(Ok(text)) => Some(text),
            Some(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => None,
            Some(Err(_)) => Some(String::new()), // there, but unreadable: resolve() will include nothing
            None => None,
        }
    };
    #[cfg(debug_assertions)]
    let bundled: Option<String> = {
        let _ = app;
        None
    };
    let _ = CURRENT.set(resolve(MANIFEST, compiled_name(), bundled.as_deref()));
}

/// The edition this run uses. Before [`init`] (unit tests) it is the compiled name alone.
pub fn current() -> &'static Edition {
    CURRENT.get_or_init(|| resolve(MANIFEST, compiled_name(), None))
}

/// A feature whose code a build leaves out with a Cargo feature, as opposed to a model family the edition leaves out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompiledOut {
    LensProfiles,
}

fn compiled_out() -> Vec<CompiledOut> {
    let mut out = Vec::new();
    if !cfg!(feature = "lens-profiles") {
        out.push(CompiledOut::LensProfiles);
    }
    out
}

/// What the UI needs: the edition, the families it leaves out, the channel and what that channel contains, and the
/// features compiled out of this binary.
#[derive(Debug, Clone, Serialize)]
pub struct EditionInfo {
    pub name: String,
    pub excluded: Vec<ModelFamily>,
    pub channel: crate::channel::Channel,
    pub caps: crate::channel::ChannelCaps,
    pub compiled_out: Vec<CompiledOut>,
}

#[tauri::command]
pub fn model_edition() -> EditionInfo {
    let e = current();
    let channel = crate::channel::CURRENT;
    EditionInfo { name: e.name.clone(), excluded: e.excluded(), channel, caps: channel.caps(), compiled_out: compiled_out() }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use ModelFamily::*;

    const M: &str = r#"{
      "editions": {
        "default": { "comment": "all", "include": ["denoise","segment","detect","inpaint","upscale","blink","face"] },
        "test-no-lama": { "include": ["denoise","segment","detect","upscale","blink","face"] }
      }
    }"#;

    /// Every family but `out`: the shape of a restricted edition, for the model modules' tests.
    pub fn without(out: ModelFamily) -> Edition {
        let mut e = Edition::all("test");
        e.include.remove(&out);
        e
    }

    fn fams(e: &Edition) -> Vec<ModelFamily> {
        e.include.iter().copied().collect()
    }

    #[test]
    fn the_real_manifest_default_edition_includes_every_family() {
        let e = resolve(MANIFEST, None, None);
        assert_eq!(e.name, "default");
        assert_eq!(e.include.len(), ModelFamily::ALL.len(), "default must list every family so the default build is unchanged");
        assert!(e.excluded().is_empty());
    }

    #[test]
    fn every_edition_in_the_real_manifest_names_only_known_families() {
        // Deserialising fails on an unknown family, so a typo in the JSON cannot silently drop a model.
        let m: serde_json::Value = serde_json::from_str(MANIFEST).unwrap();
        for name in m["editions"].as_object().expect("the manifest has an `editions` object").keys() {
            let e = resolve(MANIFEST, Some(name), None);
            assert!(!e.include.is_empty(), "edition `{name}` includes nothing: unknown family or unreadable manifest?");
        }
    }

    #[test]
    fn every_manifest_file_names_a_known_family_and_the_constants_agree() {
        let m: serde_json::Value = serde_json::from_str(include_str!("../../../models/bundled-models.json")).unwrap();
        for f in m["files"].as_array().unwrap() {
            let family: ModelFamily = serde_json::from_value(f["family"].clone()).unwrap_or_else(|_| panic!("{} has no valid `family`", f["name"]));
            assert!(ModelFamily::ALL.contains(&family));
        }
    }

    #[test]
    fn no_name_is_the_default_edition_and_an_unknown_name_includes_nothing() {
        assert_eq!(resolve(M, None, None).name, "default");
        assert_eq!(resolve(M, None, None).include.len(), 7);
        assert!(resolve(M, Some("no-such-edition"), None).include.is_empty());
        assert!(resolve("not json", None, None).include.is_empty());
    }

    #[test]
    fn a_named_edition_leaves_out_what_it_does_not_list() {
        let e = resolve(M, Some("test-no-lama"), None);
        assert!(!e.includes(Inpaint));
        assert_eq!(e.excluded(), vec![Inpaint]);
        assert!(e.includes(Denoise) && e.includes(Face));
    }

    #[test]
    fn the_bundled_file_can_only_narrow_the_compiled_edition() {
        let no_lama = r#"{"edition":"test-no-lama","include":["denoise","segment","detect","upscale","blink","face"]}"#;
        let only_denoise = r#"{"include":["denoise"]}"#;
        // A default binary with a no-LaMa bundle: narrowed.
        assert_eq!(resolve(M, None, Some(no_lama)).excluded(), vec![Inpaint]);
        // A no-LaMa binary with a bundle that claims everything: still narrowed (never widened).
        let all = r#"{"include":["denoise","segment","detect","inpaint","upscale","blink","face"]}"#;
        assert_eq!(resolve(M, Some("test-no-lama"), Some(all)).excluded(), vec![Inpaint]);
        // Both narrow.
        assert_eq!(fams(&resolve(M, Some("test-no-lama"), Some(only_denoise))), vec![Denoise]);
        // A bundle file that cannot be read includes nothing.
        assert!(resolve(M, None, Some("{")).include.is_empty());
        assert!(resolve(M, None, Some("")).include.is_empty());
    }

    /// The build's own edition: with `EMULSION_EDITION` unset it is the default (every family); the verification plan
    /// (ADR-0012) runs this with `EMULSION_EDITION=test-no-lama` to see that the compile-time name reaches `current()`.
    #[test]
    fn the_running_edition_follows_the_compiled_name() {
        match compiled_name() {
            None => assert!(current().excluded().is_empty() && current().name == "default"),
            Some(name) => assert_eq!(current().name, name),
        }
    }

    #[test]
    fn the_refusal_is_one_sentence_naming_the_feature() {
        assert_eq!(refusal(Inpaint), "Remove is not included in this edition of Emulsion.");
        for f in ModelFamily::ALL {
            let s = refusal(f);
            assert!(s.ends_with("is not included in this edition of Emulsion.") && !s.contains('\n'), "{s}");
        }
    }

    #[test]
    fn the_family_serialises_as_snake_case_for_the_frontend() {
        assert_eq!(serde_json::to_string(&Inpaint).unwrap(), "\"inpaint\"");
        let info = EditionInfo {
            name: "x".into(),
            excluded: vec![Inpaint, Face],
            channel: crate::channel::Channel::MacAppStore,
            caps: crate::channel::Channel::MacAppStore.caps(),
            compiled_out: vec![CompiledOut::LensProfiles],
        };
        assert_eq!(
            serde_json::to_string(&info).unwrap(),
            r#"{"name":"x","excluded":["inpaint","face"],"channel":"mac_app_store","caps":{"export_command_hook":false,"model_downloads":false,"custom_cache_dir":false},"compiled_out":["lens_profiles"]}"#
        );
    }

    #[test]
    fn the_store_edition_includes_exactly_denoise_and_segment() {
        let e = resolve(MANIFEST, Some("store"), None);
        assert_eq!(fams(&e), vec![Denoise, Segment]);
        assert_eq!(e.excluded(), vec![Detect, Inpaint, Upscale, Blink, Face]);
    }

    #[test]
    fn a_channel_mas_build_resolves_to_the_store_edition() {
        if cfg!(feature = "channel-mas") {
            assert_eq!(compiled_name(), Some("store"));
            assert_eq!(current().name, "store");
        } else {
            assert_ne!(compiled_name(), Some("store"), "only the Mac App Store build is fixed to `store`");
        }
    }

    #[test]
    fn compiled_out_lists_lens_profiles_exactly_when_the_feature_is_off() {
        let json = serde_json::to_string(&compiled_out()).unwrap();
        if cfg!(feature = "lens-profiles") {
            assert_eq!(json, "[]");
        } else {
            assert_eq!(json, r#"["lens_profiles"]"#);
        }
    }
}
