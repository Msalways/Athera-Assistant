//! Normalized model request, response, usage, and error contracts.
//! These types sit between the orchestration layer and provider adapters.
//! Provider adapters translate their vendor-specific formats into these types.
use serde::{Deserialize, Serialize};

/// Schema version for model contracts.
pub const MODEL_REQUEST_SCHEMA_V1: &str = "aethra.model-request.v1";

/// A normalized request sent to a model provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelRequest {
    pub schema: String,
    pub messages: Vec<ModelMessage>,
    pub tools: Vec<ToolDefinition>,
    pub model: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: bool,
}

impl ModelRequest {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != MODEL_REQUEST_SCHEMA_V1 {
            return Err("unsupported model request schema");
        }
        if self.messages.is_empty() {
            return Err("at least one message is required");
        }
        if self.model.is_empty() {
            return Err("model is required");
        }
        Ok(())
    }
}

/// A single message in the model context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelMessage {
    pub role: ModelMessageRole,
    pub content: String,
    #[serde(default)]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A tool definition exposed to the model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// The outcome of a model inference call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCallRequest>,
    pub usage: ModelUsage,
    pub finish_reason: FinishReason,
    pub model_id: String,
}

/// A tool call requested by the model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Token usage reported by the model.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredModelUsage {
    pub task_id: String,
    pub usage: ModelUsage,
    pub model_id: String,
    pub recorded_at: u64,
}

impl StoredModelUsage {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.task_id.is_empty() {
            return Err("task_id is required");
        }
        if self.usage.total_tokens != self.usage.prompt_tokens + self.usage.completion_tokens {
            return Err("usage total must equal prompt plus completion");
        }
        Ok(())
    }
}

/// Why the model stopped generating.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    Error,
}

/// A streaming delta from a model provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelStreamDelta {
    ContentDelta {
        text: String,
    },
    ToolCallDelta {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        arguments_delta: String,
    },
    UsageDelta {
        usage: ModelUsage,
    },
    StreamEnd {
        finish_reason: FinishReason,
    },
}

/// Normalized provider error. Adapters map vendor-specific errors into this type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NormalizedError {
    AuthenticationFailed,
    AuthorizationDenied,
    EndpointNotFound,
    ModelNotFound,
    RateLimited { retry_after_secs: Option<u64> },
    QuotaExceeded,
    Timeout,
    NetworkUnavailable,
    InvalidResponse { detail: String },
    ProviderError { status: u16, detail: String },
}

impl NormalizedError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. } | Self::Timeout | Self::NetworkUnavailable
        )
    }

    pub fn is_credential_error(&self) -> bool {
        matches!(self, Self::AuthenticationFailed | Self::AuthorizationDenied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_request_roundtrip() {
        let req = ModelRequest {
            schema: MODEL_REQUEST_SCHEMA_V1.into(),
            messages: vec![ModelMessage {
                role: ModelMessageRole::User,
                content: "hello".into(),
                tool_call_id: None,
            }],
            tools: vec![],
            model: "gpt-4".into(),
            max_tokens: Some(1024),
            temperature: None,
            stream: false,
        };
        req.validate().unwrap();
        let json = serde_json::to_string(&req).unwrap();
        let decoded: ModelRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn normalized_error_retryable() {
        assert!(NormalizedError::RateLimited {
            retry_after_secs: Some(60)
        }
        .is_retryable());
        assert!(NormalizedError::Timeout.is_retryable());
        assert!(!NormalizedError::AuthenticationFailed.is_retryable());
    }

    #[test]
    fn finish_reason_roundtrip() {
        let json = serde_json::to_string(&FinishReason::ToolCalls).unwrap();
        let decoded: FinishReason = serde_json::from_str(&json).unwrap();
        assert_eq!(FinishReason::ToolCalls, decoded);
    }

    #[test]
    fn stored_usage_validates_totals() {
        let record = StoredModelUsage {
            task_id: "task-1".into(),
            usage: ModelUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
            model_id: "gpt-4o".into(),
            recorded_at: 1000,
        };
        record.validate().unwrap();
        let json = serde_json::to_string(&record).unwrap();
        let decoded: StoredModelUsage = serde_json::from_str(&json).unwrap();
        assert_eq!(record, decoded);
    }

    #[test]
    fn stored_usage_rejects_bad_total() {
        let record = StoredModelUsage {
            task_id: "task-1".into(),
            usage: ModelUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 999,
            },
            model_id: "gpt-4o".into(),
            recorded_at: 1000,
        };
        assert_eq!(
            record.validate(),
            Err("usage total must equal prompt plus completion")
        );
    }
}
