use crate::auth::origin;
use assistant_contracts::{Error, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

const TRANSACTION_TTL_SECONDS: u64 = 600;
const MAX_OAUTH_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthChallenge {
    pub resource_metadata: Option<String>,
    pub scopes: Vec<String>,
    pub insufficient_scope: bool,
}

impl OAuthChallenge {
    pub fn parse(header: &str) -> Result<Self> {
        let (scheme, parameters) = header.split_once(' ').ok_or(Error::InvalidResponse)?;
        if !scheme.eq_ignore_ascii_case("bearer") {
            return Err(Error::InvalidResponse);
        }
        let mut resource_metadata = None;
        let mut scopes = Vec::new();
        let mut insufficient_scope = false;
        for (name, value) in parse_parameters(parameters)? {
            match name.as_str() {
                "resource_metadata" => {
                    let url = secure_url(&value)?;
                    resource_metadata = Some(url.to_string());
                }
                "scope" => scopes = validate_scopes(value.split_ascii_whitespace())?,
                "error" => insufficient_scope = value == "insufficient_scope",
                _ => {}
            }
        }
        Ok(Self {
            resource_metadata,
            scopes,
            insufficient_scope,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtectedResourceMetadata {
    pub resource: String,
    pub authorization_servers: Vec<String>,
    #[serde(default)]
    pub scopes_supported: Vec<String>,
}

impl ProtectedResourceMetadata {
    pub fn validate(&self, endpoint: &Url) -> Result<()> {
        let resource = secure_url(&self.resource)?;
        if canonical_resource(&resource) != canonical_resource(endpoint)
            || self.authorization_servers.is_empty()
            || self.authorization_servers.len() > 8
        {
            return Err(Error::InvalidResponse);
        }
        for issuer in &self.authorization_servers {
            secure_url(issuer)?;
        }
        validate_scopes(self.scopes_supported.iter().map(String::as_str))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizationServerMetadata {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub code_challenge_methods_supported: Vec<String>,
    #[serde(default)]
    pub scopes_supported: Vec<String>,
    #[serde(default)]
    pub registration_endpoint: Option<String>,
    #[serde(default)]
    pub client_id_metadata_document_supported: bool,
}

impl AuthorizationServerMetadata {
    pub fn validate(&self, expected_issuer: &Url) -> Result<()> {
        let issuer = secure_url(&self.issuer)?;
        if canonical_resource(&issuer) != canonical_resource(expected_issuer)
            || !self
                .code_challenge_methods_supported
                .iter()
                .any(|method| method == "S256")
        {
            return Err(Error::InvalidResponse);
        }
        secure_url(&self.authorization_endpoint)?;
        secure_url(&self.token_endpoint)?;
        if let Some(endpoint) = &self.registration_endpoint {
            secure_url(endpoint)?;
        }
        validate_scopes(self.scopes_supported.iter().map(String::as_str))?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientRegistration {
    PreRegistered { client_id: String },
    ClientIdMetadataDocument { client_id: String },
    DynamicRegistration { endpoint: String },
    ExplicitConfigurationRequired,
}

impl ClientRegistration {
    pub fn select(
        metadata: &AuthorizationServerMetadata,
        configured_client_id: Option<&str>,
        client_metadata_url: Option<&str>,
    ) -> Result<Self> {
        if let Some(client_id) = configured_client_id {
            validate_client_id(client_id)?;
            return Ok(Self::PreRegistered {
                client_id: client_id.to_owned(),
            });
        }
        if metadata.client_id_metadata_document_supported {
            if let Some(client_id) = client_metadata_url {
                let url = secure_url(client_id)?;
                if url.path() == "/" {
                    return Err(Error::InvalidInput);
                }
                return Ok(Self::ClientIdMetadataDocument {
                    client_id: url.to_string(),
                });
            }
        }
        if let Some(endpoint) = &metadata.registration_endpoint {
            secure_url(endpoint)?;
            return Ok(Self::DynamicRegistration {
                endpoint: endpoint.clone(),
            });
        }
        Ok(Self::ExplicitConfigurationRequired)
    }
}

pub struct AuthorizationTransaction {
    pub connection_id: String,
    pub origin: String,
    pub resource: String,
    pub authorization_server: String,
    pub redirect_uri: String,
    pub authorization_url: String,
    pub expires_at: u64,
    state: String,
    code_verifier: String,
    token_endpoint: String,
    client_id: String,
    used: bool,
}

impl fmt::Debug for AuthorizationTransaction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthorizationTransaction")
            .field("connection_id", &self.connection_id)
            .field("origin", &self.origin)
            .field("resource", &self.resource)
            .field("authorization_server", &self.authorization_server)
            .field("redirect_uri", &self.redirect_uri)
            .field("authorization_url", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .field("state", &"[REDACTED]")
            .field("code_verifier", &"[REDACTED]")
            .field("token_endpoint", &self.token_endpoint)
            .field("used", &self.used)
            .finish()
    }
}

impl AuthorizationTransaction {
    #[allow(clippy::too_many_arguments)]
    pub fn begin(
        connection_id: &str,
        endpoint: &Url,
        metadata: &AuthorizationServerMetadata,
        client_id: &str,
        redirect_uri: &str,
        scopes: &[String],
        now: u64,
    ) -> Result<Self> {
        if connection_id.is_empty() || connection_id.len() > 100 {
            return Err(Error::InvalidInput);
        }
        let issuer = secure_url(&metadata.issuer)?;
        metadata.validate(&issuer)?;
        validate_client_id(client_id)?;
        let redirect = secure_url(redirect_uri)?;
        if redirect.query().is_some() {
            return Err(Error::InvalidInput);
        }
        validate_scopes(scopes.iter().map(String::as_str))?;
        let mut verifier_bytes = [0_u8; 32];
        let mut state_bytes = [0_u8; 32];
        getrandom::fill(&mut verifier_bytes).map_err(|_| Error::Unavailable)?;
        getrandom::fill(&mut state_bytes).map_err(|_| Error::Unavailable)?;
        let code_verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);
        let state = URL_SAFE_NO_PAD.encode(state_bytes);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
        let mut authorization_url = secure_url(&metadata.authorization_endpoint)?;
        {
            let mut query = authorization_url.query_pairs_mut();
            query
                .append_pair("response_type", "code")
                .append_pair("client_id", client_id)
                .append_pair("redirect_uri", redirect.as_str())
                .append_pair("code_challenge", &challenge)
                .append_pair("code_challenge_method", "S256")
                .append_pair("resource", endpoint.as_str())
                .append_pair("state", &state);
            if !scopes.is_empty() {
                query.append_pair("scope", &scopes.join(" "));
            }
        }
        Ok(Self {
            connection_id: connection_id.to_owned(),
            origin: origin(endpoint)?,
            resource: endpoint.as_str().to_owned(),
            authorization_server: issuer.to_string(),
            redirect_uri: redirect.to_string(),
            authorization_url: authorization_url.to_string(),
            expires_at: now.saturating_add(TRANSACTION_TTL_SECONDS),
            state,
            code_verifier,
            token_endpoint: metadata.token_endpoint.clone(),
            client_id: client_id.to_owned(),
            used: false,
        })
    }

    pub fn accept_callback(
        &mut self,
        callback: &AuthorizationCallback,
        now: u64,
    ) -> Result<TokenExchange> {
        if self.used || now > self.expires_at || callback.url.len() > 8_192 {
            return Err(Error::Denied);
        }
        let callback_url = Url::parse(&callback.url).map_err(|_| Error::InvalidInput)?;
        let redirect = Url::parse(&self.redirect_uri).map_err(|_| Error::InvalidInput)?;
        if callback_url.scheme() != redirect.scheme()
            || callback_url.host_str() != redirect.host_str()
            || callback_url.port_or_known_default() != redirect.port_or_known_default()
            || callback_url.path() != redirect.path()
            || callback_url.fragment().is_some()
        {
            return Err(Error::Denied);
        }
        let pairs: Vec<_> = callback_url.query_pairs().collect();
        if pairs.iter().any(|(key, _)| key == "error") {
            self.used = true;
            return Err(Error::Denied);
        }
        let state = unique_parameter(&pairs, "state")?;
        let code = unique_parameter(&pairs, "code")?;
        if !constant_time_eq(state.as_bytes(), self.state.as_bytes())
            || code.is_empty()
            || code.len() > 4096
        {
            return Err(Error::Denied);
        }
        self.used = true;
        Ok(TokenExchange {
            token_endpoint: self.token_endpoint.clone(),
            client_id: self.client_id.clone(),
            code,
            code_verifier: self.code_verifier.clone(),
            redirect_uri: self.redirect_uri.clone(),
            resource: self.resource.clone(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationCallback {
    pub url: String,
}

pub struct TokenExchange {
    pub token_endpoint: String,
    pub client_id: String,
    pub code: String,
    pub code_verifier: String,
    pub redirect_uri: String,
    pub resource: String,
}

impl fmt::Debug for TokenExchange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TokenExchange")
            .field("token_endpoint", &self.token_endpoint)
            .field("client_id", &self.client_id)
            .field("code", &"[REDACTED]")
            .field("code_verifier", &"[REDACTED]")
            .field("redirect_uri", &self.redirect_uri)
            .field("resource", &self.resource)
            .finish()
    }
}

pub struct OAuthTokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
    pub granted_scopes: Vec<String>,
}

impl fmt::Debug for OAuthTokenSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthTokenSet")
            .field("access_token", &"[REDACTED]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("expires_at", &self.expires_at)
            .field("granted_scopes", &self.granted_scopes)
            .finish()
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
    scope: Option<String>,
}

impl TokenResponse {
    fn into_tokens(self, now: u64, previous_refresh: Option<String>) -> Result<OAuthTokenSet> {
        if !self.token_type.eq_ignore_ascii_case("bearer") {
            return Err(Error::InvalidResponse);
        }
        validate_token(&self.access_token)?;
        let refresh_token = self.refresh_token.or(previous_refresh);
        if let Some(refresh_token) = &refresh_token {
            validate_token(refresh_token)?;
        }
        let expires_at = match self.expires_in {
            Some(seconds) if seconds > 0 && seconds <= 365 * 24 * 60 * 60 => {
                Some(now.saturating_add(seconds))
            }
            Some(_) => return Err(Error::InvalidResponse),
            None => None,
        };
        let granted_scopes = self
            .scope
            .map(|scope| validate_scopes(scope.split_ascii_whitespace()))
            .transpose()?
            .unwrap_or_default();
        Ok(OAuthTokenSet {
            access_token: self.access_token,
            refresh_token,
            expires_at,
            granted_scopes,
        })
    }
}

#[derive(Clone)]
pub struct OAuthHttpClient {
    client: reqwest::Client,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OAuthDiscovery {
    pub protected_resource: ProtectedResourceMetadata,
    pub authorization_server: AuthorizationServerMetadata,
}

impl OAuthHttpClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|_| Error::Unavailable)?,
        })
    }

    pub async fn discover(
        &self,
        endpoint: &Url,
        expected_authorization_server: Option<&str>,
    ) -> Result<OAuthDiscovery> {
        let endpoint = secure_url(endpoint.as_str())?;
        let protected_resource = self
            .first_json::<ProtectedResourceMetadata>(&protected_resource_metadata_urls(
                &endpoint, None,
            )?)
            .await?;
        protected_resource.validate(&endpoint)?;
        let issuer = match expected_authorization_server {
            Some(expected) => {
                let expected = secure_url(expected)?;
                if !protected_resource
                    .authorization_servers
                    .iter()
                    .any(|value| {
                        Url::parse(value).is_ok_and(|value| {
                            canonical_resource(&value) == canonical_resource(&expected)
                        })
                    })
                {
                    return Err(Error::Denied);
                }
                expected
            }
            None if protected_resource.authorization_servers.len() == 1 => {
                secure_url(&protected_resource.authorization_servers[0])?
            }
            None => return Err(Error::InvalidInput),
        };
        let authorization_server = self
            .first_json::<AuthorizationServerMetadata>(&authorization_server_metadata_urls(
                &issuer,
            )?)
            .await?;
        authorization_server.validate(&issuer)?;
        Ok(OAuthDiscovery {
            protected_resource,
            authorization_server,
        })
    }

    pub async fn exchange(&self, exchange: TokenExchange, now: u64) -> Result<OAuthTokenSet> {
        let response = self
            .client
            .post(secure_url(&exchange.token_endpoint)?)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", exchange.code.as_str()),
                ("client_id", exchange.client_id.as_str()),
                ("code_verifier", exchange.code_verifier.as_str()),
                ("redirect_uri", exchange.redirect_uri.as_str()),
                ("resource", exchange.resource.as_str()),
            ])
            .send()
            .await
            .map_err(|_| Error::Unavailable)?;
        read_token_response(response, now, None).await
    }

    /// Registers this public client when the authorization server advertises RFC 7591 DCR.
    pub async fn register_dynamic(&self, endpoint: &str, redirect_uri: &str) -> Result<String> {
        let endpoint = secure_url(endpoint)?;
        let redirect_uri = secure_url(redirect_uri)?;
        let response = self
            .client
            .post(endpoint)
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&serde_json::json!({
                "client_name": "AETHRA",
                "redirect_uris": [redirect_uri.as_str()],
                "grant_types": ["authorization_code"],
                "response_types": ["code"],
                "token_endpoint_auth_method": "none"
            }))
            .send()
            .await
            .map_err(|_| Error::Unavailable)?;
        if !response.status().is_success() {
            return Err(match response.status().as_u16() {
                400 | 401 | 403 => Error::Denied,
                408 | 504 => Error::Timeout,
                429 => Error::RateLimited,
                _ => Error::Unavailable,
            });
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if !content_type
            .to_ascii_lowercase()
            .starts_with("application/json")
        {
            return Err(Error::InvalidResponse);
        }
        #[derive(Deserialize)]
        struct RegistrationResponse {
            client_id: String,
            #[serde(default)]
            token_endpoint_auth_method: Option<String>,
        }
        let registration: RegistrationResponse =
            serde_json::from_slice(&read_bounded(response).await?)
                .map_err(|_| Error::InvalidResponse)?;
        validate_client_id(&registration.client_id)?;
        if registration
            .token_endpoint_auth_method
            .as_deref()
            .is_some_and(|method| method != "none")
        {
            return Err(Error::InvalidResponse);
        }
        Ok(registration.client_id)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn refresh(
        &self,
        token_endpoint: &str,
        client_id: &str,
        resource: &str,
        refresh_token: String,
        now: u64,
    ) -> Result<OAuthTokenSet> {
        let token_endpoint = secure_url(token_endpoint)?;
        validate_client_id(client_id)?;
        secure_url(resource)?;
        validate_token(&refresh_token)?;
        let response = self
            .client
            .post(token_endpoint)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token.as_str()),
                ("client_id", client_id),
                ("resource", resource),
            ])
            .send()
            .await
            .map_err(|_| Error::Unavailable)?;
        read_token_response(response, now, Some(refresh_token)).await
    }

    async fn first_json<T: serde::de::DeserializeOwned>(&self, urls: &[Url]) -> Result<T> {
        for url in urls {
            let response = self
                .client
                .get(url.clone())
                .header(reqwest::header::ACCEPT, "application/json")
                .send()
                .await
                .map_err(|_| Error::Unavailable)?;
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                continue;
            }
            if !response.status().is_success() {
                return Err(Error::Unavailable);
            }
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default();
            if !content_type
                .to_ascii_lowercase()
                .starts_with("application/json")
            {
                return Err(Error::InvalidResponse);
            }
            let bytes = read_bounded(response).await?;
            return serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse);
        }
        Err(Error::Unavailable)
    }
}

