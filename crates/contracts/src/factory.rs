use crate::auth::{AuthError, ResolvedCredential};
use crate::credential::CredentialPurpose;
use crate::provider::{AuthKind, AuthOptionSpec, ProviderDefinition};
use crate::vault::{SecretVault, VaultError};

/// Errors from the provider factory.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum FactoryError {
    #[error("Provider not found: {0}")]
    NotFound(String),
    #[error("Auth error: {0}")]
    Auth(#[from] AuthError),
    #[error("Provider unavailable: {0}")]
    Unavailable(String),
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

/// A resolved model client ready to make inference calls.
/// Contains only vendor-neutral types; no Rig/provider SDK objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModelClient {
    pub provider_id: String,
    pub base_url: String,
    pub auth_header: ResolvedCredential,
    pub extra_headers: Vec<ResolvedCredential>,
    pub model: String,
}

/// Maps a provider definition + vault credential into a resolved model client.
pub struct ProviderFactory {
    vault: Box<dyn SecretVault>,
}

impl ProviderFactory {
    pub fn new(vault: Box<dyn SecretVault>) -> Self {
        Self { vault }
    }

    pub fn resolve_client(
        &self,
        provider: &ProviderDefinition,
        base_url: &str,
        model: &str,
        auth_option: &AuthOptionSpec,
        header_name: Option<&str>,
    ) -> Result<ResolvedModelClient, FactoryError> {
        let purpose = CredentialPurpose::ProviderAuth;
        let auth_kind = auth_option.auth_kind;

        let auth_header = if auth_kind == AuthKind::None {
            ResolvedCredential {
                header_name: String::new(),
                header_value: String::new(),
            }
        } else {
            let secret = self
                .vault
                .get(&provider.id, purpose)
                .map_err(|e| match e {
                    VaultError::NotFound => AuthError::NotFound,
                    VaultError::Locked => AuthError::VaultLocked,
                    _ => AuthError::NotFound,
                })?
                .ok_or(AuthError::NotFound)?;

            match auth_kind {
                AuthKind::ApiKey => {
                    let name = header_name
                        .or(auth_option.wire_header.as_deref())
                        .unwrap_or("x-api-key")
                        .to_owned();
                    let value = match auth_option.wire_prefix.as_deref() {
                        Some(prefix) => format!("{prefix}{secret}"),
                        None => secret,
                    };
                    ResolvedCredential {
                        header_name: name,
                        header_value: value,
                    }
                }
                AuthKind::BearerToken => {
                    let name = header_name
                        .or(auth_option.wire_header.as_deref())
                        .unwrap_or("authorization")
                        .to_owned();
                    let prefix = auth_option.wire_prefix.as_deref().unwrap_or("Bearer ");
                    ResolvedCredential {
                        header_name: name,
                        header_value: format!("{prefix}{secret}"),
                    }
                }
                AuthKind::CustomCompatible => ResolvedCredential {
                    header_name: header_name
                        .or(auth_option.wire_header.as_deref())
                        .ok_or(AuthError::MissingHeaderName)?
                        .to_owned(),
                    header_value: secret,
                },
                other => return Err(AuthError::UnsupportedKind(format!("{other:?}")).into()),
            }
        };

        Ok(ResolvedModelClient {
            provider_id: provider.id.clone(),
            base_url: base_url.to_owned(),
            auth_header,
            extra_headers: auth_option
                .extra_headers
                .iter()
                .map(|header| ResolvedCredential {
                    header_name: header.name.clone(),
                    header_value: header.value.clone(),
                })
                .collect(),
            model: model.to_owned(),
        })
    }

