use crate::provider::{AuthKind, ProviderDefinition, TransportFamily};
use serde::{Deserialize, Serialize};

pub const PROVIDER_CATALOG_REGISTRY_V1: &str = "aethra.provider-catalog-registry.v1";

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum CatalogError {
    #[error("Duplicate provider id: {0}")]
    DuplicateProvider(String),
    #[error("Unsafe field id: {0}")]
    UnsafeField(String),
    #[error("Unsupported auth combination for provider {provider}: {reason}")]
    UnsupportedAuthCombination { provider: String, reason: String },
    #[error("Invalid provider: {0}")]
    InvalidProvider(String),
    #[error("Provider not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderCatalog {
    pub schema: String,
    pub providers: Vec<ProviderDefinition>,
}

impl ProviderCatalog {
    pub fn new() -> Self {
        Self {
            schema: PROVIDER_CATALOG_REGISTRY_V1.into(),
            providers: Vec::new(),
        }
    }

    pub fn register(&mut self, provider: ProviderDefinition) -> Result<(), CatalogError> {
        provider
            .validate()
            .map_err(|e| CatalogError::InvalidProvider(e.to_string()))?;

        if self.providers.iter().any(|p| p.id == provider.id) {
            return Err(CatalogError::DuplicateProvider(provider.id));
        }

        self.providers.push(provider);
        Ok(())
    }

    pub fn validate(&self) -> Result<(), CatalogError> {
        let mut ids: Vec<String> = Vec::new();
        for provider in &self.providers {
            if ids.contains(&provider.id) {
                return Err(CatalogError::DuplicateProvider(provider.id.clone()));
            }
            ids.push(provider.id.clone());

            Self::validate_auth_combinations(provider)?;
        }
        Ok(())
    }

    fn validate_auth_combinations(provider: &ProviderDefinition) -> Result<(), CatalogError> {
        let has_oauth = provider
            .auth_options
            .iter()
            .any(|a| a.auth_kind == AuthKind::OAuth2Pkce);
        let has_none = provider
            .auth_options
            .iter()
            .any(|a| a.auth_kind == AuthKind::None);
        if has_oauth && has_none {
            return Err(CatalogError::UnsupportedAuthCombination {
                provider: provider.id.clone(),
                reason: "OAuth and no-auth cannot coexist".into(),
            });
        }
        Ok(())
    }

    pub fn get(&self, provider_id: &str) -> Option<&ProviderDefinition> {
        self.providers.iter().find(|p| p.id == provider_id)
    }

    pub fn providers_for_transport(&self, transport: TransportFamily) -> Vec<&ProviderDefinition> {
        self.providers
            .iter()
            .filter(|p| p.transport_family == transport)
            .collect()
    }

    pub fn api_key_providers(&self) -> Vec<&ProviderDefinition> {
        self.providers
            .iter()
            .filter(|p| {
                p.auth_options
                    .iter()
                    .any(|a| a.auth_kind == AuthKind::ApiKey)
            })
            .collect()
    }
}

impl Default for ProviderCatalog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::*;

    fn openai_compat(id: &str) -> ProviderDefinition {
        ProviderDefinition {
            schema: PROVIDER_CATALOG_SCHEMA_V1.into(),
            id: id.into(),
            display_name: id.into(),
            transport_family: TransportFamily::OpenAiCompatible,
            capabilities: ProviderCapabilities {
                streaming: true,
                tool_calls: true,
                vision: false,
                max_context_tokens: None,
            },
            endpoint_fields: vec![],
            model_source: ModelSource::UserSpecified,
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
            documentation_url: None,
        }
    }

    #[test]
    fn register_and_lookup() {
        let mut catalog = ProviderCatalog::new();
        let p = openai_compat("openai");
        catalog.register(p).unwrap();
        assert!(catalog.get("openai").is_some());
        assert!(catalog.get("nonexistent").is_none());
    }

    #[test]
    fn duplicate_id_rejected() {
        let mut catalog = ProviderCatalog::new();
        catalog.register(openai_compat("openai")).unwrap();
        let dup = catalog.register(openai_compat("openai"));
        assert!(matches!(dup, Err(CatalogError::DuplicateProvider(_))));
    }

    #[test]
    fn validate_catches_duplicates() {
        let mut catalog = ProviderCatalog::new();
        catalog.register(openai_compat("a")).unwrap();
        catalog.providers.push(openai_compat("a"));
        assert!(matches!(
            catalog.validate(),
            Err(CatalogError::DuplicateProvider(_))
        ));
    }

    #[test]
    fn filter_by_transport() {
        let mut catalog = ProviderCatalog::new();
        catalog.register(openai_compat("openai")).unwrap();
        catalog.register(openai_compat("deepseek")).unwrap();
        let openai_only = catalog.providers_for_transport(TransportFamily::OpenAiCompatible);
        assert_eq!(openai_only.len(), 2);
    }

    #[test]
    fn api_key_providers() {
        let mut catalog = ProviderCatalog::new();
        catalog.register(openai_compat("openai")).unwrap();
        let keys = catalog.api_key_providers();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].id, "openai");
    }
}
