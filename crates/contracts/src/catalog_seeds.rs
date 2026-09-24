use crate::catalog::{CatalogError, ProviderCatalog};
use crate::provider::*;

fn api_key_option() -> AuthOptionSpec {
    AuthOptionSpec {
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
    }
}

fn anthropic_key_option() -> AuthOptionSpec {
    AuthOptionSpec {
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
        wire_header: Some("x-api-key".into()),
        wire_prefix: None,
        extra_headers: vec![HeaderPair {
            name: "anthropic-version".into(),
            value: "2023-06-01".into(),
        }],
    }
}

fn google_key_option() -> AuthOptionSpec {
    let mut option = api_key_option();
    option.wire_header = Some("x-goog-api-key".into());
    option.wire_prefix = None;
    option
}

fn azure_key_option() -> AuthOptionSpec {
    let mut option = api_key_option();
    option.wire_header = Some("api-key".into());
    option.wire_prefix = None;
    option
}

fn base_url_field(required: bool, help: &str) -> ConfigFieldSpec {
    ConfigFieldSpec {
        id: "base_url".into(),
        label: "Base URL".into(),
        kind: ConfigFieldKind::Url,
        required,
        secret: false,
        validation: None,
        options: vec![],
        visible_when: vec![],
        help_text: Some(help.into()),
    }
}

fn model_field(help: &str) -> ConfigFieldSpec {
    ConfigFieldSpec {
        id: "model".into(),
        label: "Model".into(),
        kind: ConfigFieldKind::Text,
        required: true,
        secret: false,
        validation: None,
        options: vec![],
        visible_when: vec![],
        help_text: Some(help.into()),
    }
}

fn openai_shaped(
    id: &str,
    display: &str,
    docs: &str,
    vision: bool,
    max_context: Option<u32>,
    default_base_url: Option<&str>,
) -> ProviderDefinition {
    ProviderDefinition {
        schema: PROVIDER_CATALOG_SCHEMA_V1.into(),
        id: id.into(),
        display_name: display.into(),
        transport_family: TransportFamily::OpenAiCompatible,
        capabilities: ProviderCapabilities {
            streaming: true,
            tool_calls: true,
            vision,
            max_context_tokens: max_context,
        },
        endpoint_fields: vec![
            base_url_field(default_base_url.is_none(), "Override the default endpoint"),
            model_field("Model ID, e.g. the provider's chat model name"),
        ],
        model_source: ModelSource::UserSpecified,
        auth_options: vec![api_key_option()],
        availability: ProviderAvailability::Available,
        documentation_url: Some(docs.into()),
        default_base_url: default_base_url.map(str::to_owned),
    }
}

pub fn openai_definition() -> ProviderDefinition {
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
        endpoint_fields: vec![
            base_url_field(false, "Defaults to https://api.openai.com/v1"),
            model_field("Model ID, e.g. gpt-4o"),
        ],
        model_source: ModelSource::Catalog,
        auth_options: vec![api_key_option()],
        availability: ProviderAvailability::Available,
        documentation_url: Some("https://platform.openai.com/docs".into()),
        default_base_url: Some("https://api.openai.com/v1".into()),
    }
}

