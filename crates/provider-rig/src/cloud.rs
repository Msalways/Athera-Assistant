use assistant_contracts::model::{
    ModelMessage, ModelMessageRole, ModelRequest, ModelResponse, NormalizedError,
    MODEL_REQUEST_SCHEMA_V1,
};
use assistant_contracts::{
    protocol, AgentAction, ContextBundle, Error, ModelCapabilities, ModelProvider, Result,
};

use crate::transport::ProviderTransport;
use crate::transport::{complete_transport, stream_transport};
use crate::RigAdapterError;
use assistant_contracts::ProviderEventSink;

pub struct RigCloudProvider {
    provider_id: String,
    transport: ProviderTransport,
}

impl RigCloudProvider {
    pub fn new(
        provider_id: &str,
        transport: ProviderTransport,
    ) -> std::result::Result<Self, RigAdapterError> {
        if provider_id.is_empty() {
            return Err(RigAdapterError::RequestFailed(
                "provider id is required".into(),
            ));
        }
        Ok(Self {
            provider_id: provider_id.to_owned(),
            transport,
        })
    }

    fn model_name(&self) -> &str {
        self.transport.model()
    }

    pub fn probe_request(model: &str) -> ModelRequest {
        ModelRequest {
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
            stream: true,
        }
    }

    /// Probe over the same streaming contract the assistant chats on.
    ///
    /// A unary probe can pass against a provider whose stream is broken or
    /// empty, leaving setup green while every real message fails. Validating
    /// the stream keeps one contract for connection test and normal use.
    pub async fn probe(&self) -> std::result::Result<ModelResponse, RigAdapterError> {
        let request = Self::probe_request(self.model_name());
        let sink: ProviderEventSink = std::sync::Arc::new(|_| Ok(()));
        match stream_transport(&self.transport, &request, &sink).await {
            Ok(response) if response.content.trim().is_empty() => Err(RigAdapterError::Normalized(
                NormalizedError::InvalidResponse {
                    detail: "stream produced no content".into(),
                },
            )),
            other => other,
        }
    }

    pub fn request_from_context(model: &str, context: &ContextBundle) -> Result<ModelRequest> {
        let packet = protocol::packet(context)?;
        let tools = protocol::functions(context)
            .into_iter()
            .filter_map(|tool| {
                Some(assistant_contracts::model::ToolDefinition {
                    id: tool.get("name")?.as_str()?.to_owned(),
                    name: tool.get("name")?.as_str()?.to_owned(),
                    description: tool.get("description")?.as_str()?.to_owned(),
                    input_schema: tool.get("parameters")?.clone(),
                })
            })
            .collect::<Vec<_>>();
        Ok(ModelRequest {
            schema: MODEL_REQUEST_SCHEMA_V1.into(),
            messages: vec![
                ModelMessage {
                    role: ModelMessageRole::System,
                    content: protocol::POLICY.to_owned(),
                    tool_call_id: None,
                },
                ModelMessage {
                    role: ModelMessageRole::User,
                    content: packet,
                    tool_call_id: None,
                },
            ],
            tools,
            model: model.to_owned(),
            max_tokens: None,
            temperature: None,
            stream: false,
        })
    }

    pub fn action_from_response(
        response: ModelResponse,
        context: &ContextBundle,
    ) -> Result<AgentAction> {
        match response.tool_calls.as_slice() {
            [call] => protocol::decode_call(&call.name, call.arguments.clone(), context),
            [] if !response.content.is_empty() => Ok(AgentAction::Respond {
                text: response.content,
            }),
            _ => Err(Error::InvalidResponse),
        }
    }

    /// Map an engine error into provider vocabulary without collapsing distinct
    /// causes onto one variant.
    pub fn typed_engine_error(error: Error) -> NormalizedError {
        assistant_contracts::normalized_from_engine(error)
    }

    pub fn engine_error(error: RigAdapterError) -> Error {
        match error {
            RigAdapterError::Normalized(normalized) => match normalized {
                NormalizedError::AuthenticationFailed => Error::AuthRequired,
                NormalizedError::AuthorizationDenied => Error::Denied,
                NormalizedError::EndpointNotFound => Error::Unavailable,
                NormalizedError::ModelNotFound => Error::InvalidResponse,
                NormalizedError::RateLimited { .. } | NormalizedError::QuotaExceeded => {
                    Error::RateLimited
                }
                NormalizedError::Timeout => Error::Timeout,
                NormalizedError::NetworkUnavailable => Error::Unavailable,
                NormalizedError::InvalidResponse { .. } => Error::InvalidResponse,
                NormalizedError::ProviderError { .. } => Error::Unavailable,
            },
            RigAdapterError::Factory(_) | RigAdapterError::RequestFailed(_) => Error::InvalidInput,
        }
    }
}

