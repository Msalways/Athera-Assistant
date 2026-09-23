//! Vendor-neutral credential reference and secret-state types.
//! Secrets never leave the vault. These types describe references and purposes.
use serde::{Deserialize, Serialize};

/// Schema version for credential state.
pub const CREDENTIAL_STATE_SCHEMA_V1: &str = "aethra.credential-state.v1";

/// An opaque reference to a secret stored in the platform vault.
/// The actual secret value never enters React, model context, SQLite, or traces.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialRef {
    pub handle: String,
    pub purpose: CredentialPurpose,
    pub created_at: u64,
    pub expires_at: Option<u64>,
}

impl CredentialRef {
    pub fn new(handle: impl Into<String>, purpose: CredentialPurpose) -> Self {
        Self {
            handle: handle.into(),
            purpose,
            created_at: now_millis(),
            expires_at: None,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at
            .is_some_and(|expires| expires <= now_millis())
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.handle.is_empty() {
            return Err("credential handle is required");
        }
        Ok(())
    }
}

/// What the credential is used for. Different purposes require different
/// vault bindings and never share a handle.
#[derive(Debug, Clone, Copy, Hash, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CredentialPurpose {
    /// Model provider API key or token.
    ProviderAuth,
    /// MCP connector bearer token or API key.
    ConnectorAuth,
    /// OAuth refresh/access tokens for a connector.
    ConnectorOAuth,
    /// Internal service credential (not user-facing).
    InternalService,
}

/// The current state of a credential as known to the runtime.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialState {
    pub schema: String,
    pub credential_ref: CredentialRef,
    pub status: CredentialStatus,
    pub last_tested_at: Option<u64>,
    pub last_test_result: Option<TestResult>,
}

impl CredentialState {
    pub fn new(reference: CredentialRef) -> Self {
        Self {
            schema: CREDENTIAL_STATE_SCHEMA_V1.into(),
            credential_ref: reference,
            status: CredentialStatus::Untested,
            last_tested_at: None,
            last_test_result: None,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != CREDENTIAL_STATE_SCHEMA_V1 {
            return Err("unsupported credential state schema");
        }
        self.credential_ref.validate()
    }
}

/// Whether the credential has been tested and what happened.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CredentialStatus {
    Untested,
    Testing,
    Valid,
    Invalid,
    Expired,
    Revoked,
    NetworkUnavailable,
    ProviderError,
}

/// The result of a connection test.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestResult {
    pub success: bool,
    pub message: String,
    pub model_id: Option<String>,
    pub latency_ms: Option<u64>,
    pub tested_at: u64,
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_ref_roundtrip() {
        let r = CredentialRef::new("my-api-key", CredentialPurpose::ProviderAuth);
        r.validate().unwrap();
        let json = serde_json::to_string(&r).unwrap();
        let decoded: CredentialRef = serde_json::from_str(&json).unwrap();
        assert_eq!(r, decoded);
    }

    #[test]
    fn credential_state_schema_valid() {
        let state = CredentialState::new(CredentialRef::new(
            "handle",
            CredentialPurpose::ConnectorOAuth,
        ));
        state.validate().unwrap();
        assert_eq!(state.status, CredentialStatus::Untested);
    }

    #[test]
    fn expired_credential_detected() {
        let mut r = CredentialRef::new("key", CredentialPurpose::ProviderAuth);
        r.expires_at = Some(1); // epoch start, long expired
        assert!(r.is_expired());
    }

    #[test]
    fn not_expired_without_expiry() {
        let r = CredentialRef::new("key", CredentialPurpose::ProviderAuth);
        assert!(!r.is_expired());
    }

    #[test]
    fn validation_rejects_empty_handle() {
        let r = CredentialRef {
            handle: "".into(),
            purpose: CredentialPurpose::ProviderAuth,
            created_at: 0,
            expires_at: None,
        };
        assert_eq!(r.validate(), Err("credential handle is required"));
    }
}
