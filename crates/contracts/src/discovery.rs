use serde::{Deserialize, Serialize};

pub const MODEL_DISCOVERY_SCHEMA_V1: &str = "aethra.model-discovery.v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiscoverySource {
    StaticList,
    ProviderEndpoint,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoveredModel {
    pub id: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelCacheEntry {
    pub schema: String,
    pub profile_id: String,
    pub source: DiscoverySource,
    pub models: Vec<DiscoveredModel>,
    pub fetched_at: u64,
}

impl ModelCacheEntry {
    pub fn is_fresh(&self, now_millis: u64, ttl_millis: u64) -> bool {
        now_millis.saturating_sub(self.fetched_at) < ttl_millis
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != MODEL_DISCOVERY_SCHEMA_V1 {
            return Err("unsupported discovery schema");
        }
        if self.profile_id.is_empty() {
            return Err("profile_id is required");
        }
        Ok(())
    }
}

pub fn cache_key(profile_id: &str) -> String {
    format!("model-discovery:{profile_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> ModelCacheEntry {
        ModelCacheEntry {
            schema: MODEL_DISCOVERY_SCHEMA_V1.into(),
            profile_id: "openai".into(),
            source: DiscoverySource::StaticList,
            models: vec![DiscoveredModel {
                id: "gpt-4o".into(),
                display_name: Some("GPT-4o".into()),
            }],
            fetched_at: 1000,
        }
    }

    #[test]
    fn fresh_within_ttl() {
        assert!(entry().is_fresh(2000, 5000));
    }

    #[test]
    fn stale_after_ttl() {
        assert!(!entry().is_fresh(7000, 5000));
    }

    #[test]
    fn unsupported_source_is_explicit() {
        let mut entry = entry();
        entry.source = DiscoverySource::Unsupported;
        entry.models.clear();
        entry.validate().unwrap();
        assert!(entry.models.is_empty());
    }

    #[test]
    fn empty_profile_rejected() {
        let mut entry = entry();
        entry.profile_id.clear();
        assert_eq!(entry.validate(), Err("profile_id is required"));
    }

    #[test]
    fn cache_key_scoped_to_profile() {
        assert_eq!(cache_key("openai"), "model-discovery:openai");
        assert_ne!(cache_key("openai"), cache_key("nvidia-nim"));
    }

    #[test]
    fn entry_roundtrip() {
        let json = serde_json::to_string(&entry()).unwrap();
        let decoded: ModelCacheEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry(), decoded);
    }
}
