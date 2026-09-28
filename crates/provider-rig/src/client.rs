use assistant_contracts::connection_test::ConnectionTestService;
use assistant_contracts::model::{
    FinishReason, ModelMessage, ModelMessageRole, ModelRequest, ModelResponse, ModelStreamDelta,
    ModelUsage, NormalizedError, ToolCallRequest,
};
use assistant_contracts::provider_health::NormalizedTestFailure;
use assistant_contracts::{ProviderEvent, ProviderEventSink};
use futures::StreamExt;
use rig_core::client::CompletionClient;
use rig_core::completion::message::Text;
use rig_core::completion::{AssistantContent, CompletionModel, Message};
use rig_core::providers::openai;
use rig_core::streaming::{StreamedAssistantContent, ToolCallDeltaContent};

use crate::RigAdapterError;

pub async fn complete_openai_compatible(
    base_url: &str,
    raw_secret: &str,
    request: &ModelRequest,
) -> Result<ModelResponse, RigAdapterError> {
    let rig_request = build_rig_request(request)?;
    let client = openai::Client::builder()
        .api_key(raw_secret)
        .base_url(base_url)
        .build()
        .map_err(|e| RigAdapterError::RequestFailed(e.to_string()))?
        .completions_api();
    let model = client.completion_model(request.model.clone());
    let response = model
        .completion(rig_request)
        .await
        .map_err(normalize_error)?;
    Ok(to_model_response(&request.model, response))
}

pub async fn stream_openai_compatible(
    base_url: &str,
    raw_secret: &str,
    request: &ModelRequest,
    sink: &ProviderEventSink,
) -> Result<ModelResponse, RigAdapterError> {
    let rig_request = build_rig_request(request)?;
    let client = openai::Client::builder()
        .api_key(raw_secret)
        .base_url(base_url)
        .build()
        .map_err(|e| RigAdapterError::RequestFailed(e.to_string()))?
        .completions_api();
    let model = client.completion_model(request.model.clone());
    run_stream(model, rig_request, &request.model, sink).await
}

pub(crate) async fn run_stream<M: CompletionModel>(
    model: M,
    request: rig_core::completion::CompletionRequest,
    model_id: &str,
    sink: &ProviderEventSink,
) -> Result<ModelResponse, RigAdapterError> {
    let mut stream = model.stream(request).await.map_err(normalize_error)?;
    let mut content = String::new();
    let mut tool_calls = Vec::new();
    let mut tool_index = 0usize;
    let mut usage = ModelUsage {
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
    };
    let mut finish_reason = FinishReason::Stop;
    while let Some(item) = stream.next().await {
        let item = item.map_err(normalize_error)?;
        for delta in map_stream_item(&item, &mut tool_index) {
            match &delta {
                ModelStreamDelta::ContentDelta { text } => {
                    content.push_str(text);
                    sink(ProviderEvent::TextDelta { text: text.clone() })
                        .map_err(|_| RigAdapterError::RequestFailed("event sink failed".into()))?;
                }
                ModelStreamDelta::ToolCallDelta {
                    id,
                    name,
                    arguments_delta,
                    ..
                } => {
                    if !arguments_delta.is_empty() || name.is_some() {
                        merge_tool_delta(&mut tool_calls, id, name, arguments_delta);
                    }
                }
                ModelStreamDelta::UsageDelta { usage: delta_usage } => {
                    usage = *delta_usage;
                }
                ModelStreamDelta::StreamEnd {
                    finish_reason: reason,
                } => {
                    finish_reason = *reason;
                }
            }
        }
    }
    if !tool_calls.is_empty() && finish_reason == FinishReason::Stop {
        finish_reason = FinishReason::ToolCalls;
    }
    Ok(ModelResponse {
        content,
        tool_calls: finalize_tool_calls(tool_calls)?,
        usage,
        finish_reason,
        model_id: model_id.to_owned(),
    })
}

struct PendingToolCall {
    id: String,
    name: String,
    arguments: String,
}

