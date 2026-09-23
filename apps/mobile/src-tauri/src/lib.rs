#[cfg(target_os = "android")]
mod android;

pub mod build_info {
    pub const GIT_HASH: &str = env!("GIT_HASH");
    pub const GIT_DIRTY: &str = env!("GIT_DIRTY");
    pub const BUILD_TIMESTAMP: &str = env!("BUILD_TIMESTAMP");
    pub const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

    pub fn fingerprint() -> serde_json::Value {
        serde_json::json!({
            "git_hash": GIT_HASH,
            "git_dirty": GIT_DIRTY == "true",
            "build_timestamp": BUILD_TIMESTAMP.parse::<u64>().unwrap_or(0),
            "version": CARGO_PKG_VERSION,
        })
    }
}

#[cfg(not(target_os = "android"))]
use app_runtime::Runtime;
#[cfg(not(target_os = "android"))]
use tauri::{Manager, State};

#[tauri::command]
fn build_info() -> serde_json::Value {
    build_info::fingerprint()
}

#[tauri::command]
#[cfg(not(target_os = "android"))]
async fn command(
    state: State<'_, Runtime>,
    name: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let name = match name.as_str() {
        "authorize_connection" => "start_mcp_oauth",
        "cancel_authorization" => "cancel_mcp_oauth",
        name => name,
    };
    state
        .dispatch(name, payload)
        .await
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "android")]
    android::run();
    #[cfg(not(target_os = "android"))]
    tauri::Builder::default()
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            app.manage(Runtime::open(directory.join("assistant.db"))?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![command, build_info])
        .run(tauri::generate_context!())
        .expect("Application startup failed");
}