pub fn openai_compatible_definition() -> ProviderDefinition {
    let secret_field = |id: &str, label: &str| ConfigFieldSpec {
        id: id.into(),
        label: label.into(),
        kind: ConfigFieldKind::Secret,
        required: true,
        secret: true,
        validation: None,
        options: vec![],
        visible_when: vec![],
        help_text: None,
    };
    ProviderDefinition {
        schema: PROVIDER_CATALOG_SCHEMA_V1.into(),
        id: "openai-compatible".into(),
        display_name: "OpenAI-Compatible (Custom)".into(),
        transport_family: TransportFamily::OpenAiCompatible,
        capabilities: ProviderCapabilities {
            streaming: true,
            tool_calls: true,
            vision: false,
            max_context_tokens: None,
        },
        endpoint_fields: vec![
            ConfigFieldSpec {
                id: "base_url".into(),
                label: "Base URL".into(),
                kind: ConfigFieldKind::Url,
                required: true,
                secret: false,
                validation: None,
                options: vec![],
                visible_when: vec![],
                help_text: Some("Custom OpenAI-compatible endpoint".into()),
            },
            model_field("Model ID served by the custom endpoint"),
        ],
        model_source: ModelSource::UserSpecified,
        auth_options: vec![
            AuthOptionSpec {
                id: "none".into(),
                label: "No Auth".into(),
                auth_kind: AuthKind::None,
                fields: vec![],
                expiry_behavior: ExpiryBehavior::NeverExpires,
                refresh_behavior: RefreshBehavior::NotRefreshable,
                android_support: AndroidSupport::FullySupported,
                wire_header: None,
                wire_prefix: None,
                extra_headers: vec![],
            },
            AuthOptionSpec {
                id: "api_key".into(),
                label: "API Key".into(),
                auth_kind: AuthKind::ApiKey,
                fields: vec![secret_field("api_key", "API Key")],
                expiry_behavior: ExpiryBehavior::NeverExpires,
                refresh_behavior: RefreshBehavior::NotRefreshable,
                android_support: AndroidSupport::FullySupported,
                wire_header: Some("authorization".into()),
                wire_prefix: Some("Bearer ".into()),
                extra_headers: vec![],
            },
            AuthOptionSpec {
                id: "bearer".into(),
                label: "Bearer Token".into(),
                auth_kind: AuthKind::BearerToken,
                fields: vec![secret_field("token", "Bearer Token")],
                expiry_behavior: ExpiryBehavior::NeverExpires,
                refresh_behavior: RefreshBehavior::NotRefreshable,
                android_support: AndroidSupport::FullySupported,
                wire_header: Some("authorization".into()),
                wire_prefix: Some("Bearer ".into()),
                extra_headers: vec![],
            },
            AuthOptionSpec {
                id: "custom_header".into(),
                label: "Custom Header".into(),
                auth_kind: AuthKind::CustomCompatible,
                fields: vec![
                    ConfigFieldSpec {
                        id: "header_name".into(),
                        label: "Header Name".into(),
                        kind: ConfigFieldKind::Text,
                        required: true,
                        secret: false,
                        validation: None,
                        options: vec![],
                        visible_when: vec![],
                        help_text: None,
                    },
                    secret_field("header_value", "Header Value"),
                ],
                expiry_behavior: ExpiryBehavior::NeverExpires,
                refresh_behavior: RefreshBehavior::NotRefreshable,
                android_support: AndroidSupport::FullySupported,
                wire_header: None,
                wire_prefix: None,
                extra_headers: vec![],
            },
        ],
        availability: ProviderAvailability::Available,
        documentation_url: None,
        default_base_url: None,
    }
}

pub fn nvidia_definition() -> ProviderDefinition {
    ProviderDefinition {
        schema: PROVIDER_CATALOG_SCHEMA_V1.into(),
        id: "nvidia-nim".into(),
        display_name: "NVIDIA NIM".into(),
        transport_family: TransportFamily::OpenAiCompatible,
        capabilities: ProviderCapabilities {
            streaming: true,
            tool_calls: true,
            vision: false,
            max_context_tokens: None,
        },
        endpoint_fields: vec![
            base_url_field(false, "Defaults to https://integrate.api.nvidia.com/v1"),
            model_field("NIM model ID, e.g. meta/llama-3.1-8b-instruct"),
        ],
        model_source: ModelSource::UserSpecified,
        auth_options: vec![api_key_option()],
        availability: ProviderAvailability::Available,
        documentation_url: Some("https://docs.api.nvidia.com/nim".into()),
        default_base_url: Some("https://integrate.api.nvidia.com/v1".into()),
    }
}

pub fn anthropic_definition() -> ProviderDefinition {
    ProviderDefinition {
        schema: PROVIDER_CATALOG_SCHEMA_V1.into(),
        id: "anthropic".into(),
        display_name: "Anthropic".into(),
        transport_family: TransportFamily::AnthropicCompatible,
        capabilities: ProviderCapabilities {
            streaming: true,
            tool_calls: true,
            vision: true,
            max_context_tokens: Some(200_000),
        },
        endpoint_fields: vec![model_field("Model ID, e.g. claude-sonnet-4-5")],
        model_source: ModelSource::Catalog,
        auth_options: vec![anthropic_key_option()],
        availability: ProviderAvailability::Available,
        documentation_url: Some("https://docs.anthropic.com".into()),
        default_base_url: Some("https://api.anthropic.com".into()),
    }
}

