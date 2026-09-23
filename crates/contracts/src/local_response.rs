use crate::model::{FinishReason, ModelResponse, ModelUsage};

pub const LOCAL_DETERMINISTIC_MODEL_ID: &str = "local-deterministic";

pub fn is_greeting(text: &str) -> bool {
    let normalized = text
        .trim()
        .trim_end_matches(['?', '!', '.', ','])
        .to_lowercase();
    if normalized.is_empty() || normalized.len() > 64 {
        return false;
    }
    let stripped = normalized.strip_prefix("good ").unwrap_or(&normalized);
    let first = stripped.split_whitespace().next().unwrap_or_default();
    matches!(
        first,
        "hi" | "hello" | "hey" | "yo" | "hiya" | "namaste" | "morning" | "afternoon" | "evening"
    )
}

pub fn greeting_response() -> ModelResponse {
    ModelResponse {
        content: "Hello! How can I help?".into(),
        tool_calls: vec![],
        usage: ModelUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        },
        finish_reason: FinishReason::Stop,
        model_id: LOCAL_DETERMINISTIC_MODEL_ID.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_greetings() {
        for text in [
            "hi",
            "Hi",
            "  hello  ",
            "hey!",
            "yo?",
            "Good morning",
            "namaste,",
        ] {
            assert!(is_greeting(text), "missed greeting: {text}");
        }
    }

    #[test]
    fn rejects_lookalikes() {
        for text in [
            "",
            "high priority task",
            "history of Rome",
            "this is urgent",
            "highlight the text",
            "ship the package",
            "hi, please message Arun that I am late for dinner tonight",
        ] {
            assert!(!is_greeting(text), "false positive: {text}");
        }
    }

    #[test]
    fn greeting_needs_no_model_or_tools() {
        let response = greeting_response();
        assert!(!response.content.is_empty());
        assert!(response.tool_calls.is_empty());
        assert_eq!(response.finish_reason, FinishReason::Stop);
        assert_eq!(response.model_id, LOCAL_DETERMINISTIC_MODEL_ID);
        assert_eq!(response.usage.total_tokens, 0);
    }

    #[test]
    fn greeting_roundtrip() {
        let response = greeting_response();
        let json = serde_json::to_string(&response).unwrap();
        let decoded: ModelResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, decoded);
    }
}