fn merge_tool_delta(
    tool_calls: &mut Vec<PendingToolCall>,
    id: &Option<String>,
    name: &Option<String>,
    arguments_delta: &str,
) {
    let key = id.clone().unwrap_or_default();
    // Providers stream a tool call as: id and name first, then argument
    // fragments that carry no id. Those fragments belong to the call in
    // progress, so an id-less delta continues the most recent one. Starting a
    // second call instead would split one request into two and make the whole
    // response look invalid.
    let existing = if key.is_empty() {
        tool_calls.last_mut()
    } else {
        tool_calls.iter_mut().find(|call| call.id == key)
    };
    if let Some(existing) = existing {
        if existing.name.is_empty() {
            if let Some(name) = name {
                existing.name = name.clone();
            }
        }
        existing.arguments.push_str(arguments_delta);
        return;
    }
    tool_calls.push(PendingToolCall {
        id: key,
        name: name.clone().unwrap_or_default(),
        arguments: arguments_delta.to_owned(),
    });
}

fn finalize_tool_calls(
    pending: Vec<PendingToolCall>,
) -> Result<Vec<ToolCallRequest>, RigAdapterError> {
    let mut calls: Vec<ToolCallRequest> = Vec::new();
    for call in pending {
        let arguments = if call.arguments.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(&call.arguments).map_err(|_| {
                RigAdapterError::Normalized(NormalizedError::InvalidResponse {
                    detail: format!("tool call {} has malformed arguments", call.id),
                })
            })?
        };
        // One logical call can arrive twice: a provider's stream may yield a
        // complete tool call and then the same call again as deltas, each with
        // its own identifier. Keeping both would present the engine with two
        // calls where there is one, and the response would be rejected as
        // invalid. An identical call is the same request, so keep the first.
        if calls
            .iter()
            .any(|existing| existing.name == call.name && existing.arguments == arguments)
        {
            continue;
        }
        calls.push(ToolCallRequest {
            id: call.id,
            name: call.name,
            arguments,
        });
    }
    Ok(calls)
}

fn map_stream_item(
    item: &StreamedAssistantContent,
    tool_index: &mut usize,
) -> Vec<ModelStreamDelta> {
    match item {
        StreamedAssistantContent::Text(text) => vec![ModelStreamDelta::ContentDelta {
            text: text.text.clone(),
        }],
        StreamedAssistantContent::ToolCall { tool_call, .. } => {
            let delta = ModelStreamDelta::ToolCallDelta {
                index: *tool_index,
                id: Some(tool_call.id.as_str().to_owned()),
                name: Some(tool_call.function.name.clone()),
                arguments_delta: serde_json::to_string(&tool_call.function.arguments)
                    .unwrap_or_default(),
            };
            *tool_index += 1;
            vec![delta]
        }
        StreamedAssistantContent::ToolCallDelta {
            internal_call_id,
            content,
        } => match content {
            ToolCallDeltaContent::Name(name) => vec![ModelStreamDelta::ToolCallDelta {
                index: *tool_index,
                id: Some(internal_call_id.clone()),
                name: Some(name.clone()),
                arguments_delta: String::new(),
            }],
            ToolCallDeltaContent::Delta(fragment) => vec![ModelStreamDelta::ToolCallDelta {
                index: *tool_index,
                id: Some(internal_call_id.clone()),
                name: None,
                arguments_delta: fragment.clone(),
            }],
        },
        StreamedAssistantContent::Final(final_record) => vec![
            ModelStreamDelta::UsageDelta {
                usage: ModelUsage {
                    prompt_tokens: u32::try_from(final_record.usage.input_tokens)
                        .unwrap_or(u32::MAX),
                    completion_tokens: u32::try_from(final_record.usage.output_tokens)
                        .unwrap_or(u32::MAX),
                    total_tokens: u32::try_from(final_record.usage.total_tokens)
                        .unwrap_or(u32::MAX),
                },
            },
            ModelStreamDelta::StreamEnd {
                finish_reason: match final_record.finish_reason {
                    Some(rig_core::completion::FinishReason::Length) => FinishReason::Length,
                    Some(rig_core::completion::FinishReason::ToolCalls) => FinishReason::ToolCalls,
                    Some(rig_core::completion::FinishReason::ContentFilter) => {
                        FinishReason::ContentFilter
                    }
                    Some(_) | None => FinishReason::Stop,
                },
            },
        ],
        StreamedAssistantContent::Reasoning { .. }
        | StreamedAssistantContent::ReasoningDelta { .. }
        | StreamedAssistantContent::Unknown(_) => vec![],
    }
}

