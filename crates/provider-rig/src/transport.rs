use assistant_contracts::model::ModelRequest;
use assistant_contracts::model::ModelResponse;
use assistant_contracts::provider::{
    normalize_endpoint, resolve_model, AuthKind, AuthOptionSpec, ProviderDefinition,
    TransportFamily,
};
use assistant_contracts::ProviderEventSink;

use assistant_contracts::factory::FactoryError;

use crate::client::{complete_openai_compatible, stream_openai_compatible};
use crate::RigAdapterError;

type TransportResult<T> = std::result::Result<T, RigAdapterError>;

pub enum ProviderTransport {
    OpenAi {
        base_url: String,
        model: String,
        secret: String,
    },
    Anthropic {
        base_url: String,
        model: String,
        secret: String,
        api_version: String,
    },
    Gemini {
        model: String,
        secret: String,
    },
    Azure {
        endpoint: String,
        deployment: String,
        api_version: String,
        secret: String,
    },
}

impl ProviderTransport {
    pub fn model(&self) -> &str {
        match self {
            Self::OpenAi { model, .. }
            | Self::Anthropic { model, .. }
            | Self::Gemini { model, .. } => model,
            Self::Azure { deployment, .. } => deployment,
        }
    }
}

pub fn build_transport(
    definition: &ProviderDefinition,
    option: &AuthOptionSpec,
    config: &serde_json::Value,
    secret: Option<&str>,
) -> TransportResult<ProviderTransport> {
    if option.auth_kind == AuthKind::None {
        if secret.is_some() {
            return Err(RigAdapterError::Factory(FactoryError::InvalidConfig(
                "no-auth option takes no secret".into(),
            )));
        }
    } else if secret.is_none_or(|value| value.is_empty()) {
        return Err(RigAdapterError::Factory(FactoryError::InvalidConfig(
            "auth option requires a secret".into(),
        )));
    }
    if definition.id == "azure-openai" {
        return azure_transport(definition, config, secret);
    }
    match definition.transport_family {
        TransportFamily::OpenAiCompatible => {
            let base_url = normalize_endpoint(
                config_field(config, "base_url"),
                definition.default_base_url.as_deref(),
            )
            .map_err(|e| RigAdapterError::RequestFailed(e.into()))?;
            let model =
                resolve_model(config).map_err(|e| RigAdapterError::RequestFailed(e.into()))?;
            Ok(ProviderTransport::OpenAi {
                base_url,
                model,
                secret: secret.unwrap_or_default().to_owned(),
            })
        }
        TransportFamily::AnthropicCompatible => {
            let base_url = normalize_endpoint(
                config_field(config, "base_url"),
                definition.default_base_url.as_deref(),
            )
            .map_err(|e| RigAdapterError::RequestFailed(e.into()))?;
            let model =
                resolve_model(config).map_err(|e| RigAdapterError::RequestFailed(e.into()))?;
            let api_version = config_field(config, "api_version")
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("2023-06-01")
                .trim()
                .to_owned();
            Ok(ProviderTransport::Anthropic {
                base_url,
                model,
                secret: secret.unwrap_or_default().to_owned(),
                api_version,
            })
        }
        TransportFamily::GoogleGemini => {
            let model =
                resolve_model(config).map_err(|e| RigAdapterError::RequestFailed(e.into()))?;
            Ok(ProviderTransport::Gemini {
                model,
                secret: secret.unwrap_or_default().to_owned(),
            })
        }
        TransportFamily::AwsBedrock | TransportFamily::GoogleVertexAi | TransportFamily::Custom => {
            Err(RigAdapterError::Factory(FactoryError::InvalidConfig(
                format!(
                    "no native adapter for transport family {:?} yet",
                    definition.transport_family
                ),
            )))
        }
    }
}

fn azure_transport(
    definition: &ProviderDefinition,
    config: &serde_json::Value,
    secret: Option<&str>,
) -> TransportResult<ProviderTransport> {
    let endpoint = normalize_endpoint(
        config_field(config, "endpoint"),
        definition.default_base_url.as_deref(),
    )
    .map_err(|e| RigAdapterError::RequestFailed(e.into()))?;
    let deployment = config_field(config, "deployment")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| RigAdapterError::RequestFailed("deployment is required".into()))?
        .trim()
        .to_owned();
    let api_version = config_field(config, "api_version")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| RigAdapterError::RequestFailed("api_version is required".into()))?
        .trim()
        .to_owned();
    Ok(ProviderTransport::Azure {
        endpoint,
        deployment,
        api_version,
        secret: secret.unwrap_or_default().to_owned(),
    })
}

