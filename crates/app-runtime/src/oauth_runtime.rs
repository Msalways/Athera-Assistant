//! OAuth ownership for MCP connections. Secret values never leave this module.
use crate::credentials::SessionSecrets;
use adapter_mcp::{
    AuthorizationCallback, AuthorizationTransaction, ClientRegistration, ConnectionConfig,
    CredentialPurpose, CredentialRequest, CredentialResolver, McpAuthentication, OAuthDiscovery,
    OAuthHttpClient, OAuthTokenSet,
};
use assistant_contracts::{Error, Id, Result};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OAuthState {
    Missing,
    WaitingForUserAuthorization,
    Connected,
    Expired,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OAuthStatus {
    pub connection_id: String,
    pub state: OAuthState,
    pub granted_scopes: Vec<String>,
    pub expires_at: Option<u64>,
}

#[derive(Clone, Serialize)]
pub struct OAuthAuthorizationStart {
    pub transaction_id: Id,
    pub connection_id: String,
    pub authorization_url: String,
    pub expires_at: u64,
    pub requested_scopes: Vec<String>,
    pub resume_task_id: Option<Id>,
}

impl std::fmt::Debug for OAuthAuthorizationStart {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OAuthAuthorizationStart")
            .field("transaction_id", &self.transaction_id)
            .field("connection_id", &self.connection_id)
            .field("authorization_url", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .field("requested_scopes", &self.requested_scopes)
            .field("resume_task_id", &self.resume_task_id)
            .finish()
    }
}

struct PendingAuthorization {
    transaction: AuthorizationTransaction,
    binding: CredentialRequest,
    requested_scopes: Vec<String>,
    resume_task_id: Option<Id>,
}

#[derive(Clone)]
struct TokenCredential {
    binding: CredentialRequest,
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<u64>,
    granted_scopes: Vec<String>,
    token_endpoint: String,
    client_id: String,
}

/// Owns one-time OAuth transactions and connection-bound token sets.
pub struct OAuthRuntime {
    fallback: Arc<SessionSecrets>,
    http: OAuthHttpClient,
    pending: Mutex<BTreeMap<Id, PendingAuthorization>>,
    tokens: Mutex<BTreeMap<String, TokenCredential>>,
}

impl OAuthRuntime {
    pub fn new(fallback: Arc<SessionSecrets>) -> Result<Self> {
        Ok(Self {
            fallback,
            http: OAuthHttpClient::new()?,
            pending: Mutex::new(BTreeMap::new()),
            tokens: Mutex::new(BTreeMap::new()),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn begin(
        &self,
        config: &ConnectionConfig,
        client_id: Option<&str>,
        client_metadata_url: Option<&str>,
        redirect_uri: &str,
        resume_task_id: Option<Id>,
    ) -> Result<OAuthAuthorizationStart> {
        config.validate()?;
        let (expected_server, _) = oauth_profile(config)?;
        let endpoint = reqwest::Url::parse(&config.url).map_err(|_| Error::InvalidInput)?;
        let discovery = self.http.discover(&endpoint, Some(expected_server)).await?;
        self.begin_discovered(
            config,
            client_id,
            client_metadata_url,
            redirect_uri,
            resume_task_id,
            discovery,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn begin_discovered(
        &self,
        config: &ConnectionConfig,
        configured_client_id: Option<&str>,
        client_metadata_url: Option<&str>,
        redirect_uri: &str,
        resume_task_id: Option<Id>,
        discovery: OAuthDiscovery,
    ) -> Result<OAuthAuthorizationStart> {
        let (_, requested_scopes) = oauth_profile(config)?;
        validate_requested_scopes(requested_scopes, &discovery)?;
        let registration = ClientRegistration::select(
            &discovery.authorization_server,
            configured_client_id,
            client_metadata_url,
        )?;
        let client_id = match registration {
            ClientRegistration::PreRegistered { client_id }
            | ClientRegistration::ClientIdMetadataDocument { client_id } => client_id,
            ClientRegistration::DynamicRegistration { endpoint } => {
                self.http.register_dynamic(&endpoint, redirect_uri).await?
            }
            ClientRegistration::ExplicitConfigurationRequired => return Err(Error::InvalidInput),
        };
        let endpoint = reqwest::Url::parse(&config.url).map_err(|_| Error::InvalidInput)?;
        let now = now()?;
        let transaction = AuthorizationTransaction::begin(
            &config.id,
            &endpoint,
            &discovery.authorization_server,
            &client_id,
            redirect_uri,
            requested_scopes,
            now,
        )?;
        let transaction_id = Id::new_v4();
        let result = OAuthAuthorizationStart {
            transaction_id,
            connection_id: config.id.clone(),
            authorization_url: transaction.authorization_url.clone(),
            expires_at: transaction.expires_at,
            requested_scopes: requested_scopes.to_vec(),
            resume_task_id,
        };
        let binding = config.credential_request()?.ok_or(Error::InvalidInput)?;
        self.pending.lock().map_err(|_| Error::Unavailable)?.insert(
            transaction_id,
            PendingAuthorization {
                transaction,
                binding,
                requested_scopes: requested_scopes.to_vec(),
                resume_task_id,
            },
        );
        Ok(result)
    }

    pub async fn complete(
        &self,
        transaction_id: Id,
        callback: AuthorizationCallback,
        current: &ConnectionConfig,
    ) -> Result<Option<Id>> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| Error::Unavailable)?
            .remove(&transaction_id)
            .ok_or(Error::Denied)?;
        let current_binding = current.credential_request()?.ok_or(Error::Denied)?;
        if current_binding != pending.binding {
            return Err(Error::Denied);
        }
        let exchange = pending.transaction.accept_callback(&callback, now()?)?;
        let token_endpoint = exchange.token_endpoint.clone();
        let client_id = exchange.client_id.clone();
        let tokens = self.http.exchange(exchange, now()?).await?;
        ensure_scope_grant(&pending.requested_scopes, &tokens.granted_scopes)?;
        self.commit_tokens(current_binding, tokens, token_endpoint, client_id)?;
        Ok(pending.resume_task_id)
    }

    pub async fn ensure_fresh(&self, config: &ConnectionConfig) -> Result<()> {
        let binding = match config.credential_request()? {
            Some(binding) if binding.purpose == CredentialPurpose::OauthAccessToken => binding,
            _ => return Ok(()),
        };
        self.ensure_fresh_request(&binding).await
    }

    async fn ensure_fresh_request(&self, binding: &CredentialRequest) -> Result<()> {
        if binding.purpose != CredentialPurpose::OauthAccessToken {
            return Ok(());
        }
        let refresh = {
            let tokens = self.tokens.lock().map_err(|_| Error::Unavailable)?;
            let token = tokens
                .get(&binding.connection_id)
                .ok_or(Error::AuthRequired)?;
            if token.binding != *binding {
                return Err(Error::AuthRequired);
            }
            let refresh_deadline = now()?.saturating_add(60);
            if token
                .expires_at
                .is_none_or(|expiry| expiry > refresh_deadline)
            {
                return Ok(());
            }
            (
                token.token_endpoint.clone(),
                token.client_id.clone(),
                token.refresh_token.clone().ok_or(Error::AuthRequired)?,
            )
        };
        let updated = self
            .http
            .refresh(&refresh.0, &refresh.1, &binding.resource, refresh.2, now()?)
            .await?;
        self.commit_tokens(binding.clone(), updated, refresh.0, refresh.1)
    }

    pub fn clear(&self, connection_id: &str) -> Result<()> {
        self.tokens
            .lock()
            .map_err(|_| Error::Unavailable)?
            .remove(connection_id);
        self.pending
            .lock()
            .map_err(|_| Error::Unavailable)?
            .retain(|_, pending| pending.transaction.connection_id != connection_id);
        Ok(())
    }

    pub fn cancel(&self, transaction_id: Id) -> Result<()> {
        self.pending
            .lock()
            .map_err(|_| Error::Unavailable)?
            .remove(&transaction_id)
            .ok_or(Error::Denied)?;
        Ok(())
    }

    pub fn pending_connection(&self, transaction_id: Id) -> Result<String> {
        self.pending
            .lock()
            .map_err(|_| Error::Unavailable)?
            .get(&transaction_id)
            .map(|pending| pending.transaction.connection_id.clone())
            .ok_or(Error::Denied)
    }

    pub fn status(&self, config: &ConnectionConfig) -> Result<OAuthStatus> {
        let binding = config.credential_request()?.ok_or(Error::InvalidInput)?;
        let pending = self.pending.lock().map_err(|_| Error::Unavailable)?;
        if pending.values().any(|entry| entry.binding == binding) {
            return Ok(OAuthStatus {
                connection_id: config.id.clone(),
                state: OAuthState::WaitingForUserAuthorization,
                granted_scopes: vec![],
                expires_at: None,
            });
        }
        drop(pending);
        let tokens = self.tokens.lock().map_err(|_| Error::Unavailable)?;
        let Some(token) = tokens
            .get(&config.id)
            .filter(|token| token.binding == binding)
        else {
            return Ok(OAuthStatus {
                connection_id: config.id.clone(),
                state: OAuthState::Missing,
                granted_scopes: vec![],
                expires_at: None,
            });
        };
        let state = if token
            .expires_at
            .is_some_and(|expiry| expiry <= now().unwrap_or(u64::MAX))
        {
            OAuthState::Expired
        } else {
            OAuthState::Connected
        };
        Ok(OAuthStatus {
            connection_id: config.id.clone(),
            state,
            granted_scopes: token.granted_scopes.clone(),
            expires_at: token.expires_at,
        })
    }

    pub fn payload_contains_secret(&self, payload: &serde_json::Value) -> bool {
        self.tokens.lock().is_ok_and(|tokens| {
            tokens.values().any(|token| {
                contains_secret(payload, &token.access_token)
                    || token
                        .refresh_token
                        .as_ref()
                        .is_some_and(|secret| contains_secret(payload, secret))
            })
        })
    }

    fn commit_tokens(
        &self,
        binding: CredentialRequest,
        tokens: OAuthTokenSet,
        token_endpoint: String,
        client_id: String,
    ) -> Result<()> {
        let credential = TokenCredential {
            binding: binding.clone(),
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_at: tokens.expires_at,
            granted_scopes: tokens.granted_scopes,
            token_endpoint,
            client_id,
        };
        self.tokens
            .lock()
            .map_err(|_| Error::Unavailable)?
            .insert(binding.connection_id, credential);
        Ok(())
    }
}

#[async_trait::async_trait]
impl CredentialResolver for OAuthRuntime {
    async fn prepare(&self, request: &CredentialRequest) -> Result<()> {
        self.ensure_fresh_request(request).await
    }

    fn resolve(&self, request: &CredentialRequest) -> Result<String> {
        if request.purpose != CredentialPurpose::OauthAccessToken {
            return self.fallback.resolve(request);
        }
        let tokens = self.tokens.lock().map_err(|_| Error::Unavailable)?;
        let token = tokens
            .get(&request.connection_id)
            .ok_or(Error::AuthRequired)?;
        if token.binding != *request
            || token
                .expires_at
                .is_some_and(|expiry| expiry <= now().unwrap_or(u64::MAX))
        {
            return Err(Error::AuthRequired);
        }
        Ok(token.access_token.clone())
    }
}

fn oauth_profile(config: &ConnectionConfig) -> Result<(&str, &[String])> {
    match &config.authentication {
        McpAuthentication::OauthAuthorizationCode {
            authorization_server,
            requested_scopes,
            ..
        } => Ok((authorization_server, requested_scopes)),
        _ => Err(Error::InvalidInput),
    }
}

fn validate_requested_scopes(requested: &[String], discovery: &OAuthDiscovery) -> Result<()> {
    for supported in [
        &discovery.protected_resource.scopes_supported,
        &discovery.authorization_server.scopes_supported,
    ] {
        if !supported.is_empty() && requested.iter().any(|scope| !supported.contains(scope)) {
            return Err(Error::Denied);
        }
    }
    Ok(())
}

fn ensure_scope_grant(requested: &[String], granted: &[String]) -> Result<()> {
    if !granted.is_empty() && requested.iter().any(|scope| !granted.contains(scope)) {
        return Err(Error::Denied);
    }
    Ok(())
}

fn now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .map_err(|_| Error::Unavailable)
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

#[cfg(test)]
mod tests {
    use super::*;
    use adapter_mcp::{
        AuthorizationServerMetadata, McpTransport, ProtectedResourceMetadata,
        MCP_CONNECTION_SCHEMA_V1,
    };

    fn connection(scopes: &[&str]) -> ConnectionConfig {
        ConnectionConfig {
            schema: MCP_CONNECTION_SCHEMA_V1.into(),
            id: "fixture".into(),
            name: "Fixture".into(),
            url: "https://mcp.example.com/api".into(),
            transport: McpTransport::StreamableHttp,
            authentication: McpAuthentication::OauthAuthorizationCode {
                token_ref: "keystore:fixture".into(),
                authorization_server: "https://auth.example.com".into(),
                resource: "https://mcp.example.com/api".into(),
                requested_scopes: scopes.iter().map(|scope| (*scope).into()).collect(),
            },
            preset: None,
        }
    }

    fn discovery(scopes: &[&str]) -> OAuthDiscovery {
        let scopes: Vec<_> = scopes.iter().map(|scope| (*scope).into()).collect();
        OAuthDiscovery {
            protected_resource: ProtectedResourceMetadata {
                resource: "https://mcp.example.com/api".into(),
                authorization_servers: vec!["https://auth.example.com".into()],
                scopes_supported: scopes.clone(),
            },
            authorization_server: AuthorizationServerMetadata {
                issuer: "https://auth.example.com".into(),
                authorization_endpoint: "https://auth.example.com/authorize".into(),
                token_endpoint: "https://auth.example.com/token".into(),
                code_challenge_methods_supported: vec!["S256".into()],
                scopes_supported: scopes,
                registration_endpoint: None,
                client_id_metadata_document_supported: false,
            },
        }
    }

    #[tokio::test]
    async fn transaction_is_bounded_redacted_and_cancellable_once() {
        let runtime = OAuthRuntime::new(Arc::new(SessionSecrets::default())).unwrap();
        let config = connection(&["read"]);
        let start = runtime
            .begin_discovered(
                &config,
                Some("public-client"),
                None,
                "https://app.example.com/oauth/callback",
                Some(Id::nil()),
                discovery(&["read"]),
            )
            .await
            .unwrap();
        assert!(start
            .authorization_url
            .contains("code_challenge_method=S256"));
        assert!(start
            .authorization_url
            .contains("resource=https%3A%2F%2Fmcp.example.com%2Fapi"));
        assert!(!format!("{start:?}").contains("state="));
        assert_eq!(
            runtime.status(&config).unwrap().state,
            OAuthState::WaitingForUserAuthorization
        );
        assert_eq!(
            runtime.pending_connection(start.transaction_id).unwrap(),
            "fixture"
        );
        runtime.cancel(start.transaction_id).unwrap();
        assert_eq!(runtime.cancel(start.transaction_id), Err(Error::Denied));
        assert_eq!(runtime.status(&config).unwrap().state, OAuthState::Missing);
    }

    #[tokio::test]
    async fn unsupported_scope_stops_before_authorization() {
        let runtime = OAuthRuntime::new(Arc::new(SessionSecrets::default())).unwrap();
        assert_eq!(
            runtime
                .begin_discovered(
                    &connection(&["write"]),
                    Some("public-client"),
                    None,
                    "https://app.example.com/oauth/callback",
                    None,
                    discovery(&["read"]),
                )
                .await
                .unwrap_err(),
            Error::Denied
        );
    }

    #[test]
    fn tokens_are_exactly_bound_redacted_and_removed() {
        let runtime = OAuthRuntime::new(Arc::new(SessionSecrets::default())).unwrap();
        let config = connection(&["read"]);
        let binding = config.credential_request().unwrap().unwrap();
        let access = "access-token-fixture";
        let refresh = "refresh-token-fixture";
        runtime
            .commit_tokens(
                binding.clone(),
                OAuthTokenSet {
                    access_token: access.into(),
                    refresh_token: Some(refresh.into()),
                    expires_at: Some(now().unwrap() + 3600),
                    granted_scopes: vec!["read".into()],
                },
                "https://auth.example.com/token".into(),
                "public-client".into(),
            )
            .unwrap();
        let status = runtime.status(&config).unwrap();
        assert_eq!(status.state, OAuthState::Connected);
        assert_eq!(status.granted_scopes, vec!["read".to_string()]);
        assert_eq!(runtime.resolve(&binding).unwrap(), access);
        assert!(runtime.payload_contains_secret(&serde_json::json!({"text":refresh})));
        let mut wrong_binding = binding.clone();
        wrong_binding.resource = "https://mcp.example.com/other".into();
        assert_eq!(runtime.resolve(&wrong_binding), Err(Error::AuthRequired));
        runtime.clear(&config.id).unwrap();
        assert_eq!(runtime.resolve(&binding), Err(Error::AuthRequired));
    }
}