pub fn anthropic_compatible_definition() -> ProviderDefinition {
    let secret_field = |id: &str, label: &str| ConfigFieldSpec {
        id: id.into(),
        label: label.into(),
        kind: ConfigFieldKind::Secret,
        required: true,
        secret: true,
        validation: None,
        options: vec![],
        visible_when: vec![],
        help_text: None,
    };
    ProviderDefinition {
        schema: PROVIDER_CATALOG_SCHEMA_V1.into(),
        id: "anthropic-compatible".into(),
        display_name: "Anthropic-Compatible (Custom)".into(),
        transport_family: TransportFamily::AnthropicCompatible,
        capabilities: ProviderCapabilities {
            streaming: true,
            tool_calls: true,
            vision: false,
            max_context_tokens: None,
        },
        endpoint_fields: vec![
            ConfigFieldSpec {
                id: "base_url".into(),
                label: "Base URL".into(),
                kind: ConfigFieldKind::Url,
                required: true,
                secret: false,
                validation: None,
                options: vec![],
                visible_when: vec![],
                help_text: Some("Custom Anthropic-compatible endpoint".into()),
            },
            ConfigFieldSpec {
                id: "api_version".into(),
                label: "API Version".into(),
                kind: ConfigFieldKind::Text,
                required: false,
                secret: false,
                validation: None,
                options: vec![],
                visible_when: vec![],
                help_text: Some("Defaults to 2023-06-01".into()),
            },
            model_field("Model ID served by the custom endpoint"),
        ],
        model_source: ModelSource::UserSpecified,
        auth_options: vec![
            AuthOptionSpec {
                id: "api_key".into(),
                label: "API Key".into(),
                auth_kind: AuthKind::ApiKey,
                fields: vec![secret_field("api_key", "API Key")],
                expiry_behavior: ExpiryBehavior::NeverExpires,
                refresh_behavior: RefreshBehavior::NotRefreshable,
                android_support: AndroidSupport::FullySupported,
                wire_header: Some("x-api-key".into()),
                wire_prefix: None,
                extra_headers: vec![HeaderPair {
                    name: "anthropic-version".into(),
                    value: "2023-06-01".into(),
                }],
            },
            AuthOptionSpec {
                id: "bearer".into(),
                label: "Bearer Token".into(),
                auth_kind: AuthKind::BearerToken,
                fields: vec![secret_field("token", "Bearer Token")],
                expiry_behavior: ExpiryBehavior::NeverExpires,
                refresh_behavior: RefreshBehavior::NotRefreshable,
                android_support: AndroidSupport::FullySupported,
                wire_header: Some("authorization".into()),
                wire_prefix: Some("Bearer ".into()),
                extra_headers: vec![HeaderPair {
                    name: "anthropic-version".into(),
                    value: "2023-06-01".into(),
                }],
            },
            AuthOptionSpec {
                id: "custom_header".into(),
                label: "Custom Header".into(),
                auth_kind: AuthKind::CustomCompatible,
                fields: vec![
                    ConfigFieldSpec {
                        id: "header_name".into(),
                        label: "Header Name".into(),
                        kind: ConfigFieldKind::Text,
                        required: true,
                        secret: false,
                        validation: None,
                        options: vec![],
                        visible_when: vec![],
                        help_text: None,
                    },
                    secret_field("header_value", "Header Value"),
                ],
                expiry_behavior: ExpiryBehavior::NeverExpires,
                refresh_behavior: RefreshBehavior::NotRefreshable,
                android_support: AndroidSupport::FullySupported,
                wire_header: None,
                wire_prefix: None,
                extra_headers: vec![HeaderPair {
                    name: "anthropic-version".into(),
                    value: "2023-06-01".into(),
                }],
            },
        ],
        availability: ProviderAvailability::Available,
        documentation_url: None,
        default_base_url: None,
    }
}

pub fn azure_definition() -> ProviderDefinition {
    let required_text = |id: &str, label: &str, help: &str| ConfigFieldSpec {
        id: id.into(),
        label: label.into(),
        kind: ConfigFieldKind::Text,
        required: true,
        secret: false,
        validation: None,
        options: vec![],
        visible_when: vec![],
        help_text: Some(help.into()),
    };
    ProviderDefinition {
        schema: PROVIDER_CATALOG_SCHEMA_V1.into(),
        id: "azure-openai".into(),
        display_name: "Azure OpenAI".into(),
        transport_family: TransportFamily::OpenAiCompatible,
        capabilities: ProviderCapabilities {
            streaming: true,
            tool_calls: true,
            vision: true,
            max_context_tokens: None,
        },
        endpoint_fields: vec![
            required_text(
                "endpoint",
                "Endpoint",
                "Resource endpoint like https://my-resource.openai.azure.com",
            ),
            required_text("deployment", "Deployment", "Deployment name for the model"),
            required_text("api_version", "API Version", "API version like 2024-10-21"),
        ],
        model_source: ModelSource::UserSpecified,
        auth_options: vec![azure_key_option()],
        availability: ProviderAvailability::Available,
        documentation_url: Some("https://learn.microsoft.com/azure/ai-services/openai".into()),
        default_base_url: None,
    }
}

pub fn gemini_definition() -> ProviderDefinition {
    ProviderDefinition {
        schema: PROVIDER_CATALOG_SCHEMA_V1.into(),
        id: "gemini".into(),
        display_name: "Google Gemini".into(),
        transport_family: TransportFamily::GoogleGemini,
        capabilities: ProviderCapabilities {
            streaming: true,
            tool_calls: true,
            vision: true,
            max_context_tokens: Some(1_000_000),
        },
        endpoint_fields: vec![model_field("Model ID, e.g. gemini-2.0-flash")],
        model_source: ModelSource::Catalog,
        auth_options: vec![google_key_option()],
        availability: ProviderAvailability::Available,
        documentation_url: Some("https://ai.google.dev/gemini-api/docs".into()),
        default_base_url: Some("https://generativelanguage.googleapis.com/v1beta".into()),
    }
}