async fn read_token_response(
    response: reqwest::Response,
    now: u64,
    previous_refresh: Option<String>,
) -> Result<OAuthTokenSet> {
    match response.status() {
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            return Err(Error::AuthRequired)
        }
        reqwest::StatusCode::TOO_MANY_REQUESTS => return Err(Error::RateLimited),
        status if !status.is_success() => return Err(Error::Unavailable),
        _ => {}
    }
    let bytes = read_bounded(response).await?;
    let response: TokenResponse =
        serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)?;
    response.into_tokens(now, previous_refresh)
}

async fn read_bounded(mut response: reqwest::Response) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_OAUTH_RESPONSE_BYTES as u64)
    {
        return Err(Error::InvalidResponse);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Error::Unavailable)? {
        if bytes.len().saturating_add(chunk.len()) > MAX_OAUTH_RESPONSE_BYTES {
            return Err(Error::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub fn protected_resource_metadata_urls(
    endpoint: &Url,
    challenge: Option<&OAuthChallenge>,
) -> Result<Vec<Url>> {
    if let Some(metadata) = challenge.and_then(|challenge| challenge.resource_metadata.as_deref()) {
        let metadata = secure_url(metadata)?;
        if origin(&metadata)? != origin(endpoint)? {
            return Err(Error::Denied);
        }
        return Ok(vec![metadata]);
    }
    let mut path_url = endpoint.clone();
    let endpoint_path = endpoint.path().trim_start_matches('/');
    let metadata_path = if endpoint_path.is_empty() {
        "/.well-known/oauth-protected-resource".to_owned()
    } else {
        format!("/.well-known/oauth-protected-resource/{endpoint_path}")
    };
    path_url.set_path(&metadata_path);
    path_url.set_query(None);
    path_url.set_fragment(None);
    let mut root_url = endpoint.clone();
    root_url.set_path("/.well-known/oauth-protected-resource");
    root_url.set_query(None);
    root_url.set_fragment(None);
    if path_url == root_url {
        Ok(vec![root_url])
    } else {
        Ok(vec![path_url, root_url])
    }
}

pub fn authorization_server_metadata_urls(issuer: &Url) -> Result<Vec<Url>> {
    let issuer = secure_url(issuer.as_str())?;
    let path = issuer.path().trim_matches('/');
    let suffixes = if path.is_empty() {
        vec![
            "/.well-known/oauth-authorization-server".to_owned(),
            "/.well-known/openid-configuration".to_owned(),
        ]
    } else {
        vec![
            format!("/.well-known/oauth-authorization-server/{path}"),
            format!("/.well-known/openid-configuration/{path}"),
            format!("/{path}/.well-known/openid-configuration"),
        ]
    };
    suffixes
        .into_iter()
        .map(|path| {
            let mut url = issuer.clone();
            url.set_path(&path);
            url.set_query(None);
            url.set_fragment(None);
            Ok(url)
        })
        .collect()
}

fn parse_parameters(input: &str) -> Result<Vec<(String, String)>> {
    let mut output = Vec::new();
    let mut cursor = input.trim();
    while !cursor.is_empty() {
        let equals = cursor.find('=').ok_or(Error::InvalidResponse)?;
        let name = cursor[..equals].trim().to_ascii_lowercase();
        cursor = cursor[equals + 1..].trim_start();
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(Error::InvalidResponse);
        }
        let (value, rest) = if let Some(quoted) = cursor.strip_prefix('"') {
            let mut escaped = false;
            let mut end = None;
            for (index, character) in quoted.char_indices() {
                if character == '"' && !escaped {
                    end = Some(index);
                    break;
                }
                escaped = character == '\\' && !escaped;
                if character != '\\' {
                    escaped = false;
                }
            }
            let end = end.ok_or(Error::InvalidResponse)?;
            if quoted[..end].contains('\\') {
                return Err(Error::InvalidResponse);
            }
            (quoted[..end].to_owned(), &quoted[end + 1..])
        } else {
            let end = cursor.find(',').unwrap_or(cursor.len());
            (cursor[..end].trim().to_owned(), &cursor[end..])
        };
        if output.iter().any(|(existing, _)| existing == &name) {
            return Err(Error::InvalidResponse);
        }
        output.push((name, value));
        cursor = rest.trim_start();
        if let Some(rest) = cursor.strip_prefix(',') {
            cursor = rest.trim_start();
        } else if !cursor.is_empty() {
            return Err(Error::InvalidResponse);
        }
    }
    Ok(output)
}

fn validate_scopes<'a>(scopes: impl IntoIterator<Item = &'a str>) -> Result<Vec<String>> {
    let scopes: Vec<_> = scopes.into_iter().map(str::to_owned).collect();
    if scopes.len() > 32
        || scopes.iter().any(|scope| {
            scope.is_empty()
                || scope.len() > 200
                || !scope
                    .bytes()
                    .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b'"' | b'\\'))
        })
    {
        return Err(Error::InvalidResponse);
    }
    Ok(scopes)
}

