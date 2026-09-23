use serde::{Deserialize, Serialize};

pub const CONFORMANCE_CASSETTE_SCHEMA_V1: &str = "aethra.conformance-cassette.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScrubbedCassette {
    pub schema: String,
    pub name: String,
    pub request: crate::model::ModelRequest,
    pub response: Option<crate::model::ModelResponse>,
    pub deltas: Vec<crate::model::ModelStreamDelta>,
    pub error: Option<crate::model::NormalizedError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceReport {
    pub passed: usize,
    pub failed: Vec<String>,
}

impl ScrubbedCassette {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != CONFORMANCE_CASSETTE_SCHEMA_V1 {
            return Err("unsupported cassette schema");
        }
        if self.name.is_empty() {
            return Err("cassette name is required");
        }
        self.request.validate()?;
        if let Some(response) = &self.response {
            if response.model_id.is_empty() {
                return Err("response model_id is required");
            }
            if response.usage.total_tokens
                != response.usage.prompt_tokens + response.usage.completion_tokens
            {
                return Err("usage total must equal prompt plus completion");
            }
        }
        if self.response.is_none() && self.error.is_none() {
            return Err("cassette must carry a response or an error");
        }
        let json = serde_json::to_value(self).map_err(|_| "cassette not serializable")?;
        scrub_value(&json)?;
        Ok(())
    }
}

