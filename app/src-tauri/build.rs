/// One `cfg` per capability of `channel::ChannelCaps`, set when the channel being built has it, so code that a channel
/// must not contain is compiled out rather than switched off. `channel::tests` checks that this table and
/// `Channel::caps` agree.
const CAPS: [&str; 3] = ["export_command_hook", "model_downloads", "custom_cache_dir"];

const MANIFEST: &str = "../../models/bundled-models.json";

fn main() {
    let mac_app_store = std::env::var_os("CARGO_FEATURE_CHANNEL_MAS").is_some();
    for cap in CAPS {
        println!("cargo:rustc-check-cfg=cfg({cap})");
        if !mac_app_store {
            println!("cargo:rustc-cfg={cap}");
        }
    }
    write_editions();
    tauri_build::build()
}

/// `editions.rs` embeds only the `editions` object of the model manifest, so no build carries the download URLs of
/// its `files` list.
fn write_editions() {
    println!("cargo:rerun-if-changed={MANIFEST}");
    let text = std::fs::read_to_string(MANIFEST).expect("read the model manifest");
    let manifest: serde_json::Value = serde_json::from_str(&text).expect("parse the model manifest");
    let editions = serde_json::json!({ "editions": manifest["editions"] });
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("editions.json");
    std::fs::write(out, editions.to_string()).expect("write editions.json");
}
