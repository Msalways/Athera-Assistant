use assistant_contracts::model::{FinishReason, ModelMessageRole, ModelUsage};

pub fn map_finish_reason(reason: Option<&str>) -> FinishReason {
    match reason {
        Some("stop") | None => FinishReason::Stop,
        Some("length") => FinishReason::Length,
        Some("tool_calls") => FinishReason::ToolCalls,
        Some("content_filter") => FinishReason::ContentFilter,
        _ => FinishReason::Stop,
    }
}

pub fn map_usage(prompt_tokens: u32, completion_tokens: u32) -> ModelUsage {
    ModelUsage {
        prompt_tokens,
        completion_tokens,
        total_tokens: prompt_tokens + completion_tokens,
    }
}

pub fn role_to_string(role: ModelMessageRole) -> &'static str {
    match role {
        ModelMessageRole::System => "system",
        ModelMessageRole::User => "user",
        ModelMessageRole::Assistant => "assistant",
        ModelMessageRole::Tool => "tool",
    }
}

pub fn string_to_role(s: &str) -> ModelMessageRole {
    match s {
        "system" => ModelMessageRole::System,
        "user" => ModelMessageRole::User,
        "assistant" => ModelMessageRole::Assistant,
        "tool" => ModelMessageRole::Tool,
        _ => ModelMessageRole::User,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_reason_mapping() {
        assert_eq!(map_finish_reason(Some("stop")), FinishReason::Stop);
        assert_eq!(map_finish_reason(Some("length")), FinishReason::Length);
        assert_eq!(
            map_finish_reason(Some("tool_calls")),
            FinishReason::ToolCalls
        );
        assert_eq!(map_finish_reason(None), FinishReason::Stop);
    }

    #[test]
    fn usage_totals() {
        let u = map_usage(100, 50);
        assert_eq!(u.prompt_tokens, 100);
        assert_eq!(u.completion_tokens, 50);
        assert_eq!(u.total_tokens, 150);
    }

    #[test]
    fn role_roundtrip() {
        assert_eq!(role_to_string(ModelMessageRole::System), "system");
        assert_eq!(string_to_role("assistant"), ModelMessageRole::Assistant);
    }
}
