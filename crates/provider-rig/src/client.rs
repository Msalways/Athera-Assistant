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

/// Bound on waiting for a provider to accept a request and start its reply.
///
/// Applied here rather than left to the engine's step deadline, for one reason: the
/// engine reports a blown deadline as a bare "Operation timed out", which cannot
/// distinguish a request that never left from one the provider took and then
/// abandoned. Those have different fixes — a dead network versus a key without
/// inference entitlement — and this is where the difference is still observable.
///
/// Sized below the engine's 30s step deadline so it surfaces first, with the fault
/// named, instead of being flattened on the way out.
const PROVIDER_FIRST_RESPONSE: std::time::Duration = std::time::Duration::from_secs(25);

fn build_client(
    base_url: &str,
    raw_secret: &str,
) -> Result<openai::CompletionsClient, RigAdapterError> {
    openai::CompletionsClient::builder()
        .api_key(raw_secret)
        .base_url(base_url)
        .build()
        .map_err(|e| RigAdapterError::RequestFailed(e.to_string()))
}

pub async fn complete_openai_compatible(
    base_url: &str,
    raw_secret: &str,
    request: &ModelRequest,
) -> Result<ModelResponse, RigAdapterError> {
    let rig_request = build_rig_request(request)?;
    let client = build_client(base_url, raw_secret)?;
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
    let client = build_client(base_url, raw_secret)?;
    let model = client.completion_model(request.model.clone());
    run_stream(model, rig_request, &request.model, sink).await
}

pub(crate) async fn run_stream<M: CompletionModel>(
    model: M,
    request: rig_core::completion::CompletionRequest,
    model_id: &str,
    sink: &ProviderEventSink,
) -> Result<ModelResponse, RigAdapterError> {
    // Bound the wait for the FIRST item, not just for the stream to open. A
    // provider that accepts a request and then goes quiet still returns response
    // headers promptly - it simply never sends a body - so a deadline around
    // opening the stream expires in milliseconds and the real stall lands on the
    // first read instead. Bounding only this first item keeps a long but
    // progressing answer from being cut off, which bounding the whole exchange
    // would do.
    let deadline = tokio::time::Instant::now() + PROVIDER_FIRST_RESPONSE;
    let mut stream = tokio::time::timeout_at(deadline, model.stream(request))
        .await
        .map_err(|_| RigAdapterError::Normalized(NormalizedError::NoResponse))?
        .map_err(normalize_error)?;
    // Read the first item here so it can be covered by the same deadline, then
    // hand it to the loop below so the rest of the stream is unbounded.
    let first = tokio::time::timeout_at(deadline, stream.next())
        .await
        .map_err(|_| RigAdapterError::Normalized(NormalizedError::NoResponse))?;
    let mut content = String::new();
    let mut tool_calls = Vec::new();
    let mut tool_index = 0usize;
    let mut usage = ModelUsage {
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
    };
    let mut finish_reason = FinishReason::Stop;
    // The first item was already read under the deadline above, so it is handled
    // here and the loop takes over from the second one onwards. Chained rather
    // than duplicated so the two paths cannot drift apart.
    let mut pending = first;
    while let Some(item) = pending.take().or(stream.next().await) {
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

/// Every layer of a transport failure, outermost first.
///
/// An HTTP client reports a failure to put a request on the wire with one
/// uninformative wrapper phrase and hides the actual cause behind it. "Error
/// sending request for url" names the URL and nothing else; the reason is one
/// `source()` link deeper, and that link is the only place a certificate
/// failure, a refused connection, and an unresolvable name can be told apart.
/// Reading only the outermost message is what makes all three look identical.
fn transport_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut parts = vec![error.to_string()];
    let mut current = error.source();
    while let Some(cause) = current {
        let text = cause.to_string();
        // A repeated message adds nothing and signals a cycle worth breaking.
        if parts.last().is_some_and(|last| last == &text) {
            break;
        }
        parts.push(text);
        current = cause.source();
        // Bounded because a deep chain is a bug worth truncating, not worth
        // letting a provider's behaviour decide how much memory a log line uses.
        if parts.len() >= 8 {
            break;
        }
    }
    parts.join(": ")
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
            map_transport_failure(&transport_chain(inner.as_ref()))
        }
        CompletionError::HttpError(other) => NormalizedError::InvalidResponse {
            detail: transport_chain(&other),
        },
        CompletionError::ProviderResponse(error) => {
            let status = error
                .status
                .map(|status| status.as_u16())
                .filter(|status| *status >= 400)
                .or_else(|| embedded_error_status(&error.body));
            match status {
                Some(status) => map_status(status, None, error.body),
                // A response arrived, so the provider did say something — we just
                // could not read a status off it. Its words are still its words and
                // still worth showing.
                None => NormalizedError::ProviderError {
                    status: 0,
                    detail: provider_text(&error.body),
                },
            }
        }
        CompletionError::ResponseError(detail) => NormalizedError::InvalidResponse { detail },
        // Carries no HTTP response, so nothing reached us to report and nothing
        // said anything to us. That makes it a transport fault, and the request
        // may never have left the device — attributing it to the provider would
        // put a raw client-library string on screen as though the provider had
        // said it, and would point the user at their provider rather than at
        // their network.
        CompletionError::ProviderError(detail) => map_transport_failure(&detail),
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

fn embedded_error_status(body: &str) -> Option<u16> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let code = value.pointer("/error/code")?.as_u64()?;
    u16::try_from(code)
        .ok()
        .filter(|status| (400..=599).contains(status))
}

