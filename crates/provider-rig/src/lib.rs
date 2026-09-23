use assistant_contracts::factory::{FactoryError, ProviderFactory, ResolvedModelClient};
use assistant_contracts::provider::AuthKind;

pub mod convert;

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum RigAdapterError {
    #[error("Factory error: {0}")]
    Factory(#[from] FactoryError),
    #[error("Request failed: {0}")]
    RequestFailed(String),
}

pub struct RigProviderAdapter {
    factory: ProviderFactory,
}

impl RigProviderAdapter {
    pub fn new(factory: ProviderFactory) -> Self {
        Self { factory }
    }

    pub fn build_client(
        &self,
        provider_id: &str,
        providers: &[assistant_contracts::provider::ProviderDefinition],
        base_url: &str,
        model: &str,
        auth_kind: AuthKind,
        header_name: Option<&str>,
    ) -> Result<ResolvedModelClient, RigAdapterError> {
        let provider = providers
            .iter()
            .find(|p| p.id == provider_id)
            .ok_or_else(|| FactoryError::NotFound(provider_id.to_string()))?;
        let option = provider
            .auth_options
            .iter()
            .find(|option| option.auth_kind == auth_kind)
            .ok_or_else(|| {
                FactoryError::InvalidConfig(format!("no {auth_kind:?} option for {provider_id}"))
            })?;

        Ok(self
            .factory
            .resolve_client(provider, base_url, model, option, header_name)?)
    }

    pub fn completion_url(base_url: &str) -> String {
        format!("{base_url}/chat/completions")
    }

    pub fn build_headers(client: &ResolvedModelClient) -> Vec<(String, String)> {
        let mut headers = vec![];
        if !client.auth_header.header_name.is_empty() {
            headers.push((
                client.auth_header.header_name.clone(),
                client.auth_header.header_value.clone(),
            ));
        }
        for extra in &client.extra_headers {
            headers.push((extra.header_name.clone(), extra.header_value.clone()));
        }
        headers.push(("content-type".into(), "application/json".into()));
        headers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assistant_contracts::credential::CredentialPurpose;
    use assistant_contracts::provider::*;
    use assistant_contracts::vault::{SecretVault, TestVault};

    fn test_provider() -> ProviderDefinition {
        ProviderDefinition {
            schema: PROVIDER_CATALOG_SCHEMA_V1.into(),
            id: "openai".into(),
            display_name: "OpenAI".into(),
            transport_family: TransportFamily::OpenAiCompatible,
            capabilities: ProviderCapabilities {
                streaming: true,
                tool_calls: true,
                vision: true,
                max_context_tokens: Some(128_000),
            },
            endpoint_fields: vec![],
            model_source: ModelSource::Catalog,
            auth_options: vec![AuthOptionSpec {
                id: "api_key".into(),
                label: "API Key".into(),
                auth_kind: AuthKind::ApiKey,
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
                wire_header: Some("authorization".into()),
                wire_prefix: Some("Bearer ".into()),
                extra_headers: vec![],
            }],
            availability: ProviderAvailability::Available,
            documentation_url: Some("https://platform.openai.com/docs".into()),
        }
    }

    fn setup_adapter() -> RigProviderAdapter {
        let vault = TestVault::new();
        vault
            .put("openai", CredentialPurpose::ProviderAuth, "sk-test-key")
            .unwrap();
        RigProviderAdapter::new(ProviderFactory::new(Box::new(vault)))
    }

    #[test]
    fn build_client_from_provider() {
        let adapter = setup_adapter();
        let client = adapter
            .build_client(
                "openai",
                &[test_provider()],
                "https://api.openai.com/v1",
                "gpt-4o",
                AuthKind::ApiKey,
                None,
            )
            .unwrap();
        assert_eq!(client.provider_id, "openai");
        assert_eq!(client.base_url, "https://api.openai.com/v1");
        assert_eq!(client.auth_header.header_name, "authorization");
        assert_eq!(client.auth_header.header_value, "Bearer sk-test-key");
        assert_eq!(client.model, "gpt-4o");
    }

    #[test]
    fn missing_provider_returns_not_found() {
        let adapter = setup_adapter();
        let result = adapter.build_client(
            "nonexistent",
            &[test_provider()],
            "https://api.openai.com/v1",
            "gpt-4o",
            AuthKind::ApiKey,
            None,
        );
        assert!(matches!(
            result,
            Err(RigAdapterError::Factory(FactoryError::NotFound(_)))
        ));
    }

    #[test]
    fn completion_url_is_correct() {
        assert_eq!(
            RigProviderAdapter::completion_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn build_headers_includes_auth_and_content_type() {
        let adapter = setup_adapter();
        let client = adapter
            .build_client(
                "openai",
                &[test_provider()],
                "https://api.openai.com/v1",
                "gpt-4o",
                AuthKind::ApiKey,
                None,
            )
            .unwrap();
        let headers = RigProviderAdapter::build_headers(&client);
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0].0, "authorization");
        assert_eq!(headers[1].0, "content-type");
    }

    #[test]
    fn missing_auth_option_returns_invalid_config() {
        let adapter = setup_adapter();
        let result = adapter.build_client(
            "openai",
            &[test_provider()],
            "https://api.openai.com/v1",
            "gpt-4o",
            AuthKind::BearerToken,
            None,
        );
        assert!(matches!(
            result,
            Err(RigAdapterError::Factory(FactoryError::InvalidConfig(_)))
        ));
    }
}