pub fn bedrock_definition() -> ProviderDefinition {
    ProviderDefinition {
        schema: PROVIDER_CATALOG_SCHEMA_V1.into(),
        id: "aws-bedrock".into(),
        display_name: "AWS Bedrock".into(),
        transport_family: TransportFamily::AwsBedrock,
        capabilities: ProviderCapabilities {
            streaming: true,
            tool_calls: true,
            vision: true,
            max_context_tokens: None,
        },
        endpoint_fields: vec![
            ConfigFieldSpec {
                id: "region".into(),
                label: "AWS Region".into(),
                kind: ConfigFieldKind::Text,
                required: true,
                secret: false,
                validation: None,
                options: vec![],
                visible_when: vec![],
                help_text: Some("Bedrock region like us-east-1".into()),
            },
            model_field("Bedrock model ID"),
        ],
        model_source: ModelSource::UserSpecified,
        auth_options: vec![
            AuthOptionSpec {
                id: "sigv4".into(),
                label: "AWS Credentials (SigV4)".into(),
                auth_kind: AuthKind::CloudIdentity,
                fields: vec![
                    ConfigFieldSpec {
                        id: "access_key_id".into(),
                        label: "Access Key ID".into(),
                        kind: ConfigFieldKind::Text,
                        required: true,
                        secret: false,
                        validation: None,
                        options: vec![],
                        visible_when: vec![],
                        help_text: None,
                    },
                    ConfigFieldSpec {
                        id: "secret_access_key".into(),
                        label: "Secret Access Key".into(),
                        kind: ConfigFieldKind::Secret,
                        required: true,
                        secret: true,
                        validation: None,
                        options: vec![],
                        visible_when: vec![],
                        help_text: None,
                    },
                    ConfigFieldSpec {
                        id: "session_token".into(),
                        label: "Session Token".into(),
                        kind: ConfigFieldKind::Secret,
                        required: false,
                        secret: true,
                        validation: None,
                        options: vec![],
                        visible_when: vec![],
                        help_text: Some(
                            "Only for temporary credentials; signed into each request".into(),
                        ),
                    },
                ],
                expiry_behavior: ExpiryBehavior::NeverExpires,
                refresh_behavior: RefreshBehavior::ManualReconnect,
                android_support: AndroidSupport::PartiallySupported,
                wire_header: None,
                wire_prefix: None,
                extra_headers: vec![],
            },
            AuthOptionSpec {
                id: "bedrock_api_key".into(),
                label: "Bedrock API Key (Bearer)".into(),
                auth_kind: AuthKind::BearerToken,
                fields: vec![ConfigFieldSpec {
                    id: "api_key".into(),
                    label: "API Key".into(),
                    kind: ConfigFieldKind::Secret,
                    required: true,
                    secret: true,
                    validation: None,
                    options: vec![],
                    visible_when: vec![],
                    help_text: Some(
                        "Short-term keys last up to 12 hours; long-term keys carry a configured expiry. Sent as Authorization: Bearer."
                            .into(),
                    ),
                }],
                expiry_behavior: ExpiryBehavior::NeverExpires,
                refresh_behavior: RefreshBehavior::ManualReconnect,
                android_support: AndroidSupport::PartiallySupported,
                wire_header: Some("authorization".into()),
                wire_prefix: Some("Bearer ".into()),
                extra_headers: vec![],
            },
        ],
        availability: ProviderAvailability::DisabledByFeature,
        documentation_url: Some("https://docs.aws.amazon.com/bedrock".into()),
        default_base_url: None,
    }
}

pub fn vertex_definition() -> ProviderDefinition {
    ProviderDefinition {
        schema: PROVIDER_CATALOG_SCHEMA_V1.into(),
        id: "google-vertex".into(),
        display_name: "Google Vertex AI".into(),
        transport_family: TransportFamily::GoogleVertexAi,
        capabilities: ProviderCapabilities {
            streaming: true,
            tool_calls: true,
            vision: true,
            max_context_tokens: None,
        },
        endpoint_fields: vec![
            ConfigFieldSpec {
                id: "project".into(),
                label: "Project".into(),
                kind: ConfigFieldKind::Text,
                required: true,
                secret: false,
                validation: None,
                options: vec![],
                visible_when: vec![],
                help_text: None,
            },
            ConfigFieldSpec {
                id: "location".into(),
                label: "Location".into(),
                kind: ConfigFieldKind::Text,
                required: true,
                secret: false,
                validation: None,
                options: vec![],
                visible_when: vec![],
                help_text: None,
            },
            model_field("Vertex model ID"),
        ],
        model_source: ModelSource::UserSpecified,
        auth_options: vec![AuthOptionSpec {
            id: "google_identity".into(),
            label: "Google Identity".into(),
            auth_kind: AuthKind::CloudIdentity,
            fields: vec![],
            expiry_behavior: ExpiryBehavior::NeverExpires,
            refresh_behavior: RefreshBehavior::AutomaticRefresh,
            android_support: AndroidSupport::PartiallySupported,
            wire_header: None,
            wire_prefix: None,
            extra_headers: vec![],
        }],
        availability: ProviderAvailability::DisabledByFeature,
        documentation_url: Some("https://cloud.google.com/vertex-ai".into()),
        default_base_url: None,
    }
}