fn map_status(status: u16, retry_after_secs: Option<u64>, detail: String) -> NormalizedError {
    match status {
        401 => NormalizedError::AuthenticationFailed,
        403 => NormalizedError::AuthorizationDenied,
        // Providers answer 404 for an unknown model and 410 Gone for one that
        // has been retired. Both mean the caller must choose a different model.
        404 | 410 => NormalizedError::ModelNotFound,
        // The provider's own words are kept here because a limit is not a status
        // code. Only the provider knows whether this clears in a minute or
        // tomorrow, and whether credits, a plan, or a daily ceiling is what ran
        // out. Dropping that turns "add 10 credits" into "try again later".
        429 => NormalizedError::RateLimited {
            retry_after_secs,
            detail: provider_text(&detail),
        },
        // Everything else the provider rejected or failed is reported as itself,
        // with its explanation intact. Folding these into a single lossy category
        // is what made a 400 — the provider refusing to parse the request — read
        // to the user as though the provider were unreachable.
        _ => NormalizedError::ProviderError {
            status,
            detail: provider_text(&detail),
        },
    }
}

/// The provider's own account of a fault, in the words it used.
///
/// Providers disagree about where they put this, so the common shapes are tried
/// in turn before falling back to the raw body. Returning the raw body last
/// matters: an unrecognised shape is still the provider's own words, and showing
/// them beats inventing a summary that is wrong.
fn provider_text(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let value: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(value) => value,
        Err(_) => return assistant_contracts::blocker::bounded_detail(trimmed),
    };
    // `remedy_hint` is where providers put the action a user can take, and it is
    // often absent from the headline message, so it is appended when present.
    let remedy = value
        .pointer("/error/metadata/remedy_hint")
        .and_then(serde_json::Value::as_str);
    let headline = ["/error/message", "/message", "/detail", "/error"]
        .into_iter()
        .find_map(|pointer| match value.pointer(pointer) {
            Some(serde_json::Value::String(text)) => Some(text.as_str()),
            _ => None,
        });
    let text = match headline {
        Some(headline) => match remedy {
            // The provider wrote these as two sentences and did not punctuate
            // between them. Left as-is they run together into one garbled
            // sentence, which undoes the clarity being preserved here. A headline
            // that already ends in punctuation is left alone, so a full stop does
            // not become two.
            Some(remedy) if !remedy.trim().is_empty() => {
                if headline.trim_end().ends_with(['.', '!', '?', ':']) {
                    format!("{headline} {remedy}")
                } else {
                    format!("{headline}. {remedy}")
                }
            }
            _ => headline.to_owned(),
        },
        None => trimmed.to_owned(),
    };
    assistant_contracts::blocker::bounded_detail(text.trim())
}