fn to_rig_message(message: &ModelMessage) -> Message {
    match message.role {
        ModelMessageRole::System => Message::System {
            content: message.content.clone(),
        },
        ModelMessageRole::User | ModelMessageRole::Tool => Message::from(message.content.clone()),
        ModelMessageRole::Assistant => Message::Assistant {
            id: None,
            content: vec![AssistantContent::Text(Text::new(message.content.clone()))],
        },
    }
}

fn to_rig_tool(
    tool: &assistant_contracts::model::ToolDefinition,
) -> rig_core::completion::ToolDefinition {
    rig_core::completion::ToolDefinition {
        name: tool.name.clone(),
        description: tool.description.clone(),
        parameters: tool.input_schema.clone(),
    }
}

pub(crate) fn build_rig_request(
    request: &ModelRequest,
) -> Result<rig_core::completion::CompletionRequest, RigAdapterError> {
    request
        .validate()
        .map_err(|e| RigAdapterError::RequestFailed(e.to_owned()))?;
    Ok(rig_core::completion::CompletionRequest {
        model: Some(request.model.clone()),
        preamble: None,
        chat_history: request.messages.iter().map(to_rig_message).collect(),
        documents: vec![],
        tools: request.tools.iter().map(to_rig_tool).collect(),
        temperature: request.temperature.map(f64::from),
        max_tokens: request.max_tokens.map(u64::from),
        tool_choice: None,
        additional_params: None,
        output_schema: None,
        record_telemetry_content: false,
    })
}

pub(crate) fn to_model_response(
    model_id: &str,
    response: rig_core::completion::CompletionResponse,
) -> ModelResponse {
    let mut content = String::new();
    let mut tool_calls = Vec::new();
    let reported_finish = response.finish_reason();
    for item in response.choice {
        match item {
            AssistantContent::Text(text) => content.push_str(&text.text),
            AssistantContent::ToolCall(call) => tool_calls.push(ToolCallRequest {
                id: call.id.as_str().to_owned(),
                name: call.function.name,
                arguments: call.function.arguments,
            }),
            _ => {}
        }
    }
    let has_tool_calls = !tool_calls.is_empty();
    ModelResponse {
        content,
        tool_calls,
        usage: ModelUsage {
            prompt_tokens: u32::try_from(response.usage.input_tokens).unwrap_or(u32::MAX),
            completion_tokens: u32::try_from(response.usage.output_tokens).unwrap_or(u32::MAX),
            total_tokens: u32::try_from(response.usage.total_tokens).unwrap_or(u32::MAX),
        },
        finish_reason: match reported_finish {
            Some(rig_core::completion::FinishReason::Stop) if has_tool_calls => {
                FinishReason::ToolCalls
            }
            Some(rig_core::completion::FinishReason::Stop) => FinishReason::Stop,
            Some(rig_core::completion::FinishReason::Length) => FinishReason::Length,
            Some(rig_core::completion::FinishReason::ToolCalls) => FinishReason::ToolCalls,
            Some(rig_core::completion::FinishReason::ContentFilter) => FinishReason::ContentFilter,
            Some(rig_core::completion::FinishReason::Other(_)) | None => {
                if has_tool_calls {
                    FinishReason::ToolCalls
                } else {
                    FinishReason::Stop
                }
            }
        },
        model_id: response.model.unwrap_or_else(|| model_id.to_owned()),
    }
}

pub(crate) fn normalize_error(error: rig_core::completion::CompletionError) -> RigAdapterError {
    RigAdapterError::Normalized(map_completion_error(error))
}

fn map_completion_error(error: rig_core::completion::CompletionError) -> NormalizedError {
    use rig_core::completion::CompletionError;
    use rig_core::http_client::Error as HttpError;
    match error {
        CompletionError::HttpError(
            HttpError::InvalidStatusCode(status)
            | HttpError::InvalidStatusCodeWithMessage(status, _),
        ) => map_status(status.as_u16(), None, String::new()),
        CompletionError::HttpError(HttpError::InvalidStatusCodeWithDetails {
            status,
            body,
            headers,
            ..
        }) => {
            let retry_after = headers
                .get("retry-after")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            map_status(status.as_u16(), retry_after, body)
        }
        CompletionError::HttpError(HttpError::Instance(inner)) => {
            map_transport_failure(&inner.to_string())
        }
        CompletionError::HttpError(other) => NormalizedError::InvalidResponse {
            detail: other.to_string(),
        },
        CompletionError::ProviderResponse(error) => match error.status {
            Some(status) => map_status(status.as_u16(), None, error.body),
            None => NormalizedError::ProviderError {
                status: 0,
                detail: error.body,
            },
        },
        CompletionError::ResponseError(detail) | CompletionError::ProviderError(detail) => {
            NormalizedError::ProviderError { status: 0, detail }
        }
        CompletionError::RequestError(_) | CompletionError::JsonError(_) => {
            NormalizedError::InvalidResponse {
                detail: error.to_string(),
            }
        }
        CompletionError::UrlError(_) => NormalizedError::InvalidResponse {
            detail: error.to_string(),
        },
    }
}