#[async_trait::async_trait]
impl ModelProvider for RigCloudProvider {
    fn id(&self) -> &str {
        &self.provider_id
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            tool_calls: true,
            planning: true,
            local: false,
        }
    }

    async fn infer(&self, context: ContextBundle) -> Result<AgentAction> {
        let request = Self::request_from_context(self.model_name(), &context)?;
        let response = complete_transport(&self.transport, &request)
            .await
            .map_err(Self::engine_error)?;
        Self::action_from_response(response, &context)
    }

    async fn infer_stream(
        &self,
        context: ContextBundle,
        sink: assistant_contracts::ProviderEventSink,
    ) -> Result<AgentAction> {
        let request = Self::request_from_context(self.model_name(), &context)?;
        let response = stream_transport(&self.transport, &request, &sink)
            .await
            .map_err(Self::engine_error)?;
        Self::action_from_response(response, &context)
    }

    /// Overrides the lossy trait default: only this adapter knows the HTTP
    /// status and the adapter-level fault behind a failure, which is what
    /// separates "worth trying the next provider" from "the user's setup is
    /// wrong".
    async fn infer_typed(
        &self,
        context: ContextBundle,
        sink: Option<assistant_contracts::ProviderEventSink>,
    ) -> std::result::Result<AgentAction, NormalizedError> {
        let typed = async {
            let request = Self::request_from_context(self.model_name(), &context)
                .map_err(Self::typed_engine_error)?;
            let response = match &sink {
                Some(sink) => stream_transport(&self.transport, &request, sink).await,
                None => complete_transport(&self.transport, &request).await,
            }
            .map_err(|error| match error {
                RigAdapterError::Normalized(normalized) => normalized,
                other => NormalizedError::ProviderError {
                    status: 0,
                    detail: other.to_string(),
                },
            })?;
            Self::action_from_response(response, &context).map_err(Self::typed_engine_error)
        }
        .await;
        typed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assistant_contracts::model::{ModelUsage, ToolCallRequest};
    use assistant_contracts::{FinishReason, Id, Role};

    fn context() -> ContextBundle {
        ContextBundle {
            task_id: Id::new_v4(),
            role: Role::Reasoner,
            goal: "greet".into(),
            plan: vec![],
            handoff: None,
            history: vec![],
            results: vec![],
            skills: vec![],
            candidates: vec![],
            tools: vec![],
            adaptive_rules: vec![],
        }
    }

    #[test]
    fn probe_request_uses_the_configured_model_bounded_output_and_the_chat_stream_contract() {
        let request = RigCloudProvider::probe_request("fixture-model");
        assert_eq!(request.model, "fixture-model");
        assert_eq!(request.max_tokens, Some(16));
        // The probe must exercise the same streaming contract the assistant
        // chats on, otherwise setup can pass against a provider that cannot
        // actually stream a reply.
        assert!(request.stream);
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.messages[0].content, "hi");
    }

    #[test]
    fn rejects_empty_provider_id() {
        let transport = ProviderTransport::OpenAi {
            base_url: "https://x.test/v1".into(),
            model: "m".into(),
            secret: "s".into(),
        };
        assert!(RigCloudProvider::new("", transport).is_err());
    }

    #[test]
    fn model_name_follows_transport_variant() {
        for (transport, expected) in [
            (
                ProviderTransport::OpenAi {
                    base_url: "https://x.test/v1".into(),
                    model: "gpt-4o".into(),
                    secret: "s".into(),
                },
                "gpt-4o",
            ),
            (
                ProviderTransport::Azure {
                    endpoint: "https://my-resource.openai.azure.com".into(),
                    deployment: "my-deploy".into(),
                    api_version: "2024-10-21".into(),
                    secret: "s".into(),
                },
                "my-deploy",
            ),
        ] {
            let provider = RigCloudProvider::new("p", transport).unwrap();
            assert_eq!(provider.model_name(), expected);
            let request =
                RigCloudProvider::request_from_context(provider.model_name(), &context()).unwrap();
            assert_eq!(request.model, expected);
        }
    }

    #[test]
    fn request_carries_policy_packet_and_protocol_tools() {
        let request = RigCloudProvider::request_from_context("m", &context()).unwrap();
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].role, ModelMessageRole::System);
        assert!(request.messages[1].content.contains("greet"));
        let names: Vec<&str> = request
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect();
        assert!(names.contains(&"assistant_control"));
        assert!(names.contains(&"capabilities_search"));
    }

    fn response(content: &str, calls: Vec<ToolCallRequest>) -> ModelResponse {
        ModelResponse {
            content: content.into(),
            tool_calls: calls,
            usage: ModelUsage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
            },
            finish_reason: FinishReason::Stop,
            model_id: "m".into(),
        }
    }

    #[test]
    fn text_response_becomes_respond() {
        let action =
            RigCloudProvider::action_from_response(response("hi", vec![]), &context()).unwrap();
        assert!(matches!(action, AgentAction::Respond { .. }));
    }

    #[test]
    fn multiple_tool_calls_are_invalid() {
        let calls = vec![
            ToolCallRequest {
                id: "1".into(),
                name: "capabilities_search".into(),
                arguments: serde_json::json!({"query": "x"}),
            },
            ToolCallRequest {
                id: "2".into(),
                name: "capabilities_search".into(),
                arguments: serde_json::json!({"query": "y"}),
            },
        ];
        assert!(RigCloudProvider::action_from_response(response("", calls), &context()).is_err());
    }

    #[test]
    fn engine_error_mapping() {
        assert_eq!(
            RigCloudProvider::engine_error(RigAdapterError::Normalized(
                NormalizedError::AuthenticationFailed
            )),
            Error::AuthRequired
        );
        assert_eq!(
            RigCloudProvider::engine_error(RigAdapterError::Normalized(
                NormalizedError::RateLimited {
                    retry_after_secs: None
                }
            )),
            Error::RateLimited
        );
        assert_eq!(
            RigCloudProvider::engine_error(RigAdapterError::Normalized(
                NormalizedError::NetworkUnavailable
            )),
            Error::Unavailable
        );
    }
}
