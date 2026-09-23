use crate::credential::CredentialPurpose;
use serde::{Deserialize, Serialize};

pub const CREDENTIAL_METADATA_SCHEMA_V1: &str = "aethra.credential-metadata.v1";

/// Persisted credential metadata. Never stores plaintext secrets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CredentialMetadata {
    pub schema: String,
    pub credential_handle: String,
    pub owner_id: String,
    pub purpose: CredentialPurpose,
    pub provider_id: Option<String>,
    pub connection_id: Option<String>,
    pub created_at: u64,
    pub expires_at: Option<u64>,
    pub last_verified_at: Option<u64>,
    pub status: CredentialLifecycleStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CredentialLifecycleStatus {
    Active,
    Expired,
    Revoked,
    Error,
}

impl CredentialMetadata {
    pub fn new(
        handle: impl Into<String>,
        owner_id: impl Into<String>,
        purpose: CredentialPurpose,
    ) -> Self {
        let now = now_millis();
        Self {
            schema: CREDENTIAL_METADATA_SCHEMA_V1.into(),
            credential_handle: handle.into(),
            owner_id: owner_id.into(),
            purpose,
            provider_id: None,
            connection_id: None,
            created_at: now,
            expires_at: None,
            last_verified_at: None,
            status: CredentialLifecycleStatus::Active,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != CREDENTIAL_METADATA_SCHEMA_V1 {
            return Err("unsupported credential metadata schema");
        }
        if self.credential_handle.is_empty() {
            return Err("credential_handle is required");
        }
        if self.owner_id.is_empty() {
            return Err("owner_id is required");
        }
        if self.provider_id.is_none() && self.connection_id.is_none() {
            return Err("either provider_id or connection_id is required");
        }
        Ok(())
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|e| e <= now_millis())
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
    fn credential_metadata_for_provider() {
        let mut m = CredentialMetadata::new("handle-1", "nvidia", CredentialPurpose::ProviderAuth);
        m.provider_id = Some("nvidia".into());
        m.validate().unwrap();
        assert_eq!(m.status, CredentialLifecycleStatus::Active);
        assert!(m.connection_id.is_none());
    }

    #[test]
    fn credential_metadata_validates() {
        let mut m = CredentialMetadata::new("h1", "nvidia", CredentialPurpose::ProviderAuth);
        m.provider_id = Some("nvidia".into());
        m.validate().unwrap();
    }

    #[test]
    fn credential_metadata_needs_owner_or_provider() {
        let m = CredentialMetadata::new("h1", "nvidia", CredentialPurpose::ProviderAuth);
        assert_eq!(
            m.validate(),
            Err("either provider_id or connection_id is required")
        );
    }

    #[test]
    fn empty_handle_rejected() {
        let mut m = CredentialMetadata::new("", "nvidia", CredentialPurpose::ProviderAuth);
        m.provider_id = Some("nvidia".into());
        assert_eq!(m.validate(), Err("credential_handle is required"));
    }

    #[test]
    fn credential_metadata_roundtrip() {
        let mut m = CredentialMetadata::new("h1", "nvidia", CredentialPurpose::ProviderAuth);
        m.provider_id = Some("nvidia".into());
        m.expires_at = Some(1800000000000);
        let json = serde_json::to_string(&m).unwrap();
        let decoded: CredentialMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(m, decoded);
    }
}