fn map_status(status: u16, retry_after_secs: Option<u64>, detail: String) -> NormalizedError {
    match status {
        401 => NormalizedError::AuthenticationFailed,
        403 => NormalizedError::AuthorizationDenied,
        // Providers answer 404 for an unknown model and 410 Gone for one that
        // has been retired. Both mean the caller must choose a different model.
        404 | 410 => NormalizedError::ModelNotFound,
        429 => NormalizedError::RateLimited { retry_after_secs },
        500..=599 => NormalizedError::ProviderError { status, detail },
        _ => NormalizedError::ProviderError { status, detail },
    }
}

fn map_transport_failure(message: &str) -> NormalizedError {
    match ConnectionTestService::classify_connection_error(message) {
        NormalizedTestFailure::Credential => NormalizedError::AuthenticationFailed,
        NormalizedTestFailure::Endpoint => NormalizedError::EndpointNotFound,
        NormalizedTestFailure::ModelNotFound => NormalizedError::ModelNotFound,
        NormalizedTestFailure::Quota => NormalizedError::QuotaExceeded,
        NormalizedTestFailure::Network => NormalizedError::NetworkUnavailable,
        NormalizedTestFailure::ProviderError | NormalizedTestFailure::Unknown => {
            NormalizedError::ProviderError {
                status: 0,
                detail: message.to_owned(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assistant_contracts::model::{ModelMessageRole, MODEL_REQUEST_SCHEMA_V1};
    use rig_core::completion::CompletionResponse;

    fn text_request() -> ModelRequest {
        ModelRequest {
            schema: MODEL_REQUEST_SCHEMA_V1.into(),
            messages: vec![
                ModelMessage {
                    role: ModelMessageRole::System,
                    content: "Be brief.".into(),
                    tool_call_id: None,
                },
                ModelMessage {
                    role: ModelMessageRole::User,
                    content: "hi".into(),
                    tool_call_id: None,
                },
            ],
            tools: vec![],
            model: "meta/llama-3.1-8b-instruct".into(),
            max_tokens: Some(64),
            temperature: Some(0.5),
            stream: false,
        }
    }

    fn rig_response() -> CompletionResponse {
        CompletionResponse::new(
            vec![AssistantContent::Text(Text::new("hello"))],
            rig_core::completion::Usage {
                input_tokens: 10,
                output_tokens: 5,
                total_tokens: 15,
                cached_input_tokens: 0,
                cache_creation_input_tokens: 0,
                tool_use_prompt_tokens: 0,
                reasoning_tokens: 0,
            },
            "openai",
        )
    }

    #[test]
    fn messages_map_to_rig_roles() {
        let mapped: Vec<Message> = text_request().messages.iter().map(to_rig_message).collect();
        assert!(matches!(mapped[0], Message::System { .. }));
        assert!(matches!(mapped[1], Message::User { .. }));
    }

    #[test]
    fn a_tool_call_streamed_across_several_deltas_stays_one_call() {
        // How real providers stream: the id and name arrive first, then the
        // arguments in further deltas that carry no id of their own.
        let mut pending: Vec<PendingToolCall> = Vec::new();
        merge_tool_delta(
            &mut pending,
            &Some("call_1".into()),
            &Some("tool_0".into()),
            "",
        );
        merge_tool_delta(&mut pending, &Some("call_1".into()), &None, "{\"to\":");
        merge_tool_delta(&mut pending, &None, &None, "\"person@example.com\"}");

        let calls = finalize_tool_calls(pending).unwrap();
        assert_eq!(calls.len(), 1, "one streamed call must not split into two");
        assert_eq!(calls[0].name, "tool_0");
        assert_eq!(calls[0].arguments["to"], "person@example.com");
    }

    #[test]
    fn separate_tool_calls_stay_separate() {
        let mut pending: Vec<PendingToolCall> = Vec::new();
        merge_tool_delta(
            &mut pending,
            &Some("call_1".into()),
            &Some("tool_0".into()),
            "{\"a\":",
        );
        merge_tool_delta(&mut pending, &None, &None, "1}");
        merge_tool_delta(
            &mut pending,
            &Some("call_2".into()),
            &Some("tool_1".into()),
            "{\"b\":",
        );
        merge_tool_delta(&mut pending, &None, &None, "2}");

        let calls = finalize_tool_calls(pending).unwrap();
        assert_eq!(calls.len(), 2, "a new id starts a new call");
        assert_eq!(calls[0].name, "tool_0");
        assert_eq!(calls[1].name, "tool_1");
        assert_eq!(calls[0].arguments["a"], 1);
        assert_eq!(calls[1].arguments["b"], 2);
    }

    #[test]
    fn a_tool_call_with_no_arguments_is_still_usable() {
        let mut pending: Vec<PendingToolCall> = Vec::new();
        merge_tool_delta(
            &mut pending,
            &Some("call_1".into()),
            &Some("tool_0".into()),
            "",
        );
        let calls = finalize_tool_calls(pending).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].arguments, serde_json::Value::Null);
    }

    #[test]
    fn malformed_streamed_arguments_are_rejected_not_guessed() {
        let mut pending: Vec<PendingToolCall> = Vec::new();
        merge_tool_delta(
            &mut pending,
            &Some("call_1".into()),
            &Some("tool_0".into()),
            "{\"a\":",
        );
        assert!(finalize_tool_calls(pending).is_err());
    }

    #[test]
    fn a_call_yielded_as_a_complete_item_and_again_as_deltas_is_one_call() {
        // Observed from a real OpenAI-compatible stream: the same logical call
        // arrives first as a complete tool call with a generated id, then again
        // as name/argument deltas under the provider's own id. Both are kept
        // here to mirror the stream, and the duplicate must collapse to one.
        let mut pending: Vec<PendingToolCall> = Vec::new();
        merge_tool_delta(
            &mut pending,
            &Some("generated-id".into()),
            &Some("tool_0".into()),
            r#"{"to":"person@example.com"}"#,
        );
        merge_tool_delta(
            &mut pending,
            &Some("provider-id".into()),
            &Some("tool_0".into()),
            "",
        );
        merge_tool_delta(
            &mut pending,
            &Some("provider-id".into()),
            &None,
            r#"{"to":"person@example.com"}"#,
        );

        let calls = finalize_tool_calls(pending).unwrap();
        assert_eq!(
            calls.len(),
            1,
            "one tool call must never be presented to the engine as two"
        );
        assert_eq!(calls[0].name, "tool_0");
        assert_eq!(calls[0].arguments["to"], "person@example.com");
    }

    #[test]
    fn the_same_tool_called_with_different_arguments_is_kept_twice() {
        let mut pending: Vec<PendingToolCall> = Vec::new();
        merge_tool_delta(
            &mut pending,
            &Some("a".into()),
            &Some("tool_0".into()),
            r#"{"x":1}"#,
        );
        merge_tool_delta(
            &mut pending,
            &Some("b".into()),
            &Some("tool_0".into()),
            r#"{"x":2}"#,
        );
        let calls = finalize_tool_calls(pending).unwrap();
        assert_eq!(calls.len(), 2, "different arguments are different calls");
        assert_eq!(calls[0].arguments["x"], 1);
        assert_eq!(calls[1].arguments["x"], 2);
    }

    #[test]
    fn response_maps_text_usage_and_model() {
        let response = to_model_response("custom-model", rig_response());
        assert_eq!(response.content, "hello");
        assert!(response.tool_calls.is_empty());
        assert_eq!(response.usage.prompt_tokens, 10);
        assert_eq!(response.usage.completion_tokens, 5);
        assert_eq!(response.usage.total_tokens, 15);
        assert_eq!(response.finish_reason, FinishReason::Stop);
    }

    #[test]
    fn status_401_maps_to_authentication_failed() {
        assert_eq!(
            map_status(401, None, String::new()),
            NormalizedError::AuthenticationFailed
        );
    }

    #[test]
    fn status_429_carries_retry_after() {
        assert_eq!(
            map_status(429, Some(60), String::new()),
            NormalizedError::RateLimited {
                retry_after_secs: Some(60)
            }
        );
    }

    #[test]
    fn retired_models_410_map_to_model_not_found() {
        assert_eq!(
            map_status(410, None, String::new()),
            NormalizedError::ModelNotFound
        );
    }

    #[test]
    fn timeout_maps_to_network_unavailable() {
        assert_eq!(
            map_transport_failure("request timed out"),
            NormalizedError::NetworkUnavailable
        );
    }

    #[test]
    fn openai_client_builds_offline() {
        let client = openai::Client::builder()
            .api_key("dummy-key")
            .base_url("https://integrate.api.nvidia.com/v1")
            .build()
            .unwrap()
            .completions_api();
        let _model = client.completion_model("meta/llama-3.1-8b-instruct");
    }

    #[test]
    fn text_item_maps_to_content_delta() {
        let mut index = 0usize;
        let deltas = map_stream_item(&StreamedAssistantContent::text("hello"), &mut index);
        assert_eq!(
            deltas,
            vec![ModelStreamDelta::ContentDelta {
                text: "hello".into()
            }]
        );
        assert_eq!(index, 0);
    }

    #[test]
    fn complete_tool_call_maps_with_index() {
        use rig_core::completion::message::{ToolCall, ToolCallId, ToolFunction};
        let mut index = 0usize;
        let deltas = map_stream_item(
            &StreamedAssistantContent::ToolCall {
                tool_call: ToolCall {
                    id: ToolCallId::new_or_mint("call_1"),
                    provider: None,
                    function: ToolFunction::new("get_time".into(), serde_json::json!({})),
                    signature: None,
                    additional_params: None,
                },
                internal_call_id: "rig-1".into(),
            },
            &mut index,
        );
        assert_eq!(
            deltas,
            vec![ModelStreamDelta::ToolCallDelta {
                index: 0,
                id: Some("call_1".into()),
                name: Some("get_time".into()),
                arguments_delta: "{}".into(),
            }]
        );
        assert_eq!(index, 1);
    }

    #[test]
    fn tool_fragments_share_one_index() {
        let mut index = 0usize;
        let name = map_stream_item(
            &StreamedAssistantContent::ToolCallDelta {
                internal_call_id: "rig-1".into(),
                content: ToolCallDeltaContent::Name("get_time".into()),
            },
            &mut index,
        );
        let fragment = map_stream_item(
            &StreamedAssistantContent::ToolCallDelta {
                internal_call_id: "rig-1".into(),
                content: ToolCallDeltaContent::Delta("{\"x\":1}".into()),
            },
            &mut index,
        );
        assert_eq!(
            name,
            vec![ModelStreamDelta::ToolCallDelta {
                index: 0,
                id: Some("rig-1".into()),
                name: Some("get_time".into()),
                arguments_delta: String::new(),
            }]
        );
        assert_eq!(
            fragment,
            vec![ModelStreamDelta::ToolCallDelta {
                index: 0,
                id: Some("rig-1".into()),
                name: None,
                arguments_delta: "{\"x\":1}".into(),
            }]
        );
        assert_eq!(index, 0);
    }

    #[test]
    fn final_record_maps_usage_and_end() {
        use rig_core::streaming::StreamFinal;
        let mut index = 0usize;
        let deltas = map_stream_item(
            &StreamedAssistantContent::Final(
                StreamFinal::new(
                    "openai",
                    rig_core::completion::Usage {
                        input_tokens: 10,
                        output_tokens: 5,
                        total_tokens: 15,
                        cached_input_tokens: 0,
                        cache_creation_input_tokens: 0,
                        tool_use_prompt_tokens: 0,
                        reasoning_tokens: 0,
                    },
                )
                .with_finish_reason(rig_core::completion::FinishReason::Stop),
            ),
            &mut index,
        );
        assert_eq!(deltas.len(), 2);
        assert!(matches!(deltas[0], ModelStreamDelta::UsageDelta { .. }));
        assert_eq!(
            deltas[1],
            ModelStreamDelta::StreamEnd {
                finish_reason: FinishReason::Stop
            }
        );
    }

    #[test]
    fn reasoning_and_unknown_items_are_skipped() {
        let mut index = 0usize;
        assert!(map_stream_item(
            &StreamedAssistantContent::ReasoningDelta {
                id: "rs_1".into(),
                provider_id: None,
                reasoning: "thinking".into(),
            },
            &mut index,
        )
        .is_empty());
    }
}
