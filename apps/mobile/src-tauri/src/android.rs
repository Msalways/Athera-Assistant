//! Thin Android assembly. Kotlin owns accessibility; Rust owns model and policy state.
use app_runtime::{OAuthTokenVault, Runtime};
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
    auth: Arc<AndroidAuth>,
    epoch: Mutex<(u64, bool)>,
}

struct AndroidAuth {
    handle: PluginHandle<Wry>,
}

impl AndroidAuth {
    fn call_native(&self, operation: &str, arguments: Value) -> Result<Value> {
        let payload = json!({"operation":operation,"arguments":arguments.to_string()});
        self.handle
            .run_mobile_plugin("execute", payload)
            .map_err(|_| Error::Unavailable)
    }

    async fn call(&self, operation: &str, arguments: Value) -> Result<Value> {
        let auth = Self {
            handle: self.handle.clone(),
        };
        let operation = operation.to_owned();
        tauri::async_runtime::spawn_blocking(move || auth.call_native(&operation, arguments))
            .await
            .map_err(|_| Error::Unavailable)?
    }
}

impl OAuthTokenVault for AndroidAuth {
    fn store(&self, handle: &str, binding: &str, value: &str) -> Result<()> {
        self.call_native(
            "store_secret",
            json!({"handle":handle,"binding":binding,"value":value}),
        )?;
        Ok(())
    }

    fn load(&self, handle: &str, binding: &str) -> Result<Option<String>> {
        let response = self.call_native(
            "load_secret",
            json!({"handle":handle,"binding":binding}),
        )?;
        match response.get("value") {
            Some(Value::String(value)) => Ok(Some(value.clone())),
            Some(Value::Null) => Ok(None),
            _ => Err(Error::InvalidResponse),
        }
    }

    fn delete(&self, handle: &str) -> Result<()> {
        self.call_native("delete_secret", json!({"handle":handle}))?;
        Ok(())
    }
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
        if spec.source_tool == "open_external" {
            let url = call.arguments["url"].as_str().ok_or(Error::InvalidInput)?;
            if url.len() > 4096 {
                return Err(Error::InvalidInput);
            }
            return self.auth.call("open_external", json!({"url": url})).await;
        }
        self.call(&spec.source_tool, call.arguments.clone()).await
    }
}
struct AndroidRuntime {
    runtime: Runtime,
    experiment: Arc<Experiment>,
    tools: Arc<AndroidTools>,
    auth: Arc<AndroidAuth>,
}

fn cloud_vault_handle(id: &str) -> String {
    format!("cloud-key:{id}")
}

fn cloud_vault_binding(cloud: &Value) -> Result<String> {
    let endpoint = cloud["endpoint"].as_str().ok_or(Error::InvalidInput)?;
    let reference = cloud["secret_ref"].as_str().ok_or(Error::InvalidInput)?;
    Ok(format!("{endpoint}|{reference}"))
}

fn cloud_vault_handle_from(cloud: &Value) -> Result<String> {
    let id = cloud["id"].as_str().ok_or(Error::InvalidInput)?;
    if id.is_empty() || id.len() > 128 {
        return Err(Error::InvalidInput);
    }
    Ok(cloud_vault_handle(id))
}

fn provider_vault_handle(provider_id: &str) -> Result<String> {
    if provider_id.is_empty() || provider_id.len() > 128 {
        return Err(Error::InvalidInput);
    }
    Ok(format!("provider-key:{provider_id}"))
}

