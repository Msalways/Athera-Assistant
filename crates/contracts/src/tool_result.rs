//! Persisted tool results keep raw provider data separate from bounded model context.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const TOOL_RESULT_SCHEMA_V1: &str = "aethra.tool-result.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ToolResultRecord {
    pub schema: String,
    pub source: String,
    pub model_context: Value,
    pub raw: Value,
}

impl ToolResultRecord {
    pub fn validate(&self) -> Result<()> {
        if self.schema != TOOL_RESULT_SCHEMA_V1
            || self.source.trim().is_empty()
            || self.source.len() > 200
            || serde_json::to_vec(&self.model_context)
                .map_err(|_| Error::InvalidResponse)?
                .len()
                > 32_000
            || serde_json::to_vec(&self.raw)
                .map_err(|_| Error::InvalidResponse)?
                .len()
                > 1_000_000
        {
            return Err(Error::InvalidResponse);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WebSource {
    pub id: String,
    pub title: String,
    pub url: String,
    pub untrusted_excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WebToolContext {
    pub kind: String,
    pub query: Option<String>,
    pub retrieved_at: u64,
    pub partial: bool,
    pub sources: Vec<WebSource>,
}

impl WebToolContext {
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.kind.as_str(), "search" | "open")
            || self.query.as_ref().is_some_and(|query| query.len() > 2_000)
            || self.sources.is_empty()
            || self.sources.len() > 12
        {
            return Err(Error::InvalidResponse);
        }
        for source in &self.sources {
            if source.id.is_empty()
                || source.id.len() > 100
                || source.title.trim().is_empty()
                || source.title.len() > 300
                || source.url.len() > 2_048
                || !source.url.starts_with("https://")
                || source.untrusted_excerpt.trim().is_empty()
                || source.untrusted_excerpt.len() > 4_000
            {
                return Err(Error::InvalidResponse);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bounds_raw_and_model_visible_tool_data_separately() {
        let record = ToolResultRecord {
            schema: TOOL_RESULT_SCHEMA_V1.into(),
            source: "web.search".into(),
            model_context: json!({"sources":[]}),
            raw: json!({"private_provider_field":"kept outside model context"}),
        };
        assert_eq!(record.validate(), Ok(()));
        let mut oversized = record;
        oversized.model_context = json!("x".repeat(32_001));
        assert_eq!(oversized.validate(), Err(Error::InvalidResponse));
    }
}