pub fn ollama_definition() -> ProviderDefinition {
    ProviderDefinition {
        schema: PROVIDER_CATALOG_SCHEMA_V1.into(),
        id: "ollama".into(),
        display_name: "Ollama (Local)".into(),
        transport_family: TransportFamily::OpenAiCompatible,
        capabilities: ProviderCapabilities {
            streaming: true,
            tool_calls: false,
            vision: false,
            max_context_tokens: None,
        },
        endpoint_fields: vec![
            base_url_field(false, "Defaults to http://localhost:11434/v1"),
            model_field("Ollama model tag, e.g. llama3.1"),
        ],
        model_source: ModelSource::UserSpecified,
        auth_options: vec![AuthOptionSpec {
            id: "none".into(),
            label: "No Auth".into(),
            auth_kind: AuthKind::None,
            fields: vec![],
            expiry_behavior: ExpiryBehavior::NeverExpires,
            refresh_behavior: RefreshBehavior::NotRefreshable,
            android_support: AndroidSupport::FullySupported,
            wire_header: None,
            wire_prefix: None,
            extra_headers: vec![],
        }],
        availability: ProviderAvailability::Available,
        documentation_url: Some("https://ollama.com".into()),
        default_base_url: Some("http://localhost:11434/v1".into()),
    }
}

pub fn chatgpt_definition() -> ProviderDefinition {
    ProviderDefinition {
        schema: PROVIDER_CATALOG_SCHEMA_V1.into(),
        id: "chatgpt".into(),
        display_name: "ChatGPT (OAuth)".into(),
        transport_family: TransportFamily::OpenAiCompatible,
        capabilities: ProviderCapabilities {
            streaming: true,
            tool_calls: true,
            vision: false,
            max_context_tokens: None,
        },
        endpoint_fields: vec![model_field("ChatGPT model")],
        model_source: ModelSource::UserSpecified,
        auth_options: vec![AuthOptionSpec {
            id: "oauth".into(),
            label: "ChatGPT OAuth".into(),
            auth_kind: AuthKind::OAuth2Pkce,
            fields: vec![],
            expiry_behavior: ExpiryBehavior::NeverExpires,
            refresh_behavior: RefreshBehavior::AutomaticRefresh,
            android_support: AndroidSupport::PartiallySupported,
            wire_header: Some("authorization".into()),
            wire_prefix: Some("Bearer ".into()),
            extra_headers: vec![],
        }],
        availability: ProviderAvailability::DisabledByFeature,
        documentation_url: Some("https://help.openai.com".into()),
        default_base_url: Some("https://api.openai.com/v1".into()),
    }
}

pub fn copilot_definition() -> ProviderDefinition {
    ProviderDefinition {
        schema: PROVIDER_CATALOG_SCHEMA_V1.into(),
        id: "copilot".into(),
        display_name: "GitHub Copilot (OAuth)".into(),
        transport_family: TransportFamily::OpenAiCompatible,
        capabilities: ProviderCapabilities {
            streaming: true,
            tool_calls: true,
            vision: false,
            max_context_tokens: None,
        },
        endpoint_fields: vec![model_field("Copilot model")],
        model_source: ModelSource::UserSpecified,
        auth_options: vec![AuthOptionSpec {
            id: "oauth".into(),
            label: "GitHub Device Flow".into(),
            auth_kind: AuthKind::OAuth2Pkce,
            fields: vec![],
            expiry_behavior: ExpiryBehavior::NeverExpires,
            refresh_behavior: RefreshBehavior::AutomaticRefresh,
            android_support: AndroidSupport::PartiallySupported,
            wire_header: Some("authorization".into()),
            wire_prefix: Some("Bearer ".into()),
            extra_headers: vec![HeaderPair {
                name: "openai-intent".into(),
                value: "conversation-panel".into(),
            }],
        }],
        availability: ProviderAvailability::DisabledByFeature,
        documentation_url: Some("https://docs.github.com/copilot".into()),
        default_base_url: Some("https://api.githubcopilot.com".into()),
    }
}

pub fn cohere_definition() -> ProviderDefinition {
    let mut definition = openai_shaped(
        "cohere",
        "Cohere",
        "https://docs.cohere.com",
        false,
        None,
        Some("https://api.cohere.com/compatibility/v1"),
    );
    definition.transport_family = TransportFamily::Custom;
    definition.display_name = "Cohere (Native)".into();
    definition.availability = ProviderAvailability::DisabledByFeature;
    definition
}

