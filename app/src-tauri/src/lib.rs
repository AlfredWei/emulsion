mod catalog;
mod raw_decode;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// M0 spike only (docs/adr/ADR-0004, PROGRESS.md): the in-webview WebGPU
/// test page at /m0-spike can't be screenshotted by the tooling used to
/// build this app, so it reports its result here and this prints it to
/// the Rust process's own stdout, where it can be captured directly.
#[tauri::command]
fn report_spike_result(result_json: String) {
    println!("M0_SPIKE_RESULT: {result_json}");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet, report_spike_result])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
