//! Application assembly shared by Tauri and the local development host.
use adapter_mcp::{AuthorizationCallback, ConnectionConfig, McpAuthentication, McpManager};
use assistant_contracts::*;
use assistant_core::{registry, Assistant};
use provider_cloud::{valid_secret_reference, CloudConfig, CloudProvider};
use provider_needle::NeedleProvider;
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};
use storage_sqlite::SqliteStore;
use tokio::sync::RwLock;
mod companion_commands;
mod conversations;
mod credentials;
mod local_models;
mod oauth_runtime;
use credentials::SessionSecrets;
use oauth_runtime::OAuthRuntime;

#[cfg(test)]
mod conversation_tests;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    pub cloud: Option<CloudConfig>,
    pub engine: EngineConfig,
    pub connections: Vec<ConnectionConfig>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveCloudProvider {
    cloud: CloudConfig,
    api_key: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveMcpCredential {
    connection_id: String,
    secret: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectMcpWithCredential {
    connection: ConnectionConfig,
    secret: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartMcpOAuth {
    connection_id: String,
    client_id: Option<String>,
    client_metadata_url: Option<String>,
    redirect_uri: String,
    resume_task_id: Option<Id>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompleteMcpOAuth {
    transaction_id: Id,
    callback_url: String,
}

pub struct Runtime {
    pub store: Arc<SqliteStore>,
    pub assistant: RwLock<Arc<Assistant>>,
    pub mcp: Arc<McpManager>,
    needle: Arc<NeedleProvider>,
    settings: RwLock<Settings>,
    secrets: Arc<SessionSecrets>,
    oauth: Arc<OAuthRuntime>,
    conversations: Arc<conversations::Conversations>,
    local_models: local_models::LocalModels,
    companion_gate: tokio::sync::Mutex<()>,
}
impl Runtime {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let model_root = path
            .as_ref()
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("models");
        let local_models = local_models::LocalModels::new(model_root);
        let store = Arc::new(SqliteStore::open(path)?);
        let mut settings: Settings = store
            .setting("settings")?
            .map(|v| serde_json::from_value(v).map_err(|_| Error::Storage))
            .transpose()?
            .unwrap_or_default();
        // Older builds accepted API keys in this non-secret field. Never send those back to UI.
        if repair_credential_reference(&mut settings) {
            store.set_setting(
                "settings",
                &serde_json::to_value(&settings).map_err(|_| Error::Storage)?,
            )?;
        }
        let secrets = Arc::new(SessionSecrets::default());
        let oauth = Arc::new(OAuthRuntime::new(secrets.clone())?);
        let mcp = Arc::new(McpManager::new(oauth.clone()));
        let needle = Arc::new(
            std::env::var_os("ASSISTANT_NEEDLE_LIBRARY")
                .and_then(|path| NeedleProvider::verified_windows(Path::new(&path)).ok())
                .unwrap_or_else(NeedleProvider::unavailable),
        );
        let assistant = assemble(
            store.clone(),
            mcp.clone(),
            needle.clone(),
            &settings,
            &secrets,
        )?;
        let conversations =
            conversations::Conversations::new(store.clone(), local_models.provider.clone());
        Ok(Self {
            store,
            assistant: RwLock::new(assistant),
            mcp,
            needle,
            settings: RwLock::new(settings),
            secrets,
            oauth,
            conversations,
            local_models,
            companion_gate: tokio::sync::Mutex::new(()),
        })
    }
    pub async fn snapshot(&self) -> Result<serde_json::Value> {
        let settings = self.settings.read().await.clone();
        let cloud_credential = match &settings.cloud {
            None => "not_configured",
            Some(config) => {
                if self
                    .secrets
                    .provider(config)
                    .get(&config.secret_ref)
                    .is_ok()
                {
                    "configured"
                } else {
                    "missing"
                }
            }
        };
        let cloud_session_key = settings
            .cloud
            .as_ref()
            .is_some_and(|cloud| self.secrets.contains(cloud));
        Ok(
            serde_json::json!({"tasks":self.store.tasks()?,"capabilities":self.store.capabilities()?,"settings":settings,"cloud_credential":cloud_credential,"cloud_session_key":cloud_session_key,"needle":if self.needle.is_available(){"ready"}else{"not_linked"},"voice":"deferred"}),
        )
    }
    pub async fn configure(&self, settings: Settings) -> Result<()> {
        if settings
            .cloud
            .as_ref()
            .is_some_and(|cloud| !valid_secret_reference(&cloud.secret_ref))
        {
            return Err(Error::InvalidInput);
        }
        if self
            .store
            .tasks()?
            .iter()
            .any(|t| matches!(t.status, TaskStatus::Running | TaskStatus::Created))
        {
            return Err(Error::Conflict);
        }
        let next = assemble(
            self.store.clone(),
            self.mcp.clone(),
            self.needle.clone(),
            &settings,
            &self.secrets,
        )?;
        self.store.set_setting(
            "settings",
            &serde_json::to_value(&settings).map_err(|_| Error::InvalidInput)?,
        )?;
        *self.assistant.write().await = next;
        *self.settings.write().await = settings;
        Ok(())
    }
    pub async fn connect(&self, config: ConnectionConfig) -> Result<()> {
        self.save_connection(config.clone()).await?;
        self.oauth.ensure_fresh(&config).await?;
        let tools = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.mcp.connect(&config),
        )
        .await
        .map_err(|_| Error::Timeout)??;
        let incoming: Vec<_> = tools.iter().map(|t| t.id.clone()).collect();
        for mut tool in tools {
            if let Ok(Capability::Tool(old)) = self.store.capability(&tool.id) {
                tool.enabled = old.enabled;
                tool.risk = old.risk;
                if old.input_schema != tool.input_schema
                    || old.output_schema != tool.output_schema
                    || old.description != tool.description
                {
                    tool.version = old
                        .version
                        .parse::<u64>()
                        .unwrap_or(0)
                        .saturating_add(1)
                        .to_string();
                    tool.enabled = false;
                } else {
                    tool.version = old.version;
                }
            }
            registry::register(self.store.as_ref(), &Capability::Tool(tool))?;
        }
        for entry in self.store.capabilities()? {
            if let Capability::Tool(mut tool) = entry {
                if tool.connection_id == config.id && !incoming.contains(&tool.id) {
                    tool.enabled = false;
                    self.store.put_capability(&Capability::Tool(tool))?;
                }
            }
        }
        Ok(())
    }

    async fn save_connection(&self, config: ConnectionConfig) -> Result<()> {
        config.validate()?;
        {
            let mut settings = self.settings.write().await;
            settings
                .connections
                .retain(|connection| connection.id != config.id);
            settings.connections.push(config.clone());
            self.store.set_setting(
                "settings",
                &serde_json::to_value(&*settings).map_err(|_| Error::Storage)?,
            )?;
        }
        Ok(())
    }
    pub async fn dispatch(
        &self,
        name: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let assistant = self.assistant.read().await.clone();
        let task_id = || {
            payload["task_id"]
                .as_str()
                .and_then(|v| Id::parse_str(v).ok())
                .ok_or(Error::InvalidInput)
        };
        match name {
            "snapshot" => self.snapshot().await,
            "submit_input" => {
                self.reject_known_secrets(&payload).await?;
                let input: UserInput =
                    serde_json::from_value(payload).map_err(|_| Error::InvalidInput)?;
                let task = assistant.submit(input)?;
                launch(assistant, task.id);
                Ok(serde_json::json!(task))
            }
            "run_task" => {
                launch(assistant, task_id()?);
                Ok(serde_json::json!(null))
            }
            "cancel_task" => Ok(serde_json::json!(assistant.cancel(task_id()?)?)),
            "resume_auth" => {
                let task = assistant.resume_auth(task_id()?)?;
                launch(assistant, task.id);
                Ok(serde_json::json!(task))
            }
            "resolve_approval" => {
                let approval = payload["approval_id"]
                    .as_str()
                    .and_then(|s| Id::parse_str(s).ok())
                    .ok_or(Error::InvalidInput)?;
                let task = assistant.approve(
                    task_id()?,
                    approval,
                    payload["approved"].as_bool().ok_or(Error::InvalidInput)?,
                )?;
                if task.status == TaskStatus::Created {
                    launch(assistant, task.id);
                }
                Ok(serde_json::json!(task))
            }
            "answer_question" => {
                self.reject_known_secrets(&payload).await?;
                let task = assistant.answer(
                    task_id()?,
                    payload["answer"].as_str().ok_or(Error::InvalidInput)?,
                )?;
                launch(assistant, task.id);
                Ok(serde_json::json!(task))
            }
            "save_settings" => {
                self.configure(serde_json::from_value(payload).map_err(|_| Error::InvalidInput)?)
                    .await?;
                Ok(serde_json::json!(null))
            }
            "save_cloud_provider" => {
                let request: SaveCloudProvider =
                    serde_json::from_value(payload).map_err(|_| Error::InvalidInput)?;
                if let Some(key) = &request.api_key {
                    SessionSecrets::validate(key)?;
                }
                let mut settings = self.settings.read().await.clone();
                settings.cloud = Some(request.cloud.clone());
                self.configure(settings).await?;
                if let Some(key) = request.api_key {
                    self.secrets.set(&request.cloud, key)?;
                }
                Ok(serde_json::json!(null))
            }
            "clear_cloud_key" => {
                self.secrets.clear()?;
                Ok(serde_json::json!(null))
            }
            "save_mcp_credential" => {
                let request: SaveMcpCredential =
                    serde_json::from_value(payload).map_err(|_| Error::InvalidInput)?;
                SessionSecrets::validate(&request.secret)?;
                let config = self
                    .settings
                    .read()
                    .await
                    .connections
                    .iter()
                    .find(|connection| connection.id == request.connection_id)
                    .cloned()
                    .ok_or(Error::InvalidInput)?;
                self.secrets.set_mcp(&config, request.secret)?;
                Ok(serde_json::json!(null))
            }
            "clear_mcp_credential" => {
                let connection_id = payload["connection_id"]
                    .as_str()
                    .ok_or(Error::InvalidInput)?;
                let settings = self.settings.read().await;
                let config = settings
                    .connections
                    .iter()
                    .find(|connection| connection.id == connection_id)
                    .ok_or(Error::InvalidInput)?;
                if matches!(
                    config.authentication,
                    McpAuthentication::OauthAuthorizationCode { .. }
                ) {
                    self.oauth.clear(connection_id)?;
                } else {
                    self.secrets.clear_mcp(connection_id)?;
                }
                Ok(serde_json::json!(null))
            }
            "mcp_connection_status" => {
                let connection_id = payload["connection_id"]
                    .as_str()
                    .ok_or(Error::InvalidInput)?;
                let settings = self.settings.read().await;
                let config = settings
                    .connections
                    .iter()
                    .find(|connection| connection.id == connection_id)
                    .ok_or(Error::InvalidInput)?;
                if matches!(
                    config.authentication,
                    McpAuthentication::OauthAuthorizationCode { .. }
                ) {
                    return serde_json::to_value(self.oauth.status(config)?)
                        .map_err(|_| Error::InvalidResponse);
                }
                let credential = if !config.authentication.requires_auth() {
                    "not_required"
                } else if self.secrets.contains_mcp(config) {
                    "configured"
                } else {
                    "missing"
                };
                Ok(serde_json::json!({
                    "connection_id": config.id,
                    "authentication": config.authentication,
                    "credential": credential
                }))
            }
            "start_mcp_oauth" => {
                let request: StartMcpOAuth =
                    serde_json::from_value(payload).map_err(|_| Error::InvalidInput)?;
                let config = self
                    .settings
                    .read()
                    .await
                    .connections
                    .iter()
                    .find(|connection| connection.id == request.connection_id)
                    .cloned()
                    .ok_or(Error::InvalidInput)?;
                if let Some(task_id) = request.resume_task_id {
                    let task = self.store.task(task_id)?;
                    if task.status != TaskStatus::WaitingForAuth {
                        return Err(Error::Conflict);
                    }
                }
                let start = self
                    .oauth
                    .begin(
                        &config,
                        request.client_id.as_deref(),
                        request.client_metadata_url.as_deref(),
                        &request.redirect_uri,
                        request.resume_task_id,
                    )
                    .await?;
                serde_json::to_value(start).map_err(|_| Error::InvalidResponse)
            }
            "complete_mcp_oauth" => {
                let request: CompleteMcpOAuth =
                    serde_json::from_value(payload).map_err(|_| Error::InvalidInput)?;
                let connection_id = self.oauth.pending_connection(request.transaction_id)?;
                let config = self
                    .settings
                    .read()
                    .await
                    .connections
                    .iter()
                    .find(|connection| connection.id == connection_id)
                    .cloned()
                    .ok_or(Error::Denied)?;
                let resume_task_id = self
                    .oauth
                    .complete(
                        request.transaction_id,
                        AuthorizationCallback {
                            url: request.callback_url,
                        },
                        &config,
                    )
                    .await?;
                self.connect(config.clone()).await?;
                let task = if let Some(task_id) = resume_task_id {
                    let task = assistant.resume_auth(task_id)?;
                    launch(assistant, task.id);
                    Some(task)
                } else {
                    None
                };
                Ok(serde_json::json!({
                    "connection": self.oauth.status(&config)?,
                    "resumed_task": task
                }))
            }
            "cancel_mcp_oauth" => {
                let transaction_id = payload["transaction_id"]
                    .as_str()
                    .and_then(|value| Id::parse_str(value).ok())
                    .ok_or(Error::InvalidInput)?;
                self.oauth.cancel(transaction_id)?;
                Ok(serde_json::json!(null))
            }
            "save_capability" => {
                let capability: Capability =
                    serde_json::from_value(payload).map_err(|_| Error::InvalidInput)?;
                registry::register(self.store.as_ref(), &capability)?;
                Ok(serde_json::json!(null))
            }
            "connect_mcp" => {
                self.connect(serde_json::from_value(payload).map_err(|_| Error::InvalidInput)?)
                    .await?;
                Ok(serde_json::json!(null))
            }
            "save_mcp_connection" => {
                self.save_connection(
                    serde_json::from_value(payload).map_err(|_| Error::InvalidInput)?,
                )
                .await?;
                Ok(serde_json::json!(null))
            }
            "connect_mcp_with_credential" => {
                let request: ConnectMcpWithCredential =
                    serde_json::from_value(payload).map_err(|_| Error::InvalidInput)?;
                request.connection.validate()?;
                SessionSecrets::validate(&request.secret)?;
                self.secrets.set_mcp(&request.connection, request.secret)?;
                self.connect(request.connection).await?;
                Ok(serde_json::json!(null))
            }
            "connect_parallel_search" => {
                self.connect(ConnectionConfig::parallel_search()).await?;
                Ok(serde_json::json!(null))
            }
            "disconnect_mcp" => {
                let id = payload["id"].as_str().ok_or(Error::InvalidInput)?;
                self.mcp.disconnect(id).await;
                self.oauth.clear(id)?;
                for entry in self.store.capabilities()? {
                    if let Capability::Tool(mut tool) = entry {
                        if tool.connection_id == id {
                            tool.enabled = false;
                            self.store.put_capability(&Capability::Tool(tool))?;
                        }
                    }
                }
                Ok(serde_json::json!(null))
            }
            "events" => Ok(serde_json::json!(self
                .store
                .events(payload["after"].as_u64().unwrap_or(0))?)),
            "run_events" => {
                let after = payload["after"].as_u64().unwrap_or(0);
                let bounds = self.store.event_bounds()?;
                let reset_required = match bounds {
                    Some(bounds) => {
                        after > bounds.newest
                            || (after > 0 && after.saturating_add(1) < bounds.oldest)
                    }
                    None => after > 0,
                };
                let effective_after = if reset_required { 0 } else { after };
                let events = self.store.events(effective_after)?;
                let next_after = events
                    .last()
                    .map_or(effective_after, |event| event.sequence);
                let has_more = bounds.is_some_and(|bounds| next_after < bounds.newest);
                Ok(serde_json::json!(RunEventPage {
                    schema: RUN_EVENT_PAGE_SCHEMA_V1.into(),
                    after,
                    next_after,
                    has_more,
                    reset_required,
                    events,
                }))
            }
            "model_status"
            | "install_model"
            | "cancel_model_download"
            | "remove_model"
            | "unload_model" => {
                let _gate = self.companion_gate.lock().await;
                if name == "unload_model" {
                    self.conversations.cancel_all()?;
                }
                self.local_models
                    .command(name, self.conversations.busy())
                    .await
            }
            _ => {
                let _gate = self.companion_gate.lock().await;
                if name == "send_message" && self.local_models.downloading() {
                    return Err(Error::Conflict);
                }
                self.reject_known_secrets(&payload).await?;
                self.companion_command(name, payload)
            }
        }
    }

    async fn reject_known_secrets(&self, payload: &serde_json::Value) -> Result<()> {
        if self.secrets.payload_contains_secret(payload)
            || self.oauth.payload_contains_secret(payload)
        {
            return Err(Error::Denied);
        }
        let settings = self.settings.read().await;
        if let Some(config) = &settings.cloud {
            if let Ok(secret) = self.secrets.provider(config).get(&config.secret_ref) {
                if !secret.is_empty() && contains_secret(payload, &secret) {
                    return Err(Error::Denied);
                }
            }
        }
        Ok(())
    }
}
fn contains_secret(value: &serde_json::Value, secret: &str) -> bool {
    match value {
        serde_json::Value::String(value) => value.contains(secret),
        serde_json::Value::Array(values) => {
            values.iter().any(|value| contains_secret(value, secret))
        }
        serde_json::Value::Object(values) => {
            values.values().any(|value| contains_secret(value, secret))
        }
        _ => false,
    }
}
fn repair_credential_reference(settings: &mut Settings) -> bool {
    if let Some(cloud) = &mut settings.cloud {
        if !valid_secret_reference(&cloud.secret_ref) {
            cloud.secret_ref = "ASSISTANT_CLOUD_KEY".into();
            return true;
        }
    }
    false
}
fn launch(assistant: Arc<Assistant>, id: Id) {
    tokio::spawn(async move {
        if let Err(error) = assistant.run(id).await {
            if let Ok(mut task) = assistant.store.task(id) {
                if !task.status.terminal() && task.status != TaskStatus::WaitingForResolution {
                    task.status = if task.pending.as_ref().is_some_and(|p| p.started) {
                        TaskStatus::WaitingForResolution
                    } else {
                        TaskStatus::Failed
                    };
                    task.message = error.to_string();
                    let _ = assistant.store.save_task(&task);
                }
            }
        }
    });
}
fn assemble(
    store: Arc<SqliteStore>,
    mcp: Arc<McpManager>,
    needle: Arc<NeedleProvider>,
    settings: &Settings,
    secrets: &Arc<SessionSecrets>,
) -> Result<Arc<Assistant>> {
    let config = &settings.engine;
    if config.max_steps == 0
        || config.max_steps > 100
        || config.tool_limit > 8
        || config.tool_limit == 0
        || config.candidate_limit > 20
        || config.max_failures == 0
        || config.timeout_seconds == 0
        || config.timeout_seconds > 120
        || config.fast_context_bytes > 64_000
        || config.cloud_context_bytes > 256_000
    {
        return Err(Error::InvalidInput);
    }
    let cloud: Arc<dyn ModelProvider> = match &settings.cloud {
        Some(config) => Arc::new(CloudProvider::new(
            config.clone(),
            secrets.provider(config),
        )?),
        None => Arc::new(NeedleProvider::unavailable()),
    };
    Ok(Arc::new(Assistant::new(
        store,
        needle,
        cloud,
        mcp,
        settings.engine.clone(),
    )))
}