pub fn llamacpp_definition() -> ProviderDefinition {
    ProviderDefinition {
        schema: PROVIDER_CATALOG_SCHEMA_V1.into(),
        id: "llamacpp".into(),
        display_name: "llama.cpp (Local)".into(),
        transport_family: TransportFamily::OpenAiCompatible,
        capabilities: ProviderCapabilities {
            streaming: true,
            tool_calls: false,
            vision: false,
            max_context_tokens: None,
        },
        endpoint_fields: vec![
            base_url_field(false, "Defaults to http://localhost:8080/v1 (llama-server)"),
            model_field("llama-server model alias"),
        ],
        model_source: ModelSource::UserSpecified,
        auth_options: vec![AuthOptionSpec {
            id: "none".into(),
            label: "No Auth".into(),
            auth_kind: AuthKind::None,
            fields: vec![],
            expiry_behavior: ExpiryBehavior::NeverExpires,
            refresh_behavior: RefreshBehavior::NotRefreshable,
            android_support: AndroidSupport::FullySupported,
            wire_header: None,
            wire_prefix: None,
            extra_headers: vec![],
        }],
        availability: ProviderAvailability::DisabledByFeature,
        documentation_url: Some("https://github.com/ggerganov/llama.cpp".into()),
        default_base_url: Some("http://localhost:8080/v1".into()),
    }
}

pub fn voyage_definition() -> ProviderDefinition {
    let mut definition = openai_shaped(
        "voyageai",
        "Voyage AI",
        "https://docs.voyageai.com",
        false,
        None,
        Some("https://api.voyageai.com/v1"),
    );
    definition.display_name = "Voyage AI (Embeddings)".into();
    definition.capabilities.tool_calls = false;
    definition.capabilities.streaming = false;
    definition.availability = ProviderAvailability::DisabledByFeature;
    definition
}

fn bulk_openai_shaped() -> Vec<ProviderDefinition> {
    [
        (
            "deepseek",
            "DeepSeek",
            "https://api-docs.deepseek.com",
            false,
            None,
            Some("https://api.deepseek.com/v1"),
        ),
        (
            "groq",
            "Groq",
            "https://console.groq.com/docs",
            false,
            None,
            Some("https://api.groq.com/openai/v1"),
        ),
        (
            "mistral",
            "Mistral",
            "https://docs.mistral.ai",
            true,
            Some(128_000),
            Some("https://api.mistral.ai/v1"),
        ),
        (
            "together",
            "Together",
            "https://docs.together.ai",
            true,
            Some(128_000),
            Some("https://api.together.xyz/v1"),
        ),
        (
            "openrouter",
            "OpenRouter",
            "https://openrouter.ai/docs",
            true,
            None,
            Some("https://openrouter.ai/api/v1"),
        ),
        (
            "perplexity",
            "Perplexity",
            "https://docs.perplexity.ai",
            false,
            None,
            Some("https://api.perplexity.ai"),
        ),
        (
            "xai",
            "xAI",
            "https://docs.x.ai",
            true,
            Some(128_000),
            Some("https://api.x.ai/v1"),
        ),
        (
            "huggingface",
            "Hugging Face",
            "https://huggingface.co/docs",
            false,
            None,
            Some("https://router.huggingface.co/v1"),
        ),
        (
            "hyperbolic",
            "Hyperbolic",
            "https://docs.hyperbolic.xyz",
            false,
            None,
            Some("https://api.hyperbolic.xyz/v1"),
        ),
        (
            "minimax",
            "MiniMax",
            "https://platform.minimaxi.com",
            false,
            None,
            Some("https://api.minimaxi.com/v1"),
        ),
        (
            "moonshot",
            "Moonshot",
            "https://platform.moonshot.ai/docs",
            false,
            None,
            Some("https://api.moonshot.ai/v1"),
        ),
        (
            "venice",
            "Venice",
            "https://docs.venice.ai",
            false,
            None,
            Some("https://api.venice.ai/api/v1"),
        ),
        (
            "xiaomimimo",
            "Xiaomi MiMo",
            "https://platform.mimo.ai",
            false,
            None,
            None,
        ),
        (
            "zai",
            "Z.ai",
            "https://docs.z.ai",
            false,
            None,
            Some("https://open.bigmodel.cn/api/paas/v4"),
        ),
        (
            "mira",
            "Mira",
            "https://docs.mira.network",
            false,
            None,
            None,
        ),
        (
            "doubleword",
            "Doubleword",
            "https://docs.doubleword.ai",
            false,
            None,
            None,
        ),
    ]
    .into_iter()
    .map(
        |(id, display, docs, vision, max_context, default_base_url)| {
            openai_shaped(id, display, docs, vision, max_context, default_base_url)
        },
    )
    .collect()
}

