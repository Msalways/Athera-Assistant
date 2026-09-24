//! Vendor-neutral provider catalog descriptors.
//! These types describe providers, auth options, and configuration fields
//! without leaking vendor objects or executing code.
use serde::{Deserialize, Serialize};

/// Schema version for provider descriptors.
pub const PROVIDER_CATALOG_SCHEMA_V1: &str = "aethra.provider-catalog.v1";

/// Describes a model provider available to the runtime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderDefinition {
    pub schema: String,
    pub id: String,
    pub display_name: String,
    pub transport_family: TransportFamily,
    pub capabilities: ProviderCapabilities,
    pub endpoint_fields: Vec<ConfigFieldSpec>,
    pub model_source: ModelSource,
    pub auth_options: Vec<AuthOptionSpec>,
    pub availability: ProviderAvailability,
    pub documentation_url: Option<String>,
    pub default_base_url: Option<String>,
}

impl ProviderDefinition {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.id.is_empty() {
            return Err("provider id is required");
        }
        if self.display_name.is_empty() {
            return Err("display_name is required");
        }
        if self.auth_options.is_empty() {
            return Err("at least one auth option is required");
        }
        if let Some(default) = self.default_base_url.as_deref() {
            normalize_endpoint(Some(default), None)?;
        }
        for auth in &self.auth_options {
            auth.validate()?;
        }
        for field in &self.endpoint_fields {
            field.validate()?;
        }
        Ok(())
    }
}

/// Resolve the request endpoint from stored config with the catalog default
/// as fallback. Trims whitespace and trailing slashes; requires an
/// http(s) URL without credentials, query or fragment so profiles cannot
/// smuggle request modifiers into the base URL.
pub fn normalize_endpoint(
    raw: Option<&str>,
    default_base_url: Option<&str>,
) -> Result<String, &'static str> {
    let trimmed = raw.map(str::trim).unwrap_or("");
    let fallback = default_base_url.map(str::trim).unwrap_or("");
    let candidate = if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    };
    if candidate.is_empty() {
        return Err("endpoint is required");
    }
    let lower = candidate.to_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err("endpoint must start with http:// or https://");
    }
    let after_scheme = candidate
        .split_once("://")
        .map(|(_, after)| after)
        .ok_or("endpoint must start with http:// or https://")?;
    if after_scheme.is_empty() {
        return Err("endpoint host is required");
    }
    if after_scheme.contains('@') || candidate.contains('?') || candidate.contains('#') {
        return Err("endpoint must not contain credentials, query or fragment");
    }
    Ok(candidate.trim_end_matches('/').to_owned())
}

/// Resolve the model id from stored config. Every chat definition carries a
/// `model` endpoint field; Azure additionally accepts `deployment` as the
/// model name so the form does not ask twice.
pub fn resolve_model(config: &serde_json::Value) -> Result<String, &'static str> {
    for key in ["model", "deployment"] {
        if let Some(model) = config.get(key).and_then(|value| value.as_str()) {
            if !model.trim().is_empty() {
                return Ok(model.trim().to_owned());
            }
        }
    }
    Err("model is required")
}

/// Transport families supported by the provider catalog.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransportFamily {
    OpenAiCompatible,
    AnthropicCompatible,
    AwsBedrock,
    GoogleVertexAi,
    GoogleGemini,
    Custom,
}

/// Whether the provider is available in this build.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAvailability {
    Available,
    RequiresConfiguration,
    Unsupported,
    DisabledByFeature,
}

/// Capabilities advertised by a provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub streaming: bool,
    pub tool_calls: bool,
    pub vision: bool,
    pub max_context_tokens: Option<u32>,
}

/// How the provider resolves model identifiers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelSource {
    /// Models listed by the provider (e.g., OpenAI models endpoint).
    Catalog,
    /// User specifies a model ID directly (e.g., custom endpoint).
    UserSpecified,
    /// Model is fixed at compile time (e.g., on-device).
    Fixed,
}

/// Describes one authentication method a provider supports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthOptionSpec {
    pub id: String,
    pub label: String,
    pub auth_kind: AuthKind,
    pub fields: Vec<ConfigFieldSpec>,
    pub expiry_behavior: ExpiryBehavior,
    pub refresh_behavior: RefreshBehavior,
    pub android_support: AndroidSupport,
    pub wire_header: Option<String>,
    pub wire_prefix: Option<String>,
    pub extra_headers: Vec<HeaderPair>,
}

/// A fixed header always sent with this auth option.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeaderPair {
    pub name: String,
    pub value: String,
}

impl AuthOptionSpec {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.id.is_empty() {
            return Err("auth option id is required");
        }
        if self.label.is_empty() {
            return Err("auth option label is required");
        }
        for field in &self.fields {
            field.validate()?;
        }
        for header in &self.extra_headers {
            if header.name.is_empty() {
                return Err("extra header name is required");
            }
        }
        Ok(())
    }
}

/// The kind of authentication material.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    None,
    ApiKey,
    BearerToken,
    CompositeStaticSecret,
    OAuth2Pkce,
    WorkloadIdentity,
    CloudIdentity,
    CustomCompatible,
}

/// Whether the credential expires and how.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExpiryBehavior {
    NeverExpires,
    ExpiresAt(u64),
    UserMustRotate,
}

/// Whether the credential can be refreshed.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RefreshBehavior {
    NotRefreshable,
    AutomaticRefresh,
    ManualReconnect,
}

/// Whether the auth method works on Android.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AndroidSupport {
    FullySupported,
    PartiallySupported,
    Unsupported,
    Unknown,
}

