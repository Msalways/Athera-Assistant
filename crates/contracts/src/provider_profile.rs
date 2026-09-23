use serde::{Deserialize, Serialize};

pub const PROVIDER_PROFILE_SCHEMA_V1: &str = "aethra.provider-profile.v1";

/// A persisted provider profile. Contains only non-secret configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderProfile {
    pub schema: String,
    pub provider_id: String,
    pub auth_option_id: String,
    pub non_secret_config: serde_json::Value,
    pub enabled: bool,
    pub display_name: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl ProviderProfile {
    pub fn new(provider_id: impl Into<String>, auth_option_id: impl Into<String>) -> Self {
        let now = now_millis();
        Self {
            schema: PROVIDER_PROFILE_SCHEMA_V1.into(),
            provider_id: provider_id.into(),
            auth_option_id: auth_option_id.into(),
            non_secret_config: serde_json::json!({}),
            enabled: true,
            display_name: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != PROVIDER_PROFILE_SCHEMA_V1 {
            return Err("unsupported provider profile schema");
        }
        if self.provider_id.is_empty() {
            return Err("provider_id is required");
        }
        if self.auth_option_id.is_empty() {
            return Err("auth_option_id is required");
        }
        Ok(())
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_provider_profile_has_defaults() {
        let p = ProviderProfile::new("nvidia-nim", "api-key");
        assert_eq!(p.schema, PROVIDER_PROFILE_SCHEMA_V1);
        assert!(p.enabled);
        assert!(p.non_secret_config.is_object());
    }

    #[test]
    fn provider_profile_validates() {
        let p = ProviderProfile::new("nvidia-nim", "api-key");
        p.validate().unwrap();
    }

    #[test]
    fn empty_provider_id_rejected() {
        let p = ProviderProfile::new("", "api-key");
        assert_eq!(p.validate(), Err("provider_id is required"));
    }

    #[test]
    fn empty_auth_option_rejected() {
        let p = ProviderProfile::new("nvidia-nim", "");
        assert_eq!(p.validate(), Err("auth_option_id is required"));
    }

    #[test]
    fn provider_profile_roundtrip() {
        let mut p = ProviderProfile::new("openai", "bearer-token");
        p.non_secret_config = serde_json::json!({"base_url": "https://api.openai.com/v1"});
        p.display_name = Some("OpenAI".into());
        let json = serde_json::to_string(&p).unwrap();
        let decoded: ProviderProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(p, decoded);
    }
}