fn validate_client_id(client_id: &str) -> Result<()> {
    if client_id.is_empty()
        || client_id.len() > 2048
        || client_id
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err(Error::InvalidInput);
    }
    Ok(())
}

fn validate_token(token: &str) -> Result<()> {
    if token.is_empty() || token.len() > 8192 || !token.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(Error::InvalidResponse);
    }
    Ok(())
}

fn unique_parameter(
    pairs: &[(std::borrow::Cow<'_, str>, std::borrow::Cow<'_, str>)],
    name: &str,
) -> Result<String> {
    let values: Vec<_> = pairs
        .iter()
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value.to_string())
        .collect();
    if values.len() == 1 {
        Ok(values[0].clone())
    } else {
        Err(Error::Denied)
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn secure_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).map_err(|_| Error::InvalidResponse)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::InvalidResponse);
    }
    Ok(url)
}

fn canonical_resource(url: &Url) -> String {
    let mut normalized = url.clone();
    normalized.set_fragment(None);
    if normalized.path() == "/" {
        normalized.set_path("");
    }
    normalized.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn metadata() -> AuthorizationServerMetadata {
        AuthorizationServerMetadata {
            issuer: "https://auth.example.com/tenant".into(),
            authorization_endpoint: "https://auth.example.com/authorize".into(),
            token_endpoint: "https://auth.example.com/token".into(),
            code_challenge_methods_supported: vec!["S256".into()],
            scopes_supported: vec!["read".into()],
            registration_endpoint: Some("https://auth.example.com/register".into()),
            client_id_metadata_document_supported: true,
        }
    }

    #[test]
    fn parses_challenges_and_builds_required_discovery_order() {
        let endpoint = Url::parse("https://mcp.example.com/public/mcp").unwrap();
        let challenge = OAuthChallenge::parse(
            "Bearer resource_metadata=\"https://mcp.example.com/meta\", scope=\"files:read profile\", error=\"insufficient_scope\"",
        )
        .unwrap();
        assert_eq!(challenge.scopes, ["files:read", "profile"]);
        assert!(challenge.insufficient_scope);
        assert_eq!(
            protected_resource_metadata_urls(&endpoint, Some(&challenge)).unwrap(),
            [Url::parse("https://mcp.example.com/meta").unwrap()]
        );
        assert_eq!(
            protected_resource_metadata_urls(&endpoint, None).unwrap(),
            [
                Url::parse(
                    "https://mcp.example.com/.well-known/oauth-protected-resource/public/mcp"
                )
                .unwrap(),
                Url::parse("https://mcp.example.com/.well-known/oauth-protected-resource").unwrap(),
            ]
        );
        assert_eq!(
            authorization_server_metadata_urls(
                &Url::parse("https://auth.example.com/tenant").unwrap()
            )
            .unwrap(),
            [
                Url::parse(
                    "https://auth.example.com/.well-known/oauth-authorization-server/tenant"
                )
                .unwrap(),
                Url::parse("https://auth.example.com/.well-known/openid-configuration/tenant")
                    .unwrap(),
                Url::parse("https://auth.example.com/tenant/.well-known/openid-configuration")
                    .unwrap(),
            ]
        );
    }

    #[test]
    fn metadata_and_registration_fail_closed() {
        let endpoint = Url::parse("https://mcp.example.com/mcp").unwrap();
        let resource = ProtectedResourceMetadata {
            resource: endpoint.to_string(),
            authorization_servers: vec!["https://auth.example.com/tenant".into()],
            scopes_supported: vec!["read".into()],
        };
        assert_eq!(resource.validate(&endpoint), Ok(()));
        let mut wrong = resource.clone();
        wrong.resource = "https://other.example.com/mcp".into();
        assert_eq!(wrong.validate(&endpoint), Err(Error::InvalidResponse));

        let metadata = metadata();
        let extensible: AuthorizationServerMetadata = serde_json::from_value(json!({
            "issuer":"https://auth.example.com/tenant",
            "authorization_endpoint":"https://auth.example.com/authorize",
            "token_endpoint":"https://auth.example.com/token",
            "code_challenge_methods_supported":["S256"],
            "revocation_endpoint":"https://auth.example.com/revoke"
        }))
        .unwrap();
        assert_eq!(
            extensible.validate(&Url::parse(&extensible.issuer).unwrap()),
            Ok(())
        );
        assert_eq!(
            ClientRegistration::select(&metadata, Some("registered-client"), None).unwrap(),
            ClientRegistration::PreRegistered {
                client_id: "registered-client".into()
            }
        );
        assert_eq!(
            ClientRegistration::select(
                &metadata,
                None,
                Some("https://app.example.com/oauth/client.json")
            )
            .unwrap(),
            ClientRegistration::ClientIdMetadataDocument {
                client_id: "https://app.example.com/oauth/client.json".into()
            }
        );
        let mut no_pkce = metadata;
        no_pkce.code_challenge_methods_supported.clear();
        assert_eq!(
            no_pkce.validate(&Url::parse(&no_pkce.issuer).unwrap()),
            Err(Error::InvalidResponse)
        );
    }

    #[test]
    fn pkce_callback_is_bound_expiring_and_single_use() {
        let endpoint = Url::parse("https://mcp.example.com/mcp").unwrap();
        let mut transaction = AuthorizationTransaction::begin(
            "connection",
            &endpoint,
            &metadata(),
            "registered-client",
            "https://app.example.com/oauth/callback",
            &["read".into()],
            1_000,
        )
        .unwrap();
        let authorization = Url::parse(&transaction.authorization_url).unwrap();
        let params: std::collections::BTreeMap<_, _> = authorization.query_pairs().collect();
        assert_eq!(params["code_challenge_method"], "S256");
        assert_eq!(params["resource"], endpoint.as_str());
        assert!(!params["code_challenge"].contains('='));
        assert!(!format!("{transaction:?}").contains(&transaction.state));

        let wrong = AuthorizationCallback {
            url: format!(
                "https://app.example.com/oauth/callback?code=code&state={}x",
                transaction.state
            ),
        };
        assert!(matches!(
            transaction.accept_callback(&wrong, 1_001),
            Err(Error::Denied)
        ));
        let callback = AuthorizationCallback {
            url: format!(
                "https://app.example.com/oauth/callback?code=code&state={}",
                transaction.state
            ),
        };
        let exchange = transaction.accept_callback(&callback, 1_001).unwrap();
        assert_eq!(exchange.resource, endpoint.as_str());
        assert!(!format!("{exchange:?}").contains(&exchange.code_verifier));
        assert!(matches!(
            transaction.accept_callback(&callback, 1_001),
            Err(Error::Denied)
        ));

        let mut expired = AuthorizationTransaction::begin(
            "connection",
            &endpoint,
            &metadata(),
            "registered-client",
            "https://app.example.com/oauth/callback",
            &[],
            1_000,
        )
        .unwrap();
        assert!(matches!(
            expired.accept_callback(&callback, 1_601),
            Err(Error::Denied)
        ));
    }

    #[test]
    fn duplicate_or_cross_origin_challenges_are_rejected() {
        assert_eq!(
            OAuthChallenge::parse("Bearer scope=\"read\", scope=\"write\""),
            Err(Error::InvalidResponse)
        );
        let endpoint = Url::parse("https://mcp.example.com/mcp").unwrap();
        let challenge = OAuthChallenge {
            resource_metadata: Some("https://attacker.example/meta".into()),
            scopes: vec![],
            insufficient_scope: false,
        };
        assert_eq!(
            protected_resource_metadata_urls(&endpoint, Some(&challenge)),
            Err(Error::Denied)
        );
    }

    #[test]
    fn token_responses_are_redacted_bounded_and_rotate_refresh_tokens() {
        let tokens = TokenResponse {
            access_token: "access-secret".into(),
            token_type: "Bearer".into(),
            expires_in: Some(300),
            refresh_token: Some("rotated-refresh-secret".into()),
            scope: Some("read write".into()),
        }
        .into_tokens(1_000, Some("old-refresh-secret".into()))
        .unwrap();
        assert_eq!(tokens.expires_at, Some(1_300));
        assert_eq!(
            tokens.refresh_token.as_deref(),
            Some("rotated-refresh-secret")
        );
        assert_eq!(tokens.granted_scopes, ["read", "write"]);
        let debug = format!("{tokens:?}");
        assert!(!debug.contains("access-secret"));
        assert!(!debug.contains("rotated-refresh-secret"));

        let preserved = TokenResponse {
            access_token: "next-access".into(),
            token_type: "bearer".into(),
            expires_in: None,
            refresh_token: None,
            scope: None,
        }
        .into_tokens(1_000, Some("existing-refresh".into()))
        .unwrap();
        assert_eq!(preserved.refresh_token.as_deref(), Some("existing-refresh"));
        assert!(matches!(
            TokenResponse {
                access_token: "value".into(),
                token_type: "mac".into(),
                expires_in: None,
                refresh_token: None,
                scope: None,
            }
            .into_tokens(0, None),
            Err(Error::InvalidResponse)
        ));
    }
}