fn scrub_value(value: &serde_json::Value) -> Result<(), &'static str> {
    match value {
        serde_json::Value::String(s) => {
            let lower = s.to_lowercase();
            for marker in ["sk-", "Bearer ", "x-api-key", "authorization:"] {
                if lower.contains(&marker.to_lowercase()) {
                    return Err("cassette contains credential material");
                }
            }
            Ok(())
        }
        serde_json::Value::Array(items) => {
            for item in items {
                scrub_value(item)?;
            }
            Ok(())
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                let lower = key.to_lowercase();
                if [
                    "api_key",
                    "apikey",
                    "secret",
                    "client_secret",
                    "access_token",
                    "refresh_token",
                    "auth_token",
                    "id_token",
                    "authorization",
                    "password",
                    "private_key",
                    "session_key",
                ]
                .contains(&lower.as_str())
                {
                    return Err("cassette contains secret field name");
                }
                scrub_value(item)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub fn replay_without_credentials(
    cassette: &ScrubbedCassette,
) -> Result<ReplayOutcome, &'static str> {
    cassette.validate()?;
    Ok(ReplayOutcome {
        name: cassette.name.clone(),
        has_response: cassette.response.is_some(),
        delta_count: cassette.deltas.len(),
        has_error: cassette.error.is_some(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayOutcome {
    pub name: String,
    pub has_response: bool,
    pub delta_count: usize,
    pub has_error: bool,
}

pub fn run_suite(cassettes: &[ScrubbedCassette]) -> ConformanceReport {
    let mut passed = 0;
    let mut failed = Vec::new();
    for cassette in cassettes {
        match replay_without_credentials(cassette) {
            Ok(_) => passed += 1,
            Err(e) => failed.push(format!("{}: {e}", cassette.name)),
        }
    }
    ConformanceReport { passed, failed }
}

pub fn sample_cassettes() -> Vec<ScrubbedCassette> {
    use crate::model::*;
    let base_request = |model: &str, stream: bool, tools: Vec<ToolDefinition>| ModelRequest {
        schema: MODEL_REQUEST_SCHEMA_V1.into(),
        messages: vec![ModelMessage {
            role: ModelMessageRole::User,
            content: "hello".into(),
            tool_call_id: None,
        }],
        tools,
        model: model.into(),
        max_tokens: Some(64),
        temperature: None,
        stream,
    };
    let base_usage = ModelUsage {
        prompt_tokens: 10,
        completion_tokens: 5,
        total_tokens: 15,
    };
    vec![
        ScrubbedCassette {
            schema: CONFORMANCE_CASSETTE_SCHEMA_V1.into(),
            name: "text".into(),
            request: base_request("gpt-4o", false, vec![]),
            response: Some(ModelResponse {
                content: "hi".into(),
                tool_calls: vec![],
                usage: base_usage,
                finish_reason: FinishReason::Stop,
                model_id: "gpt-4o".into(),
            }),
            deltas: vec![],
            error: None,
        },
        ScrubbedCassette {
            schema: CONFORMANCE_CASSETTE_SCHEMA_V1.into(),
            name: "streaming".into(),
            request: base_request("gpt-4o", true, vec![]),
            response: Some(ModelResponse {
                content: "hi".into(),
                tool_calls: vec![],
                usage: base_usage,
                finish_reason: FinishReason::Stop,
                model_id: "gpt-4o".into(),
            }),
            deltas: vec![
                ModelStreamDelta::ContentDelta { text: "h".into() },
                ModelStreamDelta::ContentDelta { text: "i".into() },
                ModelStreamDelta::UsageDelta { usage: base_usage },
                ModelStreamDelta::StreamEnd {
                    finish_reason: FinishReason::Stop,
                },
            ],
            error: None,
        },
        ScrubbedCassette {
            schema: CONFORMANCE_CASSETTE_SCHEMA_V1.into(),
            name: "tool".into(),
            request: base_request(
                "gpt-4o",
                false,
                vec![ToolDefinition {
                    id: "get_time".into(),
                    name: "get_time".into(),
                    description: "current time".into(),
                    input_schema: serde_json::json!({"type": "object"}),
                }],
            ),
            response: Some(ModelResponse {
                content: String::new(),
                tool_calls: vec![ToolCallRequest {
                    id: "call_1".into(),
                    name: "get_time".into(),
                    arguments: serde_json::json!({}),
                }],
                usage: base_usage,
                finish_reason: FinishReason::ToolCalls,
                model_id: "gpt-4o".into(),
            }),
            deltas: vec![],
            error: None,
        },
        ScrubbedCassette {
            schema: CONFORMANCE_CASSETTE_SCHEMA_V1.into(),
            name: "error".into(),
            request: base_request("missing-model", false, vec![]),
            response: None,
            deltas: vec![],
            error: Some(NormalizedError::ModelNotFound),
        },
        ScrubbedCassette {
            schema: CONFORMANCE_CASSETTE_SCHEMA_V1.into(),
            name: "usage".into(),
            request: base_request("gpt-4o", false, vec![]),
            response: Some(ModelResponse {
                content: "hi".into(),
                tool_calls: vec![],
                usage: ModelUsage {
                    prompt_tokens: 100,
                    completion_tokens: 50,
                    total_tokens: 150,
                },
                finish_reason: FinishReason::Stop,
                model_id: "gpt-4o".into(),
            }),
            deltas: vec![],
            error: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_suite_passes_without_credentials() {
        let cassettes = sample_cassettes();
        assert_eq!(cassettes.len(), 5);
        let report = run_suite(&cassettes);
        assert!(report.failed.is_empty());
        assert_eq!(report.passed, 5);
    }

    #[test]
    fn credential_material_rejected() {
        let mut cassettes = sample_cassettes();
        if let Some(response) = cassettes[0].response.as_mut() {
            response.content = "Bearer sk-test".into();
        }
        let report = run_suite(&cassettes);
        assert_eq!(report.passed, 4);
        assert_eq!(report.failed.len(), 1);
    }

    #[test]
    fn bad_usage_rejected() {
        let mut cassettes = sample_cassettes();
        if let Some(response) = cassettes[0].response.as_mut() {
            response.usage.total_tokens = 999;
        }
        assert!(cassettes[0].validate().is_err());
    }

    #[test]
    fn cassette_roundtrip() {
        let cassette = &sample_cassettes()[0];
        let json = serde_json::to_string(cassette).unwrap();
        let decoded: ScrubbedCassette = serde_json::from_str(&json).unwrap();
        assert_eq!(cassette, &decoded);
    }
}