/// Whether a transport failure was our own request deadline expiring.
///
/// Matched on the wording because `reqwest` reports an expired deadline as a
/// boxed error rather than a distinct variant, and connecting a timeout to DNS
/// or to a refused connection would be wrong. `connect_timeout` is deliberately
/// excluded: a host that never accepts the connection never took the request, so
/// that case really is unreachable rather than unanswered.
fn is_request_timeout(message: &str) -> bool {
    let lowered = message.to_ascii_lowercase();
    // Wording that names the connection itself means the request was never
    // delivered, which is an unreachable host and not a provider that went quiet.
    if lowered.contains("operation timed out")
        || lowered.contains("connect timeout")
        || lowered.contains("connection timed out")
    {
        return false;
    }
    lowered.contains("timed out") || lowered.contains("timeout")
}

fn map_transport_failure(message: &str) -> NormalizedError {
    // Kept in full, because this is the only place the reason exists. A
    // connectivity fault that reports only "unreachable" cannot be told apart
    // from a certificate that will not validate or a proxy that intercepts, and
    // those have nothing in common but the inability to answer.
    let detail = assistant_contracts::blocker::bounded_detail(message);
    // An expired request deadline is not a dropped connection. The request was
    // already delivered and the provider chose not to answer, which is what an
    // account without inference entitlement looks like from the client side: the
    // connection stays open and nothing ever comes back. Checked before the shared
    // classifier so it is not flattened into a generic network fault, which would
    // point the user at their Wi-Fi.
    if is_request_timeout(message) {
        return NormalizedError::NoResponse;
    }
    match ConnectionTestService::classify_connection_error(message) {
        NormalizedTestFailure::Credential => NormalizedError::AuthenticationFailed,
        NormalizedTestFailure::Endpoint => NormalizedError::EndpointNotFound,
        NormalizedTestFailure::ModelNotFound => NormalizedError::ModelNotFound,
        NormalizedTestFailure::Quota => NormalizedError::QuotaExceeded,
        NormalizedTestFailure::Network => NormalizedError::NetworkUnavailable { detail },
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
                retry_after_secs: Some(60),
                detail: String::new(),
            }
        );
    }

    /// A provider that explained its limit explained it in a way no status code
    /// can reproduce, so the explanation has to survive normalization.
    #[test]
    fn an_in_band_error_code_normalizes_and_keeps_the_providers_reason() {
        let body = r#"{"error":{"code":429,"message":"free-models-per-day: add 10 credits"}}"#;
        let error = rig_core::completion::CompletionError::ProviderResponse(
            rig_core::ProviderResponseError::without_status(body),
        );
        assert_eq!(
            map_completion_error(error),
            NormalizedError::RateLimited {
                retry_after_secs: None,
                detail: "free-models-per-day: add 10 credits".into(),
            }
        );
        assert_eq!(embedded_error_status(body), Some(429));
        assert_eq!(
            embedded_error_status(r#"{"error":{"code":"server_error"}}"#),
            None
        );
    }

    #[test]
    fn retired_models_410_map_to_model_not_found() {
        assert_eq!(
            map_status(410, None, String::new()),
            NormalizedError::ModelNotFound
        );
    }

    /// Taken from a real provider reply. The remedy was in `metadata`, not in the
    /// headline message, and it is the part that tells the user what to do.
    #[test]
    fn a_providers_limit_is_read_from_the_shape_it_actually_sends() {
        let body = r#"{"error":{"message":"Rate limit exceeded: free-models-per-day. Add 10 credits to unlock 1000 free model requests per day","code":429,"metadata":{"headers":{"X-RateLimit-Reset":"1790726400000"},"limit_source":"openrouter_free_tier_daily","remedy_hint":"Wait for the daily reset (see X-RateLimit-Reset), or purchase credits to raise your free model daily limit."}}}"#;
        assert_eq!(
            map_status(429, None, body.to_owned()),
            NormalizedError::RateLimited {
                retry_after_secs: None,
                detail: "Rate limit exceeded: free-models-per-day. Add 10 credits to unlock 1000 free model requests per day. Wait for the daily reset (see X-RateLimit-Reset), or purchase credits to raise your free model daily limit.".into(),
            }
        );
    }

    /// A provider that names the parameter it refused has diagnosed the problem
    /// better than a status code can, so the refusal is reported as itself.
    #[test]
    fn a_rejected_request_keeps_the_providers_objection() {
        let body =
            r#"{"error":{"message":"'temperature' is not supported by this model","code":400}}"#;
        assert_eq!(
            map_status(400, None, body.to_owned()),
            NormalizedError::ProviderError {
                status: 400,
                detail: "'temperature' is not supported by this model".into(),
            }
        );
    }

    /// The shapes providers actually use, plus a body that is not JSON at all.
    #[test]
    fn the_providers_own_words_survive_every_common_shape() {
        for (body, expected) in [
            (
                r#"{"error":{"message":"from error.message"}}"#,
                "from error.message",
            ),
            (r#"{"message":"from message"}"#, "from message"),
            (r#"{"detail":"from detail"}"#, "from detail"),
            (
                r#"{"error":"from error as a string"}"#,
                "from error as a string",
            ),
            ("plain text failure", "plain text failure"),
        ] {
            assert_eq!(provider_text(body), expected, "body was {body}");
        }
        // An unrecognised JSON shape still yields the provider's own bytes rather
        // than a summary nobody wrote.
        assert!(provider_text(r#"{"unexpected":{"nested":"value"}}"#).contains("nested"));
        assert_eq!(provider_text("   "), "");
    }

    /// Two sentences the provider wrote separately must not be welded into one.
    #[test]
    fn a_remedy_hint_is_joined_as_a_sentence() {
        let with_remedy = r#"{"error":{"message":"Limit reached for today.","metadata":{"remedy_hint":"Add credits."}}}"#;
        assert_eq!(
            provider_text(with_remedy),
            "Limit reached for today. Add credits."
        );
        // An unpunctuated headline gains the full stop rather than running on.
        let unpunctuated =
            r#"{"error":{"message":"Limit reached","metadata":{"remedy_hint":"Add credits."}}}"#;
        assert_eq!(provider_text(unpunctuated), "Limit reached. Add credits.");
        // A headline that already ends in a full stop must not gain a second.
        let already = r#"{"error":{"message":"Done.","metadata":{"remedy_hint":"Now retry."}}}"#;
        assert_eq!(provider_text(already), "Done. Now retry.");
        // No remedy, nothing invented.
        let bare = r#"{"error":{"message":"Limit reached"}}"#;
        assert_eq!(provider_text(bare), "Limit reached");
    }

    /// The distinction that keeps a fault honest: a fault the provider described
    /// is the provider's fault, and a failure to reach one at all is not.
    ///
    /// Found on a real device: a dropped connection surfaced as
    /// "openrouter reported: Http client error: error sending request for url",
    /// which reads as though the provider had answered and blames the provider for
    /// something that never got off the device.
    #[test]
    fn a_failure_to_reach_a_provider_is_not_attributed_to_that_provider() {
        let error = rig_core::completion::CompletionError::ProviderError(
            "Http client error: error sending request for url (https://example.invalid/v1/chat/completions)"
                .into(),
        );
        let normalized = map_completion_error(error);
        // The reason survives - that is the point of carrying it - but it is
        // reachable only through the transport channel, never through
        // `provider_text`, which is reserved for words a provider actually sent.
        assert!(
            !normalized
                .provider_text()
                .is_some_and(|text| text.contains("error sending request")),
            "a client-library string must never be shown as a provider's own words, got {normalized:?}"
        );
        assert!(
            matches!(
                normalized,
                NormalizedError::NetworkUnavailable { .. }
            ),
            "a request that never reached the provider is an unreachable provider, not a provider that spoke, got {normalized:?}"
        );
        assert!(
            normalized
                .reason_text()
                .is_some_and(|text| text.contains("error sending request for url")),
            "the transport reason must be retained for the action log, got {normalized:?}"
        );
    }

    /// A response that arrived without a readable status is still the provider
    /// replying, so its words are kept rather than reclassified as a dead network.
    #[test]
    fn a_response_without_a_readable_status_keeps_the_providers_words() {
        let body = r#"{"detail":"gateway returned an unrecognised body"}"#;
        let error = rig_core::completion::CompletionError::ProviderResponse(
            rig_core::ProviderResponseError::without_status(body),
        );
        assert_eq!(
            map_completion_error(error),
            NormalizedError::ProviderError {
                status: 0,
                detail: "gateway returned an unrecognised body".into(),
            }
        );
    }

    #[test]
    fn a_deep_error_chain_is_read_to_its_root_not_just_its_wrapper() {
        // Mirrors how an HTTP client reports a failure: an uninformative outer
        // phrase with the real cause one link deeper. Reading only the outer
        // message is what leaves a certificate failure looking like any other.
        #[derive(Debug)]
        struct Root;
        impl std::fmt::Display for Root {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("invalid peer certificate: unable to get local issuer")
            }
        }
        impl std::error::Error for Root {}
        #[derive(Debug)]
        struct Middle(Root);
        impl std::fmt::Display for Middle {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("client error (Connect)")
            }
        }
        impl std::error::Error for Middle {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }
        #[derive(Debug)]
        struct Outer(Middle);
        impl std::fmt::Display for Outer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("error sending request for url (https://example.invalid)")
            }
        }
        impl std::error::Error for Outer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        let chained = transport_chain(&Outer(Middle(Root)));
        assert!(chained.contains("error sending request for url"));
        assert!(
            chained.contains("unable to get local issuer"),
            "the root cause must survive, got: {chained}"
        );
        // Both layers are present, so a reader can see the symptom and the cause
        // rather than having to guess which one the classifier acted on.
        assert!(chained.contains("client error (Connect)"));
    }

    #[test]
    fn a_repeated_message_does_not_loop_forever() {
        #[derive(Debug)]
        struct Loop;
        impl std::fmt::Display for Loop {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("same")
            }
        }
        impl std::error::Error for Loop {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(self)
            }
        }
        assert_eq!(transport_chain(&Loop), "same");
    }

    /// A provider echoing a prompt back inside its error must not be able to grow
    /// the fault text without bound, and the cut must be visible rather than
    /// silently presenting a partial sentence as the provider's whole reason.
    #[test]
    fn an_oversized_providers_message_is_bounded_and_says_so() {
        let limit = assistant_contracts::blocker::DIAGNOSTIC_DETAIL_LIMIT;
        let body = serde_json::json!({ "error": { "message": "x".repeat(9_000) } }).to_string();
        let text = provider_text(&body);
        assert!(
            text.len() <= limit + "... (truncated)".len(),
            "grew to {} bytes",
            text.len()
        );
        assert!(text.ends_with("... (truncated)"));
    }

    /// A connection that was never established is a network fault, and stays one.
    #[test]
    fn an_unestablished_connection_is_still_reported_as_the_network() {
        for message in [
            "operation timed out",
            "connect timeout",
            "connection timed out",
        ] {
            assert_eq!(
                map_transport_failure(message),
                NormalizedError::NetworkUnavailable {
                    detail: message.into(),
                },
                "{message} never reached the provider, so it is not a silent provider"
            );
        }
    }

    /// A request deadline expiring means the connection was established and the
    /// request delivered, with no reply. Reported as itself.
    ///
    /// Found on a device: NVIDIA accepted the request with a valid key and held
    /// the connection open without replying, and the user was told to check their
    /// network — which the same request had already proven was fine.
    #[test]
    fn an_expired_request_deadline_is_a_provider_that_went_quiet() {
        assert_eq!(
            map_transport_failure("operation error: request timed out"),
            NormalizedError::NoResponse
        );
        assert_eq!(
            map_transport_failure("error sending request for url (...): timeout"),
            NormalizedError::NoResponse
        );
    }

    /// The quiet-provider fault must not send the turn to a fallback provider: the
    /// request arrived, so a different key would be ignored the same way, and
    /// switching would hide a setup mistake behind a working-looking answer.
    #[test]
    fn a_quiet_provider_never_fails_over() {
        assert!(
            !assistant_contracts::failover::may_fail_over(&NormalizedError::NoResponse),
            "a provider that took the request and went quiet is not a reason to try someone else"
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
