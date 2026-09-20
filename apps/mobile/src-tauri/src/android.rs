//! Thin Android assembly. Kotlin owns accessibility; Rust owns model and policy state.
use assistant_contracts::*;
use assistant_core::sms::Experiment;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tauri::{
    plugin::{Builder, PluginHandle},
    Manager, State, Wry,
};

struct AndroidTools {
    handle: PluginHandle<Wry>,
    epoch: Mutex<(u64, bool)>,
}
impl AndroidTools {
    async fn call(&self, operation: &str, arguments: Value) -> Result<Value> {
        let epoch = {
            let mut epoch = self.epoch.lock().unwrap();
            if operation == "arm" || operation == "stop" {
                epoch.0 += 1;
                epoch.1 = operation == "arm";
            }
            if !matches!(operation, "arm" | "stop" | "settings" | "status") && !epoch.1 {
                return Err(Error::Denied);
            }
            epoch.0
        };
        let handle = self.handle.clone();
        let payload =
            json!({"operation":operation,"epoch":epoch,"arguments":arguments.to_string()});
        tauri::async_runtime::spawn_blocking(move || handle.run_mobile_plugin("execute", payload))
            .await
            .map_err(|_| Error::Unavailable)?
            .map_err(|_| Error::Denied)
    }
}
#[async_trait::async_trait]
impl ToolExecutor for AndroidTools {
    async fn execute(&self, spec: &ToolSpec, call: &ToolCall) -> Result<Value> {
        assistant_core::registry::validate_call(spec, call)?;
        self.call(&spec.source_tool, call.arguments.clone()).await
    }
}
struct AndroidRuntime {
    experiment: Arc<Experiment>,
    tools: Arc<AndroidTools>,
}

#[tauri::command]
async fn command(
    state: State<'_, AndroidRuntime>,
    name: String,
    payload: Value,
) -> std::result::Result<Value, String> {
    dispatch(&state, &name, payload)
        .await
        .map_err(|e| e.to_string())
}
async fn dispatch(state: &AndroidRuntime, name: &str, payload: Value) -> Result<Value> {
    let string = |key: &str| {
        payload[key]
            .as_str()
            .map(str::to_owned)
            .ok_or(Error::InvalidInput)
    };
    match name {
        "sms_snapshot" => {
            let device = state.tools.call("status", json!({})).await?;
            if state.experiment.snapshot().status == "running" && device["stopped"] == true {
                state.experiment.stop();
            }
            Ok(json!({"session":state.experiment.snapshot(),"device":device}))
        }
        "sms_settings" => state.tools.call("settings", json!({})).await,
        "sms_stop" => {
            state.experiment.stop();
            state.tools.call("stop", json!({})).await
        }
        "sms_draft" => {
            state.tools.call("stop", json!({})).await?;
            state
                .experiment
                .draft(string("recipient")?, string("instruction")?)
                .await?;
            Ok(json!(null))
        }
        "sms_approve" => {
            let id = Id::parse_str(&string("id")?).map_err(|_| Error::InvalidInput)?;
            assistant_core::sms::PolicyEngine::approve(
                &state.experiment.snapshot(),
                id,
                &string("recipient")?,
                &string("message")?,
            )?;
            state.tools.call("arm", json!({})).await?;
            let result = state
                .experiment
                .approve(id, string("recipient")?, string("message")?)
                .await;
            let _ = state.tools.call("stop", json!({})).await;
            result.map(|_| json!(null))
        }
        _ => Err(Error::Denied),
    }
}
pub fn run() {
    tauri::Builder::default()
        .plugin(
            Builder::<Wry, ()>::new("sms-native")
                .setup(|app, api| {
                    let handle = api.register_android_plugin("dev.local.assistant", "SmsPlugin")?;
                    let tools = Arc::new(AndroidTools {
                        handle,
                        epoch: Mutex::new((0, false)),
                    });
                    let directory = app.path().app_data_dir()?;
                    std::fs::create_dir_all(&directory)?;
                    let store = Arc::new(storage_sqlite::SqliteStore::open(
                        directory.join("sms-experiment.db"),
                    )?);
                    let experiment = Arc::new(Experiment::new(
                        Arc::new(provider_needle::NeedleProvider::linked()),
                        tools.clone(),
                        store,
                    ));
                    app.manage(AndroidRuntime { experiment, tools });
                    Ok(())
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![command])
        .run(tauri::generate_context!())
        .expect("Android application startup failed");
}