/// A configuration field for provider setup or auth.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigFieldSpec {
    pub id: String,
    pub label: String,
    pub kind: ConfigFieldKind,
    pub required: bool,
    pub secret: bool,
    pub validation: Option<ConfigValidation>,
    pub options: Vec<String>,
    pub visible_when: Vec<VisibilityRule>,
    pub help_text: Option<String>,
}

impl ConfigFieldSpec {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.id.is_empty() {
            return Err("field id is required");
        }
        if self.label.is_empty() {
            return Err("field label is required");
        }
        Ok(())
    }
}

/// The type of value a configuration field accepts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFieldKind {
    Text,
    Secret,
    Select,
    Boolean,
    Url,
    Integer,
}

/// Optional validation constraints for a field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigValidation {
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub pattern: Option<String>,
    pub min_value: Option<i64>,
    pub max_value: Option<i64>,
}

/// A field is visible only when another field equals a specific value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VisibilityRule {
    pub field_id: String,
    pub equals: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_catalog_schema_roundtrip() {
        let p = ProviderDefinition {
            schema: PROVIDER_CATALOG_SCHEMA_V1.into(),
            id: "test-provider".into(),
            display_name: "Test".into(),
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
                wire_header: None,
                wire_prefix: None,
                extra_headers: vec![],
            }],
            availability: ProviderAvailability::Available,
            documentation_url: None,
            default_base_url: None,
        };
        p.validate().unwrap();
        let json = serde_json::to_string(&p).unwrap();
        let decoded: ProviderDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(p, decoded);
    }

    #[test]
    fn validation_rejects_empty_id() {
        let p = ProviderDefinition {
            schema: PROVIDER_CATALOG_SCHEMA_V1.into(),
            id: "".into(),
            display_name: "Test".into(),
            transport_family: TransportFamily::OpenAiCompatible,
            capabilities: ProviderCapabilities {
                streaming: false,
                tool_calls: false,
                vision: false,
                max_context_tokens: None,
            },
            endpoint_fields: vec![],
            model_source: ModelSource::UserSpecified,
            auth_options: vec![AuthOptionSpec {
                id: "key".into(),
                label: "Key".into(),
                auth_kind: AuthKind::ApiKey,
                fields: vec![],
                expiry_behavior: ExpiryBehavior::NeverExpires,
                refresh_behavior: RefreshBehavior::NotRefreshable,
                android_support: AndroidSupport::FullySupported,
                wire_header: Some("x-api-key".into()),
                wire_prefix: None,
                extra_headers: vec![],
            }],
            availability: ProviderAvailability::Available,
            documentation_url: None,
            default_base_url: None,
        };
        assert_eq!(p.validate(), Err("provider id is required"));
    }

    #[test]
    fn visibility_rule_controls_field_rendering() {
        let field = ConfigFieldSpec {
            id: "region".into(),
            label: "AWS Region".into(),
            kind: ConfigFieldKind::Select,
            required: true,
            secret: false,
            validation: None,
            options: vec!["us-east-1".into(), "eu-west-1".into()],
            visible_when: vec![VisibilityRule {
                field_id: "auth_kind".into(),
                equals: "cloud_identity".into(),
            }],
            help_text: Some("Select the AWS region for Bedrock.".into()),
        };
        field.validate().unwrap();
        assert_eq!(field.visible_when.len(), 1);
        assert_eq!(field.visible_when[0].field_id, "auth_kind");
    }

    #[test]
    fn normalize_endpoint_prefers_config_over_default() {
        assert_eq!(
            normalize_endpoint(
                Some("https://custom.example.com/v1/"),
                Some("https://api.openai.com/v1")
            ),
            Ok("https://custom.example.com/v1".into())
        );
        assert_eq!(
            normalize_endpoint(None, Some("https://api.openai.com/v1")),
            Ok("https://api.openai.com/v1".into())
        );
        assert_eq!(
            normalize_endpoint(Some("  http://localhost:11434/v1  "), None),
            Ok("http://localhost:11434/v1".into())
        );
    }

    #[test]
    fn normalize_endpoint_rejects_bad_shapes() {
        assert_eq!(normalize_endpoint(None, None), Err("endpoint is required"));
        assert_eq!(
            normalize_endpoint(Some(""), Some("  ")),
            Err("endpoint is required")
        );
        assert_eq!(
            normalize_endpoint(Some("api.openai.com/v1"), None),
            Err("endpoint must start with http:// or https://")
        );
        assert_eq!(
            normalize_endpoint(Some("https://"), None),
            Err("endpoint host is required")
        );
        assert_eq!(
            normalize_endpoint(Some("https://user:key@api.example.com/v1"), None),
            Err("endpoint must not contain credentials, query or fragment")
        );
        assert_eq!(
            normalize_endpoint(Some("https://api.example.com/v1?key=x"), None),
            Err("endpoint must not contain credentials, query or fragment")
        );
        assert_eq!(
            normalize_endpoint(Some("https://api.example.com/v1#frag"), None),
            Err("endpoint must not contain credentials, query or fragment")
        );
    }

    #[test]
    fn resolve_model_reads_model_then_deployment() {
        assert_eq!(
            resolve_model(&serde_json::json!({"model": " gpt-4o "})),
            Ok("gpt-4o".into())
        );
        assert_eq!(
            resolve_model(&serde_json::json!({"deployment": "my-deploy"})),
            Ok("my-deploy".into())
        );
        assert_eq!(
            resolve_model(&serde_json::json!({"model": ""})),
            Err("model is required")
        );
        assert_eq!(
            resolve_model(&serde_json::json!({})),
            Err("model is required")
        );
    }
}