    pub fn has_credential(&self, provider_id: &str) -> bool {
        self.vault.has(provider_id, CredentialPurpose::ProviderAuth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::*;
    use crate::vault::TestVault;

    fn test_provider() -> ProviderDefinition {
        ProviderDefinition {
            schema: PROVIDER_CATALOG_SCHEMA_V1.into(),
            id: "nvidia-nim".into(),
            display_name: "NVIDIA NIM".into(),
            transport_family: TransportFamily::OpenAiCompatible,
            capabilities: ProviderCapabilities {
                streaming: true,
                tool_calls: true,
                vision: false,
                max_context_tokens: Some(4096),
            },
            endpoint_fields: vec![],
            model_source: ModelSource::UserSpecified,
            auth_options: vec![AuthOptionSpec {
                id: "api_key".into(),
                label: "API Key".into(),
                auth_kind: AuthKind::ApiKey,
                wire_header: Some("authorization".into()),
                wire_prefix: Some("Bearer ".into()),
                extra_headers: vec![],
                fields: vec![ConfigFieldSpec {
                    id: "api_key".into(),
                    label: "API Key".into(),
                    kind: ConfigFieldKind::Secret,
                    required: true,
                    secret: true,
                    validation: None,
                    options: vec![],
                    visible_when: vec![],
                    help_text: None,
                }],
                expiry_behavior: ExpiryBehavior::NeverExpires,
                refresh_behavior: RefreshBehavior::NotRefreshable,
                android_support: AndroidSupport::FullySupported,
            }],
            availability: ProviderAvailability::Available,
            documentation_url: None,
        }
    }

    fn setup_factory() -> ProviderFactory {
        let vault = TestVault::new();
        vault
            .put(
                "nvidia-nim",
                CredentialPurpose::ProviderAuth,
                "sk-nim-key-12345678",
            )
            .unwrap();
        ProviderFactory::new(Box::new(vault))
    }

    #[test]
    fn resolve_api_key_client() {
        let factory = setup_factory();
        let provider = test_provider();
        let option = provider.auth_options[0].clone();
        let client = factory
            .resolve_client(
                &provider,
                "https://integrate.api.nvidia.com/v1",
                "meta/llama-3.1-8b-instruct",
                &option,
                None,
            )
            .unwrap();
        assert_eq!(client.provider_id, "nvidia-nim");
        assert_eq!(client.base_url, "https://integrate.api.nvidia.com/v1");
        assert_eq!(client.auth_header.header_name, "authorization");
        assert_eq!(
            client.auth_header.header_value,
            "Bearer sk-nim-key-12345678"
        );
        assert!(client.extra_headers.is_empty());
        assert_eq!(client.model, "meta/llama-3.1-8b-instruct");
    }

    #[test]
    fn resolve_bearer_client() {
        let factory = setup_factory();
        let mut provider = test_provider();
        provider.auth_options[0].auth_kind = AuthKind::BearerToken;
        let option = provider.auth_options[0].clone();
        let client = factory
            .resolve_client(
                &provider,
                "https://integrate.api.nvidia.com/v1",
                "meta/llama-3.1-8b-instruct",
                &option,
                None,
            )
            .unwrap();
        assert_eq!(
            client.auth_header.header_value,
            "Bearer sk-nim-key-12345678"
        );
    }

    #[test]
    fn missing_credential_returns_auth_error() {
        let vault = TestVault::new();
        let factory = ProviderFactory::new(Box::new(vault));
        let provider = test_provider();
        let option = provider.auth_options[0].clone();
        let result =
            factory.resolve_client(&provider, "https://example.com/v1", "model", &option, None);
        assert!(matches!(
            result,
            Err(FactoryError::Auth(AuthError::NotFound))
        ));
    }

    #[test]
    fn has_credential_check() {
        let factory = setup_factory();
        assert!(factory.has_credential("nvidia-nim"));
        assert!(!factory.has_credential("nonexistent"));
    }

    #[test]
    fn openai_compatible_resolves_all_auth_modes() {
        use crate::catalog_seeds::openai_compatible_definition;
        let definition = openai_compatible_definition();
        let vault = TestVault::new();
        vault
            .put(
                "openai-compatible",
                CredentialPurpose::ProviderAuth,
                "custom-secret",
            )
            .unwrap();
        let factory = ProviderFactory::new(Box::new(vault));
        let base = "https://custom.example.com/v1";
        let option_by_kind = |kind| {
            definition
                .auth_options
                .iter()
                .find(|option| option.auth_kind == kind)
                .unwrap()
                .clone()
        };
        let none = factory
            .resolve_client(
                &definition,
                base,
                "custom-model",
                &option_by_kind(AuthKind::None),
                None,
            )
            .unwrap();
        assert!(none.auth_header.header_name.is_empty());
        let key = factory
            .resolve_client(
                &definition,
                base,
                "custom-model",
                &option_by_kind(AuthKind::ApiKey),
                None,
            )
            .unwrap();
        assert_eq!(key.auth_header.header_name, "authorization");
        assert_eq!(key.auth_header.header_value, "Bearer custom-secret");
        let bearer = factory
            .resolve_client(
                &definition,
                base,
                "custom-model",
                &option_by_kind(AuthKind::BearerToken),
                None,
            )
            .unwrap();
        assert_eq!(bearer.auth_header.header_value, "Bearer custom-secret");
        let custom = factory
            .resolve_client(
                &definition,
                base,
                "custom-model",
                &option_by_kind(AuthKind::CustomCompatible),
                Some("x-custom-auth"),
            )
            .unwrap();
        assert_eq!(custom.auth_header.header_name, "x-custom-auth");
        assert_eq!(custom.auth_header.header_value, "custom-secret");
    }

    #[test]
    fn anthropic_wire_sends_version_header() {
        use crate::catalog_seeds::anthropic_definition;
        let definition = anthropic_definition();
        let option = definition.auth_options[0].clone();
        let vault = TestVault::new();
        vault
            .put("anthropic", CredentialPurpose::ProviderAuth, "ant-key")
            .unwrap();
        let factory = ProviderFactory::new(Box::new(vault));
        let client = factory
            .resolve_client(
                &definition,
                "https://api.anthropic.com",
                "claude-3",
                &option,
                None,
            )
            .unwrap();
        assert_eq!(client.auth_header.header_name, "x-api-key");
        assert_eq!(client.auth_header.header_value, "ant-key");
        assert_eq!(client.extra_headers.len(), 1);
        assert_eq!(client.extra_headers[0].header_name, "anthropic-version");
        assert_eq!(client.extra_headers[0].header_value, "2023-06-01");
    }
}
