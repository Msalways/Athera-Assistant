use assistant_contracts::{Error, Result};
use async_trait::async_trait;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Url,
};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum McpAuthentication {
    #[default]
    None,
    BearerToken {
        secret_ref: String,
    },
    ApiKeyHeader {
        header: String,
        secret_ref: String,
    },
    OauthAuthorizationCode {
        token_ref: String,
        authorization_server: String,
        resource: String,
        #[serde(default)]
        requested_scopes: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CredentialPurpose {
    BearerToken,
    ApiKeyHeader,
    OauthAccessToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialRequest {
    pub connection_id: String,
    pub origin: String,
    pub resource: String,
    pub purpose: CredentialPurpose,
    pub secret_ref: String,
}

#[async_trait]
pub trait CredentialResolver: Send + Sync {
    async fn prepare(&self, _request: &CredentialRequest) -> Result<()> {
        Ok(())
    }

    fn resolve(&self, request: &CredentialRequest) -> Result<String>;
}

#[derive(Default)]
pub struct MissingCredentials;

#[async_trait]
impl CredentialResolver for MissingCredentials {
    fn resolve(&self, _request: &CredentialRequest) -> Result<String> {
        Err(Error::AuthRequired)
    }
}

pub(crate) struct ResolvedAuth {
    pub bearer: Option<String>,
    pub headers: HeaderMap,
}

impl fmt::Debug for ResolvedAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedAuth")
            .field("bearer", &self.bearer.as_ref().map(|_| "[REDACTED]"))
            .field("header_count", &self.headers.len())
            .finish()
    }
}

impl McpAuthentication {
    pub fn requires_auth(&self) -> bool {
        !matches!(self, Self::None)
    }

    pub fn credential_reference(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::BearerToken { secret_ref } | Self::ApiKeyHeader { secret_ref, .. } => {
                Some(secret_ref)
            }
            Self::OauthAuthorizationCode { token_ref, .. } => Some(token_ref),
        }
    }

    pub(crate) fn validate(&self, endpoint: &Url) -> Result<()> {
        match self {
            Self::None => Ok(()),
            Self::BearerToken { secret_ref } => validate_reference(secret_ref),
            Self::ApiKeyHeader { header, secret_ref } => {
                validate_reference(secret_ref)?;
                validate_api_key_header(header)
            }
            Self::OauthAuthorizationCode {
                token_ref,
                authorization_server,
                resource,
                requested_scopes,
            } => {
                validate_reference(token_ref)?;
                let _authorization_server = secure_url(authorization_server)?;
                let resource = secure_url(resource)?;
                if resource != *endpoint
                    || requested_scopes.len() > 32
                    || requested_scopes.iter().any(|scope| {
                        scope.is_empty()
                            || scope.len() > 200
                            || !scope.bytes().all(|byte| {
                                byte.is_ascii_graphic() && !matches!(byte, b'"' | b'\\')
                            })
                    })
                {
                    return Err(Error::InvalidInput);
                }
                Ok(())
            }
        }
    }

    pub(crate) fn resolve(
        &self,
        connection_id: &str,
        endpoint: &Url,
        resolver: &dyn CredentialResolver,
    ) -> Result<ResolvedAuth> {
        let Some(request) = self.credential_request(connection_id, endpoint)? else {
            return Ok(ResolvedAuth {
                bearer: None,
                headers: HeaderMap::new(),
            });
        };
        let value = resolver.resolve(&request)?;
        validate_secret(&value)?;
        match self {
            Self::BearerToken { .. } | Self::OauthAuthorizationCode { .. } => Ok(ResolvedAuth {
                bearer: Some(value),
                headers: HeaderMap::new(),
            }),
            Self::ApiKeyHeader { header, .. } => {
                let name =
                    HeaderName::from_bytes(header.as_bytes()).map_err(|_| Error::InvalidInput)?;
                let value = HeaderValue::from_str(&value).map_err(|_| Error::InvalidInput)?;
                let mut headers = HeaderMap::new();
                headers.insert(name, value);
                Ok(ResolvedAuth {
                    bearer: None,
                    headers,
                })
            }
            Self::None => Err(Error::InvalidInput),
        }
    }

    pub fn credential_request(
        &self,
        connection_id: &str,
        endpoint: &Url,
    ) -> Result<Option<CredentialRequest>> {
        self.validate(endpoint)?;
        let Some(secret_ref) = self.credential_reference() else {
            return Ok(None);
        };
        let (resource, purpose) = match self {
            Self::BearerToken { .. } => {
                (endpoint.as_str().to_owned(), CredentialPurpose::BearerToken)
            }
            Self::ApiKeyHeader { .. } => (
                endpoint.as_str().to_owned(),
                CredentialPurpose::ApiKeyHeader,
            ),
            Self::OauthAuthorizationCode { resource, .. } => {
                (resource.clone(), CredentialPurpose::OauthAccessToken)
            }
            Self::None => return Ok(None),
        };
        Ok(Some(CredentialRequest {
            connection_id: connection_id.to_owned(),
            origin: origin(endpoint)?,
            resource,
            purpose,
            secret_ref: secret_ref.to_owned(),
        }))
    }
}

fn validate_reference(reference: &str) -> Result<()> {
    if reference.is_empty()
        || reference.len() > 200
        || !reference
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':'))
    {
        return Err(Error::InvalidInput);
    }
    Ok(())
}

fn validate_secret(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 8192 || !value.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(Error::InvalidInput);
    }
    Ok(())
}

fn validate_api_key_header(header: &str) -> Result<()> {
    let name = HeaderName::from_bytes(header.as_bytes()).map_err(|_| Error::InvalidInput)?;
    let lower = name.as_str();
    if !matches!(lower, "x-api-key" | "api-key") {
        return Err(Error::InvalidInput);
    }
    Ok(())
}

fn secure_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).map_err(|_| Error::InvalidInput)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::InvalidInput);
    }
    Ok(url)
}

pub(crate) fn origin(url: &Url) -> Result<String> {
    if url.host_str().is_none() {
        return Err(Error::InvalidInput);
    }
    Ok(url.origin().ascii_serialization())
}