fn config_field<'a>(config: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    config.get(key).and_then(|value| value.as_str())
}

pub async fn complete_transport(
    transport: &ProviderTransport,
    request: &ModelRequest,
) -> TransportResult<ModelResponse> {
    match transport {
        ProviderTransport::OpenAi {
            base_url, secret, ..
        } => complete_openai_compatible(base_url, secret, request).await,
        ProviderTransport::Anthropic {
            base_url,
            model,
            secret,
            api_version,
        } => {
            let response =
                complete_anthropic(base_url, model, secret, api_version, request).await?;
            Ok(response)
        }
        ProviderTransport::Gemini { model, secret } => {
            complete_gemini(model, secret, request).await
        }
        ProviderTransport::Azure {
            endpoint,
            deployment,
            api_version,
            secret,
        } => complete_azure(endpoint, deployment, api_version, secret, request).await,
    }
}

pub async fn stream_transport(
    transport: &ProviderTransport,
    request: &ModelRequest,
    sink: &ProviderEventSink,
) -> TransportResult<ModelResponse> {
    match transport {
        ProviderTransport::OpenAi {
            base_url, secret, ..
        } => stream_openai_compatible(base_url, secret, request, sink).await,
        ProviderTransport::Anthropic {
            base_url,
            model,
            secret,
            api_version,
        } => stream_anthropic(base_url, model, secret, api_version, request, sink).await,
        ProviderTransport::Gemini { model, secret } => {
            stream_gemini(model, secret, request, sink).await
        }
        ProviderTransport::Azure {
            endpoint,
            deployment,
            api_version,
            secret,
        } => stream_azure(endpoint, deployment, api_version, secret, request, sink).await,
    }
}

fn anthropic_request(
    request: &ModelRequest,
) -> TransportResult<rig_core::completion::CompletionRequest> {
    super::client::build_rig_request(request)
}

async fn complete_anthropic(
    base_url: &str,
    model: &str,
    secret: &str,
    api_version: &str,
    request: &ModelRequest,
) -> TransportResult<ModelResponse> {
    use rig_core::client::CompletionClient;
    use rig_core::completion::CompletionModel;
    use rig_core::providers::anthropic;
    request
        .validate()
        .map_err(|e| RigAdapterError::RequestFailed(e.to_owned()))?;
    let client = anthropic::Client::builder()
        .api_key(secret)
        .base_url(base_url)
        .anthropic_version(api_version)
        .build()
        .map_err(|e| RigAdapterError::RequestFailed(e.to_string()))?;
    let model = client.completion_model(model.to_owned());
    let rig_request = anthropic_request(request)?;
    let response = model
        .completion(rig_request)
        .await
        .map_err(super::client::normalize_error)?;
    Ok(super::client::to_model_response(&request.model, response))
}

async fn stream_anthropic(
    base_url: &str,
    model: &str,
    secret: &str,
    api_version: &str,
    request: &ModelRequest,
    sink: &ProviderEventSink,
) -> TransportResult<ModelResponse> {
    use rig_core::client::CompletionClient;
    use rig_core::providers::anthropic;
    request
        .validate()
        .map_err(|e| RigAdapterError::RequestFailed(e.to_owned()))?;
    let client = anthropic::Client::builder()
        .api_key(secret)
        .base_url(base_url)
        .anthropic_version(api_version)
        .build()
        .map_err(|e| RigAdapterError::RequestFailed(e.to_string()))?;
    let model = client.completion_model(model.to_owned());
    let rig_request = anthropic_request(request)?;
    super::client::run_stream(model, rig_request, &request.model, sink).await
}

