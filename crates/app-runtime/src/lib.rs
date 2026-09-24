//! Application assembly shared by Tauri and the local development host.
use adapter_mcp::{AuthorizationCallback, ConnectionConfig, McpAuthentication, McpManager};
use assistant_contracts::provider::AuthKind;
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
mod personalization;
use credentials::SessionSecrets;
pub use oauth_runtime::OAuthTokenVault;
use oauth_runtime::{OAuthRuntime, OAuthState};

#[cfg(test)]
mod conversation_tests;
#[cfg(test)]
mod personalization_tests;
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
struct SaveProviderProfile {
    provider_id: String,
    auth_option_id: String,
    config: serde_json::Value,
    secret: Option<String>,
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
    fast: Arc<dyn ModelProvider>,
    tool_executor: Option<Arc<dyn ToolExecutor>>,
    settings: RwLock<Settings>,
    secrets: Arc<SessionSecrets>,
    oauth: Arc<OAuthRuntime>,
    conversations: Arc<conversations::Conversations>,
    local_models: local_models::LocalModels,
    companion_gate: tokio::sync::Mutex<()>,
}
impl Runtime {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_inner(path.as_ref(), None, None)
    }

    /// Opens the runtime with the device's fast local model.
    pub fn open_with_fast_provider(
        path: impl AsRef<Path>,
        fast: Arc<dyn ModelProvider>,
    ) -> Result<Self> {
        Self::open_inner(path.as_ref(), None, Some(fast))
    }

    pub fn open_with_oauth_vault(
        path: impl AsRef<Path>,
        vault: Arc<dyn OAuthTokenVault>,
    ) -> Result<Self> {
        Self::open_inner(path.as_ref(), Some(vault), None)
    }

    /// Opens the runtime with durable OAuth storage and the device's fast local model.
    /// The vendor-neutral contract lets platform shells link native models without
    /// making orchestration depend on their vendor.
    pub fn open_with_oauth_vault_and_fast_provider(
        path: impl AsRef<Path>,
        vault: Arc<dyn OAuthTokenVault>,
        fast: Arc<dyn ModelProvider>,
    ) -> Result<Self> {
        Self::open_inner(path.as_ref(), Some(vault), Some(fast))
    }

    /// Opens the runtime with a platform executor for device-native actions.
    pub fn open_with_oauth_vault_fast_provider_and_executor(
        path: impl AsRef<Path>,
        vault: Arc<dyn OAuthTokenVault>,
        fast: Arc<dyn ModelProvider>,
        executor: Arc<dyn ToolExecutor>,
    ) -> Result<Self> {
        Self::open_inner_with_executor(path.as_ref(), Some(vault), Some(fast), Some(executor))
    }

    fn open_inner(
        path: &Path,
        vault: Option<Arc<dyn OAuthTokenVault>>,
        injected_fast: Option<Arc<dyn ModelProvider>>,
    ) -> Result<Self> {
        Self::open_inner_with_executor(path, vault, injected_fast, None)
    }

    fn open_inner_with_executor(
        path: &Path,
        vault: Option<Arc<dyn OAuthTokenVault>>,
        injected_fast: Option<Arc<dyn ModelProvider>>,
        injected_executor: Option<Arc<dyn ToolExecutor>>,
    ) -> Result<Self> {
        let model_root = path
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
        let oauth = Arc::new(OAuthRuntime::with_vault(
            secrets.clone(),
            vault,
            &settings.connections,
        )?);
        let mcp = Arc::new(McpManager::new(oauth.clone()));
        let fast = injected_fast.unwrap_or_else(|| {
            Arc::new(
                std::env::var_os("ASSISTANT_NEEDLE_LIBRARY")
                    .and_then(|path| NeedleProvider::verified_windows(Path::new(&path)).ok())
                    .unwrap_or_else(NeedleProvider::unavailable),
            )
        });
        let assistant = assemble(
            store.clone(),
            mcp.clone(),
            fast.clone(),
            injected_executor.clone(),
            &settings,
            &secrets,
            &oauth,
        )?;
        let recovery = assistant.recover_unfinished_graphs()?;
        launch_recovery(assistant.clone(), recovery)?;
        personalization::kick_learning(assistant.clone());
        let conversations =
            conversations::Conversations::new(store.clone(), local_models.provider.clone());
        Ok(Self {
            store,
            assistant: RwLock::new(assistant),
            mcp,
            fast,
            tool_executor: injected_executor,
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
        let mut connections = Vec::with_capacity(settings.connections.len());
        for config in &settings.connections {
            connections.push(self.connection_state(config).await?);
        }
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
        let adaptive_rules = self.store.adaptive_rules(None, 100)?;
        let rule_proposals = self.store.rule_proposals(None, 100)?;
        let tasks = self.store.tasks()?;
        let suggestions = self.next_step_suggestions(&tasks);
        Ok(
            serde_json::json!({"tasks":tasks,"capabilities":self.store.capabilities()?,"settings":settings,"connections":connections,"adaptive_rules":adaptive_rules,"rule_proposals":rule_proposals,"cloud_credential":cloud_credential,"cloud_session_key":cloud_session_key,"needle":if self.fast.is_available(){"ready"}else{"not_linked"},"voice":"deferred","suggestions":suggestions}),
        )
    }

    /// Restores a platform-keystore credential into the in-memory provider scope.
    /// The value remains outside SQLite and model context.
    pub fn restore_cloud_key(&self, config: &CloudConfig, key: String) -> Result<()> {
        self.secrets.set(config, key)
    }

    /// Restores a platform-keystore catalog provider key into the session scope.
    /// The value remains outside SQLite and model context.
    pub fn restore_provider_key(
        &self,
        provider_id: &str,
        auth_option_id: &str,
        key: String,
    ) -> Result<()> {
        self.secrets
            .set_provider_key(provider_id, auth_option_id, key)
    }

    fn next_step_suggestions(&self, tasks: &[Task]) -> Vec<serde_json::Value> {
        let mut suggestions = Vec::new();
        for task in tasks.iter().filter(|task| !task.status.terminal()).take(3) {
            let suggestion = match task.status {
                TaskStatus::WaitingForApproval => Some((
                    "review_approval",
                    "approval",
                    "Review the pending action",
                    "This task is ready for your approval.",
                )),
                TaskStatus::WaitingForUser | TaskStatus::WaitingForResolution => Some((
                    "answer_task",
                    "question",
                    "Answer the assistant’s question",
                    "The workflow needs one detail to continue.",
                )),
                TaskStatus::WaitingForAuth => Some(self.auth_suggestion(task)),
                TaskStatus::Created | TaskStatus::Running => Some((
                    "view_progress",
                    "progress",
                    "View workflow progress",
                    "Aethra is continuing this task.",
                )),
                _ => None,
            };
            if let Some((id, kind, title, reason)) = suggestion {
                suggestions.push(serde_json::json!({"id":id,"task_id":task.id,"kind":kind,"title":title,"reason":reason}));
            }
        }
        suggestions
    }

    fn auth_suggestion(
        &self,
        task: &Task,
    ) -> (&'static str, &'static str, &'static str, &'static str) {
        let fallback = (
            "connect_service",
            "authorization",
            "Connect the requested service",
            "The workflow is waiting for authorization.",
        );
        let blocker = match self.store.task_blocker(task.id) {
            Ok(Some(blocker)) => blocker,
            _ => return fallback,
        };
        let title = match blocker {
            TaskBlocker::ProviderCredentialRequired { .. } => "Update the provider API key",
            TaskBlocker::ConnectorAuthorizationRequired { .. } => "Connect the requested service",
            TaskBlocker::AndroidPermissionRequired { .. } => "Grant the Android permission",
            TaskBlocker::ApprovalRequired { .. } => "Review the pending action",
            TaskBlocker::ClarificationRequired { .. } => "Answer the assistant's question",
            TaskBlocker::DeviceConstraint { .. } => "Resolve the device constraint",
            TaskBlocker::CapabilityUnavailable { .. } => "Connect a supporting service",
        };
        (
            "connect_service",
            "authorization",
            title,
            blocker.recovery_action(),
        )
    }

    async fn connection_state(&self, config: &ConnectionConfig) -> Result<ConnectionState> {
        let (state, granted_scopes, expires_at, message) = match &config.authentication {
            McpAuthentication::None => (
                ConnectionStatus::Connected,
                vec![],
                None,
                "No credential required.".into(),
            ),
            McpAuthentication::OauthAuthorizationCode {
                requested_scopes, ..
            } => {
                let status = self.oauth.status(config)?;
                let challenge = self.mcp.oauth_challenge(&config.id).await;
                let (state, message) = if challenge
                    .as_ref()
                    .is_some_and(|challenge| challenge.insufficient_scope)
                {
                    (
                        ConnectionStatus::StepUpRequired,
                        format!("{} needs additional access to continue.", config.name),
                    )
                } else {
                    match status.state {
                        OAuthState::Missing => (
                            ConnectionStatus::Required,
                            format!("Connect {} to continue.", config.name),
                        ),
                        OAuthState::WaitingForUserAuthorization => (
                            ConnectionStatus::Connecting,
                            format!("Finish connecting {} in your browser.", config.name),
                        ),
                        OAuthState::Connected => (
                            ConnectionStatus::Connected,
                            format!("{} is connected.", config.name),
                        ),
                        OAuthState::Expired => (
                            ConnectionStatus::Expired,
                            format!("Reconnect {} to continue.", config.name),
                        ),
                    }
                };
                let requested_scopes = challenge
                    .filter(|challenge| challenge.insufficient_scope)
                    .map(|challenge| {
                        let mut scopes = requested_scopes.clone();
                        for scope in challenge.scopes {
                            if !scopes.contains(&scope) {
                                scopes.push(scope);
                            }
                        }
                        scopes
                    })
                    .unwrap_or_else(|| requested_scopes.clone());
                let public = ConnectionState {
                    schema: CONNECTION_STATE_SCHEMA_V1.into(),
                    connection_id: config.id.clone(),
                    service_name: config.name.clone(),
                    state,
                    requested_scopes,
                    granted_scopes: status.granted_scopes,
                    expires_at: status.expires_at,
                    resume_task_id: None,
                    message,
                };
                public.validate()?;
                return Ok(public);
            }
            McpAuthentication::BearerToken { .. } | McpAuthentication::ApiKeyHeader { .. } => {
                if self.secrets.contains_mcp(config) {
                    (
                        ConnectionStatus::Connected,
                        vec![],
                        None,
                        format!("{} credential is configured.", config.name),
                    )
                } else {
                    (
                        ConnectionStatus::Required,
                        vec![],
                        None,
                        format!("Add a credential for {} to continue.", config.name),
                    )
                }
            }
        };
        let public = ConnectionState {
            schema: CONNECTION_STATE_SCHEMA_V1.into(),
            connection_id: config.id.clone(),
            service_name: config.name.clone(),
            state,
            requested_scopes: vec![],
            granted_scopes,
            expires_at,
            resume_task_id: None,
            message,
        };
        public.validate()?;
        Ok(public)
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
            self.fast.clone(),
            self.tool_executor.clone(),
            &settings,
            &self.secrets,
            &self.oauth,
        )?;
        self.store.set_setting(
            "settings",
            &serde_json::to_value(&settings).map_err(|_| Error::InvalidInput)?,
        )?;
        personalization::kick_learning(next.clone());
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
            "get_provider_catalog" => {
                let catalog = assistant_contracts::catalog_seeds::default_catalog()
                    .map_err(|_| Error::Storage)?;
                Ok(serde_json::to_value(catalog).map_err(|_| Error::Storage)?)
            }
            "next_step_suggestions" => {
                let tasks = self.store.tasks()?;
                Ok(serde_json::json!(self.next_step_suggestions(&tasks)))
            }
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
            "list_adaptive_rules"
            | "list_rule_proposals"
            | "propose_rule"
            | "remember_preference"
            | "review_rule_proposal"
            | "disable_adaptive_rule"
            | "rollback_rule"
            | "rule_history"
            | "record_observation"
            | "process_learning"
            | "personal_usage"
            | "propose_skill"
            | "evaluate_skill"
            | "learning_settings"
            | "personal_rule_details" => {
                self.reject_known_secrets(&payload).await?;
                self.personal_command(name, payload).await
            }
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
            "test_cloud_provider" => {
                let request: SaveCloudProvider =
                    serde_json::from_value(payload).map_err(|_| Error::InvalidInput)?;
                let secrets = if let Some(key) = request.api_key {
                    let secrets = Arc::new(SessionSecrets::default());
                    secrets.set(&request.cloud, key)?;
                    secrets
                } else {
                    self.secrets.clone()
                };
                let provider =
                    CloudProvider::new(request.cloud.clone(), secrets.provider(&request.cloud))?;
                serde_json::to_value(provider.test_connection().await)
                    .map_err(|_| Error::InvalidResponse)
            }
            "clear_cloud_key" => {
                self.secrets.clear()?;
                Ok(serde_json::json!(null))
            }
            "save_provider_profile" => {
                let request: SaveProviderProfile =
                    serde_json::from_value(payload).map_err(|_| Error::InvalidInput)?;
                let catalog = assistant_contracts::catalog_seeds::default_catalog()
                    .map_err(|_| Error::Storage)?;
                let definition = catalog
                    .get(&request.provider_id)
                    .ok_or(Error::InvalidInput)?;
                if definition.availability
                    != assistant_contracts::provider::ProviderAvailability::Available
                {
                    return Err(Error::Unavailable);
                }
                let option = definition
                    .auth_options
                    .iter()
                    .find(|option| option.id == request.auth_option_id)
                    .ok_or(Error::InvalidInput)?;
                if !request.config.is_object() {
                    return Err(Error::InvalidInput);
                }
                if let Some(base_url) = request
                    .config
                    .get("base_url")
                    .and_then(|value| value.as_str())
                {
                    if !base_url.trim().is_empty() {
                        assistant_contracts::provider::normalize_endpoint(Some(base_url), None)
                            .map_err(|_| Error::InvalidInput)?;
                    }
                }
                let needs_secret = option.auth_kind != AuthKind::None;
                match (&request.secret, needs_secret) {
                    (Some(_), false) => return Err(Error::InvalidInput),
                    (None, true) => return Err(Error::InvalidInput),
                    _ => {}
                }
                if let Some(secret) = &request.secret {
                    SessionSecrets::validate(secret)?;
                    if contains_secret(&request.config, secret) {
                        return Err(Error::InvalidInput);
                    }
                }
                let mut profile = ProviderProfile::new(
                    request.provider_id.clone(),
                    request.auth_option_id.clone(),
                );
                profile.non_secret_config = request.config;
                profile.display_name = Some(definition.display_name.clone());
                profile.validate().map_err(|_| Error::InvalidInput)?;
                self.store.save_provider_profile(&profile)?;
                if let Some(secret) = request.secret {
                    self.secrets.set_provider_key(
                        &request.provider_id,
                        &request.auth_option_id,
                        secret,
                    )?;
                }
                self.refresh_cloud_provider().await?;
                Ok(serde_json::to_value(profile).map_err(|_| Error::Storage)?)
            }
            "list_provider_profiles" => {
                let profiles = self.store.provider_profiles()?;
                let active = self
                    .store
                    .setting("active_provider_id")?
                    .and_then(|value| value.as_str().map(str::to_owned));
                let annotated = profiles
                    .into_iter()
                    .map(|profile| {
                        let key_configured = self
                            .secrets
                            .has_provider_key(&profile.provider_id, &profile.auth_option_id);
                        serde_json::json!({ "profile": profile, "key_configured": key_configured, "active": active.as_deref() == Some(profile.provider_id.as_str()) })
                    })
                    .collect::<Vec<_>>();
                Ok(serde_json::Value::Array(annotated))
            }
            "delete_provider_profile" => {
                let provider_id = payload["provider_id"].as_str().ok_or(Error::InvalidInput)?;
                self.store.delete_provider_profile(provider_id)?;
                self.secrets.clear_provider_key(provider_id)?;
                if self
                    .store
                    .setting("active_provider_id")?
                    .is_some_and(|value| value.as_str() == Some(provider_id))
                {
                    self.store
                        .set_setting("active_provider_id", &serde_json::Value::Null)?;
                }
                self.refresh_cloud_provider().await?;
                Ok(serde_json::json!(null))
            }
            "set_active_provider" => {
                let provider_id = payload["provider_id"].as_str().ok_or(Error::InvalidInput)?;
                self.store
                    .provider_profile(provider_id)?
                    .ok_or(Error::InvalidInput)?;
                self.store.set_setting(
                    "active_provider_id",
                    &serde_json::Value::String(provider_id.to_owned()),
                )?;
                self.refresh_cloud_provider().await?;
                Ok(serde_json::json!(null))
            }
            "test_provider_connection" => {
                let provider_id = payload["provider_id"].as_str().ok_or(Error::InvalidInput)?;
                let profile = self
                    .store
                    .provider_profile(provider_id)?
                    .ok_or(Error::InvalidInput)?;
                let catalog = assistant_contracts::catalog_seeds::default_catalog()
                    .map_err(|_| Error::Storage)?;
                let definition = catalog
                    .get(&profile.provider_id)
                    .ok_or(Error::InvalidInput)?;
                let option = definition
                    .auth_options
                    .iter()
                    .find(|option| option.id == profile.auth_option_id)
                    .ok_or(Error::InvalidInput)?;
                let secret = if option.auth_kind == AuthKind::None {
                    None
                } else {
                    Some(
                        self.secrets
                            .provider_secret(&profile.provider_id, &profile.auth_option_id)?,
                    )
                };
                let transport = provider_rig::transport::build_transport(
                    definition,
                    option,
                    &profile.non_secret_config,
                    secret.as_deref(),
                )
                .map_err(|_| Error::InvalidInput)?;
                let model = match &transport {
                    provider_rig::transport::ProviderTransport::OpenAi { model, .. }
                    | provider_rig::transport::ProviderTransport::Anthropic { model, .. }
                    | provider_rig::transport::ProviderTransport::Gemini { model, .. } => {
                        model.clone()
                    }
                    provider_rig::transport::ProviderTransport::Azure { deployment, .. } => {
                        deployment.clone()
                    }
                };
                let probe = ModelRequest {
                    schema: MODEL_REQUEST_SCHEMA_V1.into(),
                    messages: vec![ModelMessage {
                        role: ModelMessageRole::User,
                        content: "hi".into(),
                        tool_call_id: None,
                    }],
                    tools: vec![],
                    model: model.to_owned(),
                    max_tokens: Some(16),
                    temperature: None,
                    stream: false,
                };
                let started = std::time::Instant::now();
                match provider_rig::transport::complete_transport(&transport, &probe).await {
                    Ok(response) => Ok(serde_json::to_value(
                        assistant_contracts::connection_test::ConnectionTestResult::success(
                            &response.model_id,
                            started.elapsed().as_millis() as u64,
                        ),
                    )
                    .map_err(|_| Error::Storage)?),
                    Err(provider_rig::RigAdapterError::Normalized(error)) => {
                        let (kind, message) = test_failure(&error);
                        Ok(serde_json::to_value(
                            assistant_contracts::connection_test::ConnectionTestResult::failure(
                                kind, &message,
                            ),
                        )
                        .map_err(|_| Error::Storage)?)
                    }
                    Err(_) => Err(Error::Unavailable),
                }
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
                serde_json::to_value(self.connection_state(config).await?)
                    .map_err(|_| Error::InvalidResponse)
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

    /// Rebuilds the assistant so new tasks use the currently configured
    /// cloud provider. In-flight tasks keep running on the previous one.
    pub async fn refresh_cloud_provider(&self) -> Result<()> {
        let settings = self.settings.read().await.clone();
        let assistant = assemble(
            self.store.clone(),
            self.mcp.clone(),
            self.fast.clone(),
            self.tool_executor.clone(),
            &settings,
            &self.secrets,
            &self.oauth,
        )?;
        *self.assistant.write().await = assistant;
        Ok(())
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
    tokio::spawn(run_task(assistant, id));
}

fn test_failure(
    error: &NormalizedError,
) -> (
    assistant_contracts::provider_health::NormalizedTestFailure,
    String,
) {
    use assistant_contracts::provider_health::NormalizedTestFailure;
    match error {
        NormalizedError::AuthenticationFailed => (
            NormalizedTestFailure::Credential,
            "Authentication was rejected. Check the API key.".into(),
        ),
        NormalizedError::AuthorizationDenied => (
            NormalizedTestFailure::Credential,
            "Access was denied. Check the key permissions.".into(),
        ),
        NormalizedError::EndpointNotFound => (
            NormalizedTestFailure::Endpoint,
            "The endpoint could not be reached. Check the base URL.".into(),
        ),
        NormalizedError::ModelNotFound => (
            NormalizedTestFailure::ModelNotFound,
            "The model was not found. Check the model ID.".into(),
        ),
        NormalizedError::RateLimited { .. } | NormalizedError::QuotaExceeded => (
            NormalizedTestFailure::Quota,
            "Quota or rate limit reached. Retry later.".into(),
        ),
        NormalizedError::Timeout | NormalizedError::NetworkUnavailable => (
            NormalizedTestFailure::Network,
            "The network request failed. Check connectivity.".into(),
        ),
        NormalizedError::InvalidResponse { detail } => (
            NormalizedTestFailure::ProviderError,
            format!("The provider returned an invalid response: {detail}"),
        ),
        NormalizedError::ProviderError { detail, .. } => (
            NormalizedTestFailure::ProviderError,
            format!("The provider returned an error: {detail}"),
        ),
    }
}

fn bounded_rule_text(payload: &serde_json::Value, key: &str, limit: usize) -> Result<String> {
    let value = payload[key].as_str().ok_or(Error::InvalidInput)?.trim();
    if value.is_empty() || value.chars().count() > limit {
        return Err(Error::InvalidInput);
    }
    Ok(value.to_owned())
}

fn parse_rule_scope(value: &str) -> Result<RuleScope> {
    if value == "global" {
        return Ok(RuleScope::Global);
    }
    if let Some(id) = value.strip_prefix("conversation:") {
        return Id::parse_str(id)
            .map(RuleScope::Conversation)
            .map_err(|_| Error::InvalidInput);
    }
    if let Some(name) = value.strip_prefix("workflow:") {
        if name.trim().is_empty() {
            return Err(Error::InvalidInput);
        }
        return Ok(RuleScope::Workflow(name.to_owned()));
    }
    Err(Error::InvalidInput)
}

fn launch_recovery(assistant: Arc<Assistant>, task_ids: Vec<Id>) -> Result<()> {
    if task_ids.is_empty() {
        return Ok(());
    }
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        for task_id in task_ids {
            handle.spawn(run_task(assistant.clone(), task_id));
        }
        return Ok(());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| Error::Unavailable)?;
    std::thread::Builder::new()
        .name("aethra-recovery".into())
        .spawn(move || {
            runtime.block_on(async move {
                for task_id in task_ids {
                    run_task(assistant.clone(), task_id).await;
                }
            });
        })
        .map_err(|_| Error::Unavailable)?;
    Ok(())
}

async fn run_task(assistant: Arc<Assistant>, id: Id) {
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
    if let Ok(task) = assistant.store.task(id) {
        let _ = assistant.record_task_outcome(&task);
    }
}
fn assemble(
    store: Arc<SqliteStore>,
    mcp: Arc<McpManager>,
    fast: Arc<dyn ModelProvider>,
    injected_executor: Option<Arc<dyn ToolExecutor>>,
    settings: &Settings,
    secrets: &Arc<SessionSecrets>,
    oauth: &Arc<OAuthRuntime>,
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
    let cloud: Arc<dyn ModelProvider> = match rig_cloud_provider(&store, secrets) {
        Some(provider) => provider,
        None => match &settings.cloud {
            Some(config) => Arc::new(CloudProvider::new(
                config.clone(),
                secrets.provider(config),
            )?),
            None => Arc::new(NeedleProvider::unavailable()),
        },
    };
    let secret_store = secrets.clone();
    let oauth_store = oauth.clone();
    let cloud_config = settings.cloud.clone();
    let guard = Arc::new(move |payload: &serde_json::Value| {
        if secret_store.payload_contains_secret(payload)
            || oauth_store.payload_contains_secret(payload)
        {
            return Err(Error::Denied);
        }
        if let Some(config) = &cloud_config {
            if let Ok(secret) = secret_store.provider(config).get(&config.secret_ref) {
                if !secret.is_empty() && contains_secret(payload, &secret) {
                    return Err(Error::Denied);
                }
            }
        }
        Ok(())
    });
    let executor: Arc<dyn ToolExecutor> = match injected_executor {
        Some(native) => Arc::new(RoutingExecutor {
            native,
            mcp: mcp.clone(),
        }),
        None => mcp,
    };
    Ok(Arc::new(
        Assistant::new(store, fast, cloud, executor, settings.engine.clone())
            .with_personal_guard(guard),
    ))
}

struct RoutingExecutor {
    native: Arc<dyn ToolExecutor>,
    mcp: Arc<McpManager>,
}

fn rig_cloud_provider(
    store: &SqliteStore,
    secrets: &SessionSecrets,
) -> Option<Arc<dyn ModelProvider>> {
    let profiles = store.provider_profiles().ok()?;
    let active = store
        .setting("active_provider_id")
        .ok()
        .flatten()
        .and_then(|value| value.as_str().map(str::to_owned));
    let catalog = assistant_contracts::catalog_seeds::default_catalog().ok()?;
    let mut ordered: Vec<_> = profiles.iter().filter(|profile| profile.enabled).collect();
    ordered.sort_by_key(|profile| active.as_deref() != Some(profile.provider_id.as_str()));
    for profile in ordered {
        let definition = catalog.get(&profile.provider_id)?;
        if definition.availability != assistant_contracts::provider::ProviderAvailability::Available
        {
            continue;
        }
        let option = definition
            .auth_options
            .iter()
            .find(|option| option.id == profile.auth_option_id)?;
        let secret = if option.auth_kind == AuthKind::None {
            None
        } else {
            Some(
                secrets
                    .provider_secret(&profile.provider_id, &profile.auth_option_id)
                    .ok()?,
            )
        };
        let transport = provider_rig::transport::build_transport(
            definition,
            option,
            &profile.non_secret_config,
            secret.as_deref(),
        )
        .ok()?;
        if let Ok(provider) =
            provider_rig::cloud::RigCloudProvider::new(&profile.provider_id, transport)
        {
            return Some(Arc::new(provider));
        }
    }
    None
}

#[async_trait::async_trait]
impl ToolExecutor for RoutingExecutor {
    async fn execute(&self, spec: &ToolSpec, call: &ToolCall) -> Result<serde_json::Value> {
        if spec.source_tool == "open_external" {
            self.native.execute(spec, call).await
        } else {
            self.mcp.execute(spec, call).await
        }
    }
}
