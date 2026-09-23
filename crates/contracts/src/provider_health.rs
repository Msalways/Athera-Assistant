use serde::{Deserialize, Serialize};

pub const PROVIDER_HEALTH_SCHEMA_V1: &str = "aethra.provider-health.v1";

/// Persisted provider health state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderHealth {
    pub schema: String,
    pub provider_id: String,
    pub last_tested_at: Option<u64>,
    pub last_test_success: bool,
    pub last_error: Option<NormalizedTestFailure>,
    pub capability_snapshot: CapabilitySnapshot,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum NormalizedTestFailure {
    Credential,
    Endpoint,
    ModelNotFound,
    Quota,
    Network,
    ProviderError,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilitySnapshot {
    pub streaming: bool,
    pub tool_calls: bool,
    pub vision: bool,
}

impl ProviderHealth {
    pub fn new(provider_id: impl Into<String>) -> Self {
        Self {
            schema: PROVIDER_HEALTH_SCHEMA_V1.into(),
            provider_id: provider_id.into(),
            last_tested_at: None,
            last_test_success: false,
            last_error: None,
            capability_snapshot: CapabilitySnapshot {
                streaming: false,
                tool_calls: false,
                vision: false,
            },
        }
    }

    pub fn mark_success(&mut self, now: u64, caps: CapabilitySnapshot) {
        self.last_tested_at = Some(now);
        self.last_test_success = true;
        self.last_error = None;
        self.capability_snapshot = caps;
    }

    pub fn mark_failure(&mut self, now: u64, error: NormalizedTestFailure) {
        self.last_tested_at = Some(now);
        self.last_test_success = false;
        self.last_error = Some(error);
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != PROVIDER_HEALTH_SCHEMA_V1 {
            return Err("unsupported provider health schema");
        }
        if self.provider_id.is_empty() {
            return Err("provider_id is required");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_health_is_untested() {
        let h = ProviderHealth::new("openai");
        assert!(!h.last_test_success);
        assert!(h.last_tested_at.is_none());
    }

    #[test]
    fn mark_success_records_caps() {
        let mut h = ProviderHealth::new("openai");
        h.mark_success(
            1000,
            CapabilitySnapshot {
                streaming: true,
                tool_calls: true,
                vision: false,
            },
        );
        assert!(h.last_test_success);
        assert_eq!(h.last_tested_at, Some(1000));
        assert!(h.last_error.is_none());
        assert!(h.capability_snapshot.streaming);
    }

    #[test]
    fn mark_failure_records_error() {
        let mut h = ProviderHealth::new("openai");
        h.mark_failure(2000, NormalizedTestFailure::Credential);
        assert!(!h.last_test_success);
        assert_eq!(h.last_error, Some(NormalizedTestFailure::Credential));
    }

    #[test]
    fn validate_empty_provider_rejected() {
        let h = ProviderHealth::new("");
        assert_eq!(h.validate(), Err("provider_id is required"));
    }

    #[test]
    fn health_roundtrip() {
        let mut h = ProviderHealth::new("openai");
        h.mark_success(
            1000,
            CapabilitySnapshot {
                streaming: true,
                tool_calls: true,
                vision: false,
            },
        );
        let json = serde_json::to_string(&h).unwrap();
        let decoded: ProviderHealth = serde_json::from_str(&json).unwrap();
        assert_eq!(h, decoded);
    }
}