async fn complete_gemini(
    model: &str,
    secret: &str,
    request: &ModelRequest,
) -> TransportResult<ModelResponse> {
    use rig_core::client::CompletionClient;
    use rig_core::completion::CompletionModel;
    use rig_core::providers::gemini;
    request
        .validate()
        .map_err(|e| RigAdapterError::RequestFailed(e.to_owned()))?;
    let client = gemini::Client::builder()
        .api_key(secret)
        .build()
        .map_err(|e| RigAdapterError::RequestFailed(e.to_string()))?;
    let model = client.completion_model(model.to_owned());
    let rig_request = anthropic_request(request)?;
    let response = model
        .completion(rig_request)
        .await
        .map_err(super::client::normalize_error)?;
    Ok(super::client::to_model_response(&request.model, response))
}

async fn stream_gemini(
    model: &str,
    secret: &str,
    request: &ModelRequest,
    sink: &ProviderEventSink,
) -> TransportResult<ModelResponse> {
    use rig_core::client::CompletionClient;
    use rig_core::providers::gemini;
    request
        .validate()
        .map_err(|e| RigAdapterError::RequestFailed(e.to_owned()))?;
    let client = gemini::Client::builder()
        .api_key(secret)
        .build()
        .map_err(|e| RigAdapterError::RequestFailed(e.to_string()))?;
    let model = client.completion_model(model.to_owned());
    let rig_request = anthropic_request(request)?;
    super::client::run_stream(model, rig_request, &request.model, sink).await
}

async fn complete_azure(
    endpoint: &str,
    deployment: &str,
    api_version: &str,
    secret: &str,
    request: &ModelRequest,
) -> TransportResult<ModelResponse> {
    use rig_core::client::CompletionClient;
    use rig_core::completion::CompletionModel;
    use rig_core::providers::azure;
    request
        .validate()
        .map_err(|e| RigAdapterError::RequestFailed(e.to_owned()))?;
    let client = azure::Client::builder()
        .api_key(secret)
        .azure_endpoint(endpoint.to_owned())
        .api_version(api_version)
        .build()
        .map_err(|e| RigAdapterError::RequestFailed(e.to_string()))?;
    let model = client.completion_model(deployment.to_owned());
    let rig_request = anthropic_request(request)?;
    let response = model
        .completion(rig_request)
        .await
        .map_err(super::client::normalize_error)?;
    Ok(super::client::to_model_response(&request.model, response))
}