fn provider_vault_binding(provider_id: &str, auth_option_id: &str) -> Result<String> {
    if auth_option_id.is_empty() || auth_option_id.len() > 128 {
        return Err(Error::InvalidInput);
    }
    Ok(format!("{provider_id}|{auth_option_id}"))
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
        "save_cloud_provider" => {
            let result = state.runtime.dispatch(name, payload.clone()).await?;
            if let Some(key) = payload["api_key"].as_str() {
                state.auth.store(
                    &cloud_vault_handle_from(&payload["cloud"] )?,
                    &cloud_vault_binding(&payload["cloud"] )?,
                    key,
                )?;
            }
            Ok(result)
        }
        "clear_cloud_key" => {
            let snapshot = state.runtime.snapshot().await?;
            if let Some(cloud) = snapshot["settings"]["cloud"].as_object() {
                let cloud = Value::Object(cloud.clone());
                state.auth.delete(
                    &cloud_vault_handle_from(&cloud)?,
                )?;
            }
            state.runtime.dispatch(name, payload).await
        }
        "save_provider_profile" => {
            let result = state.runtime.dispatch(name, payload.clone()).await?;
            if let Some(secret) = payload["secret"].as_str() {
                let provider_id = string("provider_id")?;
                let auth_option_id = payload["auth_option_id"]
                    .as_str()
                    .ok_or(Error::InvalidInput)?;
                state.auth.store(
                    &provider_vault_handle(&provider_id)?,
                    &provider_vault_binding(&provider_id, auth_option_id)?,
                    secret,
                )?;
            }
            Ok(result)
        }
        "delete_provider_profile" => {
            if let Some(provider_id) = payload["provider_id"].as_str() {
                state.auth.delete(&provider_vault_handle(provider_id)?)?;
            }
            state.runtime.dispatch(name, payload).await
        }
        "authorize_connection" => {
            let redirect_uri = string("redirect_uri")?;
            let mut start = state.runtime.dispatch("start_mcp_oauth", payload).await?;
            let authorization_url = start["authorization_url"]
                .as_str()
                .ok_or(Error::InvalidResponse)?
                .to_owned();
            state
                .auth
                .call(
                    "open",
                    json!({
                        "authorization_url": authorization_url,
                        "redirect_uri": redirect_uri,
                    }),
                )
                .await?;
            start
                .as_object_mut()
                .ok_or(Error::InvalidResponse)?
                .remove("authorization_url");
            Ok(start)
        }
        "poll_authorization" => {
            let callback = state.auth.call("take_callback", json!({})).await?;
            let Some(callback_url) = callback["callback_url"].as_str() else {
                return Ok(json!({"state":"waiting_for_user_authorization"}));
            };
            let result = state
                .runtime
                .dispatch(
                    "complete_mcp_oauth",
                    json!({
                        "transaction_id": string("transaction_id")?,
                        "callback_url": callback_url,
                    }),
                )
                .await?;
            Ok(json!({"state":"connected","result":result}))
        }
        "cancel_authorization" => {
            state.auth.call("clear_callback", json!({})).await?;
            state.runtime.dispatch("cancel_mcp_oauth", payload).await
        }
        "open_external" => {
            let url = string("url")?;
            if url.len() > 4096 {
                return Err(Error::InvalidInput);
            }
            state.auth.call("open_external", json!({"url": url})).await
        }
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
        _ => state.runtime.dispatch(name, payload).await,
    }
}
pub fn run() {
    tauri::Builder::default()
        .plugin(
            Builder::<Wry, ()>::new("sms-native")
                .setup(|app, api| {
                    let handle = api.register_android_plugin("dev.local.assistant", "SmsPlugin")?;
                    let auth = Arc::new(AndroidAuth {
                        handle: api.register_android_plugin("dev.local.assistant", "AuthPlugin")?,
                    });
                    let tools = Arc::new(AndroidTools {
                        handle,
                        auth: auth.clone(),
                        epoch: Mutex::new((0, false)),
                    });
                    let directory = app.path().app_data_dir()?;
                    std::fs::create_dir_all(&directory)?;
                    let needle = Arc::new(provider_needle::NeedleProvider::linked());
                    let runtime = Runtime::open_with_oauth_vault_fast_provider_and_executor(
                        directory.join("assistant.db"),
                        auth.clone(),
                        needle.clone(),
                        tools.clone(),
                    )?;
                    if let Some(cloud) = runtime
                        .store
                        .setting("settings")?
                        .and_then(|value| serde_json::from_value::<app_runtime::Settings>(value).ok())
                        .and_then(|settings| settings.cloud)
                    {
                        let binding = format!("{}|{}", cloud.endpoint, cloud.secret_ref);
                        if let Ok(Some(key)) = auth.load(&cloud_vault_handle(&cloud.id), &binding) {
                            runtime.restore_cloud_key(&cloud, key)?;
                        }
                    }
                    for profile in runtime.store.provider_profiles()? {
                        let binding = provider_vault_binding(
                            &profile.provider_id,
                            &profile.auth_option_id,
                        )?;
                        if let Ok(Some(key)) =
                            auth.load(&provider_vault_handle(&profile.provider_id)?, &binding)
                        {
                            runtime.restore_provider_key(
                                &profile.provider_id,
                                &profile.auth_option_id,
                                key,
                            )?;
                        }
                    }
                    let store = Arc::new(storage_sqlite::SqliteStore::open(
                        directory.join("sms-experiment.db"),
                    )?);
                    assistant_core::registry::register(
                        runtime.store.as_ref(),
                        &Capability::Tool(ToolSpec {
                            id: "android.open_external".into(),
                            version: "1".into(),
                            name: "Open a mobile app or deep link".into(),
                            description: "Open a supported Android URL or app deep link for the user to continue an action.".into(),
                            input_schema: json!({"type":"object","required":["url"],"properties":{"url":{"type":"string","maxLength":4096}}}),
                            output_schema: Some(json!({"type":"object","properties":{"opened":{"type":"boolean"}}})),
                            connection_id: "android_device".into(),
                            source_tool: "open_external".into(),
                            risk: Risk::LocalSafeWrite,
                            enabled: true,
                            requires_auth: false,
                            requires_network: false,
                        }),
                    )?;
                    let experiment = Arc::new(Experiment::new(
                        needle,
                        tools.clone(),
                        store,
                    ));
                    app.manage(AndroidRuntime {
                        runtime,
                        experiment,
                        tools,
                        auth,
                    });
                    Ok(())
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![command, super::build_info])
        .run(tauri::generate_context!())
        .expect("Android application startup failed");
}
