#[cfg(target_os = "android")]
mod android;
#[cfg(not(target_os = "android"))]
use app_runtime::Runtime;
#[cfg(not(target_os = "android"))]
use tauri::{Manager, State};

#[tauri::command]
#[cfg(not(target_os = "android"))]
async fn command(
    state: State<'_, Runtime>,
    name: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    state
        .dispatch(&name, payload)
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
        .invoke_handler(tauri::generate_handler![command])
        .run(tauri::generate_context!())
        .expect("Application startup failed");
}