async fn stream_azure(
    endpoint: &str,
    deployment: &str,
    api_version: &str,
    secret: &str,
    request: &ModelRequest,
    sink: &ProviderEventSink,
) -> TransportResult<ModelResponse> {
    use rig_core::client::CompletionClient;
    use rig_core::providers::azure;
    request
        .validate()
        .map_err(|e| RigAdapterError::RequestFailed(e.to_owned()))?;
    let client = azure::Client::builder()
        .api_key(secret)
        .azure_endpoint(endpoint.to_owned())
        .api_version(api_version)
        .build()
        .map_err(|e| RigAdapterError::RequestFailed(e.to_string()))?;
    let model = client.completion_model(deployment.to_owned());
    let rig_request = anthropic_request(request)?;
    super::client::run_stream(model, rig_request, &request.model, sink).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use assistant_contracts::catalog_seeds::{
        anthropic_compatible_definition, anthropic_definition, azure_definition, default_catalog,
        gemini_definition, nvidia_definition, ollama_definition, openai_compatible_definition,
        openai_definition,
    };
    use assistant_contracts::provider::AuthKind;
    use serde_json::json;

    fn option_for(
        definition: &ProviderDefinition,
        kind: AuthKind,
    ) -> assistant_contracts::provider::AuthOptionSpec {
        definition
            .auth_options
            .iter()
            .find(|option| option.auth_kind == kind)
            .unwrap()
            .clone()
    }

    #[test]
    fn openai_transport_uses_default_when_config_empty() {
        let definition = openai_definition();
        let option = option_for(&definition, AuthKind::ApiKey);
        let transport = build_transport(
            &definition,
            &option,
            &json!({"model": "gpt-4o"}),
            Some("sk-x"),
        )
        .unwrap();
        assert!(matches!(
            transport,
            ProviderTransport::OpenAi { ref base_url, .. } if base_url == "https://api.openai.com/v1"
        ));
    }

    #[test]
    fn config_endpoint_overrides_default() {
        let definition = nvidia_definition();
        let option = option_for(&definition, AuthKind::ApiKey);
        let transport = build_transport(
            &definition,
            &option,
            &json!({"base_url": "https://proxy.example.com/v1/", "model": "m"}),
            Some("sk-x"),
        )
        .unwrap();
        assert!(matches!(
            transport,
            ProviderTransport::OpenAi { ref base_url, .. } if base_url == "https://proxy.example.com/v1"
        ));
    }

    #[test]
    fn bad_endpoint_rejected() {
        let definition = openai_definition();
        let option = option_for(&definition, AuthKind::ApiKey);
        assert!(build_transport(
            &definition,
            &option,
            &json!({"base_url": "ftp://x", "model": "m"}),
            Some("sk-x")
        )
        .is_err());
    }

    #[test]
    fn anthropic_transport_carries_version() {
        let definition = anthropic_definition();
        let option = option_for(&definition, AuthKind::ApiKey);
        let transport = build_transport(
            &definition,
            &option,
            &json!({"model": "claude-sonnet-4-5"}),
            Some("sk-x"),
        )
        .unwrap();
        assert!(matches!(
            transport,
            ProviderTransport::Anthropic { ref api_version, .. } if api_version == "2023-06-01"
        ));
    }

    #[test]
    fn anthropic_compatible_accepts_custom_version() {
        let definition = anthropic_compatible_definition();
        let option = option_for(&definition, AuthKind::ApiKey);
        let transport = build_transport(
            &definition,
            &option,
            &json!({"base_url": "https://proxy.example.com", "model": "m", "api_version": "2024-01-01"}),
            Some("sk-x"),
        )
        .unwrap();
        assert!(matches!(
            transport,
            ProviderTransport::Anthropic { ref api_version, .. } if api_version == "2024-01-01"
        ));
    }

    #[test]
    fn gemini_transport_needs_no_endpoint() {
        let definition = gemini_definition();
        let option = option_for(&definition, AuthKind::ApiKey);
        let transport = build_transport(
            &definition,
            &option,
            &json!({"model": "gemini-2.0-flash"}),
            Some("sk-x"),
        )
        .unwrap();
        assert!(matches!(transport, ProviderTransport::Gemini { .. }));
    }

    #[test]
    fn azure_transport_requires_deployment_fields() {
        let definition = azure_definition();
        let option = option_for(&definition, AuthKind::ApiKey);
        assert!(
            build_transport(&definition, &option, &json!({"model": "m"}), Some("sk-x")).is_err()
        );
        let transport = build_transport(
            &definition,
            &option,
            &json!({
                "endpoint": "https://my-resource.openai.azure.com/",
                "deployment": "my-deploy",
                "api_version": "2024-10-21",
            }),
            Some("sk-x"),
        )
        .unwrap();
        assert!(matches!(
            transport,
            ProviderTransport::Azure { ref deployment, .. } if deployment == "my-deploy"
        ));
    }

    #[test]
    fn no_auth_option_takes_no_secret() {
        let definition = ollama_definition();
        let option = option_for(&definition, AuthKind::None);
        assert!(build_transport(
            &definition,
            &option,
            &json!({"model": "llama3.1"}),
            Some("sk-x")
        )
        .is_err());
        assert!(matches!(
            build_transport(&definition, &option, &json!({"model": "llama3.1"}), None).unwrap(),
            ProviderTransport::OpenAi { .. }
        ));
    }

    #[test]
    fn missing_secret_rejected() {
        let definition = openai_compatible_definition();
        let option = option_for(&definition, AuthKind::ApiKey);
        assert!(build_transport(
            &definition,
            &option,
            &json!({"base_url": "https://x.example.com", "model": "m"}),
            None
        )
        .is_err());
    }

    #[test]
    fn missing_model_rejected() {
        let definition = openai_definition();
        let option = option_for(&definition, AuthKind::ApiKey);
        assert!(build_transport(&definition, &option, &json!({}), Some("sk-x")).is_err());
    }

    #[test]
    fn full_catalog_constructs() {
        let catalog = default_catalog().unwrap();
        assert!(catalog.providers.len() >= 30);
    }
}