pub fn default_catalog() -> Result<ProviderCatalog, CatalogError> {
    let mut catalog = ProviderCatalog::new();
    catalog.register(openai_definition())?;
    catalog.register(openai_compatible_definition())?;
    catalog.register(nvidia_definition())?;
    catalog.register(anthropic_definition())?;
    catalog.register(anthropic_compatible_definition())?;
    catalog.register(azure_definition())?;
    catalog.register(gemini_definition())?;
    catalog.register(ollama_definition())?;
    catalog.register(bedrock_definition())?;
    catalog.register(vertex_definition())?;
    catalog.register(chatgpt_definition())?;
    catalog.register(copilot_definition())?;
    catalog.register(cohere_definition())?;
    catalog.register(llamacpp_definition())?;
    catalog.register(voyage_definition())?;
    for definition in bulk_openai_shaped() {
        catalog.register(definition)?;
    }
    catalog.validate()?;
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::PROVIDER_CATALOG_REGISTRY_V1;

    #[test]
    fn default_catalog_registers_all_providers() {
        let catalog = default_catalog().unwrap();
        for id in [
            "openai",
            "openai-compatible",
            "nvidia-nim",
            "anthropic",
            "anthropic-compatible",
            "azure-openai",
            "gemini",
            "ollama",
            "deepseek",
            "groq",
            "mistral",
            "together",
            "openrouter",
            "perplexity",
            "xai",
            "huggingface",
            "hyperbolic",
            "minimax",
            "moonshot",
            "venice",
            "xiaomimimo",
            "zai",
            "mira",
            "doubleword",
            "aws-bedrock",
            "google-vertex",
            "chatgpt",
            "copilot",
            "cohere",
            "llamacpp",
            "voyageai",
        ] {
            assert!(catalog.get(id).is_some(), "missing provider {id}");
        }
        assert_eq!(catalog.providers.len(), 31);
    }

    #[test]
    fn nvidia_uses_nim_endpoint() {
        let definition = nvidia_definition();
        definition.validate().unwrap();
        assert_eq!(
            definition.transport_family,
            TransportFamily::OpenAiCompatible
        );
        assert_eq!(definition.model_source, ModelSource::UserSpecified);
    }

    #[test]
    fn azure_requires_deployment_fields() {
        let definition = azure_definition();
        definition.validate().unwrap();
        for id in ["endpoint", "deployment", "api_version"] {
            assert!(definition
                .endpoint_fields
                .iter()
                .any(|field| field.id == id && field.required));
        }
    }

    #[test]
    fn anthropic_uses_native_transport() {
        let definition = anthropic_definition();
        definition.validate().unwrap();
        assert_eq!(
            definition.transport_family,
            TransportFamily::AnthropicCompatible
        );
    }

    #[test]
    fn anthropic_compatible_covers_key_bearer_and_custom_header() {
        let definition = anthropic_compatible_definition();
        definition.validate().unwrap();
        assert_eq!(
            definition.transport_family,
            TransportFamily::AnthropicCompatible
        );
        assert_eq!(definition.model_source, ModelSource::UserSpecified);
        assert!(definition
            .endpoint_fields
            .iter()
            .any(|field| field.id == "base_url" && field.required));
        let kinds: Vec<AuthKind> = definition
            .auth_options
            .iter()
            .map(|option| option.auth_kind)
            .collect();
        assert!(kinds.contains(&AuthKind::ApiKey));
        assert!(kinds.contains(&AuthKind::BearerToken));
        assert!(kinds.contains(&AuthKind::CustomCompatible));
        for option in &definition.auth_options {
            assert!(
                option.extra_headers.iter().any(
                    |header| header.name == "anthropic-version" && header.value == "2023-06-01"
                ),
                "missing version header on {}",
                option.id
            );
        }
    }

    #[test]
    fn ollama_needs_no_auth() {
        let definition = ollama_definition();
        definition.validate().unwrap();
        assert!(definition
            .auth_options
            .iter()
            .any(|option| option.auth_kind == AuthKind::None));
    }

    #[test]
    fn bedrock_supports_sigv4_and_bearer_key() {
        let definition = bedrock_definition();
        definition.validate().unwrap();
        assert_eq!(definition.transport_family, TransportFamily::AwsBedrock);
        assert_eq!(
            definition.availability,
            ProviderAvailability::DisabledByFeature
        );
        let sigv4 = definition
            .auth_options
            .iter()
            .find(|option| option.id == "sigv4")
            .unwrap();
        assert_eq!(sigv4.auth_kind, AuthKind::CloudIdentity);
        assert!(sigv4
            .fields
            .iter()
            .any(|field| field.id == "session_token" && !field.required));
        let bearer = definition
            .auth_options
            .iter()
            .find(|option| option.id == "bedrock_api_key")
            .unwrap();
        assert_eq!(bearer.auth_kind, AuthKind::BearerToken);
        assert!(bearer
            .fields
            .iter()
            .any(|field| field.id == "api_key" && field.required));
    }

    #[test]
    fn vertex_requires_project_location() {
        let definition = vertex_definition();
        definition.validate().unwrap();
        assert_eq!(definition.transport_family, TransportFamily::GoogleVertexAi);
        for id in ["project", "location"] {
            assert!(definition
                .endpoint_fields
                .iter()
                .any(|field| field.id == id && field.required));
        }
    }

    #[test]
    fn openai_compatible_covers_all_auth_modes() {
        let definition = openai_compatible_definition();
        definition.validate().unwrap();
        let kinds: Vec<AuthKind> = definition
            .auth_options
            .iter()
            .map(|option| option.auth_kind)
            .collect();
        assert!(kinds.contains(&AuthKind::None));
        assert!(kinds.contains(&AuthKind::ApiKey));
        assert!(kinds.contains(&AuthKind::BearerToken));
        assert!(kinds.contains(&AuthKind::CustomCompatible));
        assert!(definition
            .endpoint_fields
            .iter()
            .any(|field| field.id == "base_url" && field.required));
        let mut catalog = ProviderCatalog::new();
        catalog.register(definition).unwrap();
        catalog.validate().unwrap();
    }

    #[test]
    fn catalog_snapshot_has_schema_and_no_secrets() {
        let catalog = default_catalog().unwrap();
        let json = serde_json::to_string(&catalog).unwrap();
        let lower = json.to_lowercase();
        assert!(json.contains(PROVIDER_CATALOG_REGISTRY_V1));
        assert_eq!(catalog.providers.len(), 31);
        for marker in ["\"sk-\"", "bearer sk-"] {
            assert!(!lower.contains(marker), "snapshot leaks {marker}");
        }
        let decoded: ProviderCatalog = serde_json::from_str(&json).unwrap();
        assert_eq!(catalog, decoded);
    }

    #[test]
    fn oauth_stubs_require_device_flow() {
        for definition in [chatgpt_definition(), copilot_definition()] {
            definition.validate().unwrap();
            assert_eq!(
                definition.availability,
                ProviderAvailability::DisabledByFeature
            );
            assert!(definition
                .auth_options
                .iter()
                .any(|option| option.auth_kind == AuthKind::OAuth2Pkce));
        }
        let copilot = copilot_definition();
        assert!(copilot.auth_options[0]
            .extra_headers
            .iter()
            .any(|header| header.name == "openai-intent"));
    }

    #[test]
    fn cohere_uses_custom_transport() {
        let definition = cohere_definition();
        definition.validate().unwrap();
        assert_eq!(definition.transport_family, TransportFamily::Custom);
        assert_eq!(
            definition.availability,
            ProviderAvailability::DisabledByFeature
        );
    }

    #[test]
    fn llamacpp_is_local_without_auth() {
        let definition = llamacpp_definition();
        definition.validate().unwrap();
        assert!(definition
            .auth_options
            .iter()
            .any(|option| option.auth_kind == AuthKind::None));
        assert_eq!(
            definition.availability,
            ProviderAvailability::DisabledByFeature
        );
    }

    #[test]
    fn voyage_is_embeddings_only() {
        let definition = voyage_definition();
        definition.validate().unwrap();
        assert!(!definition.capabilities.tool_calls);
        assert!(!definition.capabilities.streaming);
        assert_eq!(
            definition.availability,
            ProviderAvailability::DisabledByFeature
        );
    }

    #[test]
    fn every_definition_resolves_endpoint_and_model_shape() {
        use crate::provider::{normalize_endpoint, resolve_model};
        let catalog = default_catalog().unwrap();
        for provider in &catalog.providers {
            provider.validate().unwrap();
            let model_field = provider
                .endpoint_fields
                .iter()
                .find(|field| field.id == "model");
            if provider.id == "azure-openai" {
                assert!(
                    provider
                        .endpoint_fields
                        .iter()
                        .any(|field| field.id == "deployment" && field.required),
                    "azure must keep its deployment field"
                );
                continue;
            }
            assert!(
                model_field.is_some_and(|field| field.required),
                "{} must carry a required model field",
                provider.id
            );
            let has_base = provider.default_base_url.is_some()
                || provider.endpoint_fields.iter().any(|field| {
                    matches!(
                        field.id.as_str(),
                        "base_url" | "endpoint" | "region" | "project"
                    )
                });
            assert!(has_base, "{} must carry an endpoint field", provider.id);
            if provider.availability == ProviderAvailability::Available {
                assert!(
                    normalize_endpoint(None, provider.default_base_url.as_deref()).is_ok()
                        || provider
                            .endpoint_fields
                            .iter()
                            .any(|field| (field.id == "base_url" || field.id == "endpoint")
                                && field.required),
                    "{} must be usable without stored config",
                    provider.id
                );
            }
        }
        assert_eq!(
            resolve_model(&serde_json::json!({"model": "gpt-4o"})),
            Ok("gpt-4o".into())
        );
    }
}
