use serde::{Deserialize, Serialize};

pub const IDENTITY_SCHEMA_V1: &str = "aethra.identity.v1";

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum IdentityError {
    #[error("Unsupported identity path")]
    UnsupportedPath,
    #[error("Incomplete identity config: {0}")]
    IncompleteConfig(String),
    #[error("Token expired")]
    Expired,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IdentityKind {
    GoogleOAuth,
    GoogleWorkloadIdentity,
    AzureEntra,
    AzureApiKey,
    AnthropicWorkloadIdentity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IdentityConfig {
    pub schema: String,
    pub kind: IdentityKind,
    pub project: Option<String>,
    pub location: Option<String>,
    pub tenant: Option<String>,
    pub workspace_id: Option<String>,
    pub token_ref: Option<String>,
    pub expires_at: Option<u64>,
}

impl IdentityConfig {
    pub fn validate(&self) -> Result<(), IdentityError> {
        if self.schema != IDENTITY_SCHEMA_V1 {
            return Err(IdentityError::IncompleteConfig("unsupported schema".into()));
        }
        match self.kind {
            IdentityKind::GoogleOAuth => {
                if self.token_ref.is_none() {
                    return Err(IdentityError::IncompleteConfig(
                        "google oauth needs a token ref".into(),
                    ));
                }
            }
            IdentityKind::GoogleWorkloadIdentity => {
                if self.project.is_none() {
                    return Err(IdentityError::IncompleteConfig(
                        "google workload identity needs a project".into(),
                    ));
                }
            }
            IdentityKind::AzureEntra => {
                if self.tenant.is_none() {
                    return Err(IdentityError::IncompleteConfig(
                        "entra needs a tenant".into(),
                    ));
                }
            }
            IdentityKind::AzureApiKey => {
                if self.token_ref.is_none() {
                    return Err(IdentityError::IncompleteConfig(
                        "azure api key needs a token ref".into(),
                    ));
                }
            }
            IdentityKind::AnthropicWorkloadIdentity => {
                if self.workspace_id.is_none() {
                    return Err(IdentityError::IncompleteConfig(
                        "anthropic workload identity needs a workspace".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn is_expired(&self, now_millis: u64) -> bool {
        self.expires_at.is_some_and(|expires| expires <= now_millis)
    }

    pub fn bearer_header(&self, token_value: &str) -> Result<(String, String), IdentityError> {
        match self.kind {
            IdentityKind::GoogleOAuth
            | IdentityKind::AzureEntra
            | IdentityKind::AnthropicWorkloadIdentity => {
                Ok(("authorization".into(), format!("Bearer {token_value}")))
            }
            IdentityKind::GoogleWorkloadIdentity => Err(IdentityError::UnsupportedPath),
            IdentityKind::AzureApiKey => Ok(("api-key".into(), token_value.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(kind: IdentityKind) -> IdentityConfig {
        IdentityConfig {
            schema: IDENTITY_SCHEMA_V1.into(),
            kind,
            project: None,
            location: None,
            tenant: None,
            workspace_id: None,
            token_ref: None,
            expires_at: None,
        }
    }

    #[test]
    fn google_oauth_needs_token_ref() {
        assert!(base(IdentityKind::GoogleOAuth).validate().is_err());
        let mut config = base(IdentityKind::GoogleOAuth);
        config.token_ref = Some("ref-1".into());
        config.validate().unwrap();
    }

    #[test]
    fn google_workload_needs_project() {
        assert!(base(IdentityKind::GoogleWorkloadIdentity)
            .validate()
            .is_err());
        let mut config = base(IdentityKind::GoogleWorkloadIdentity);
        config.project = Some("my-project".into());
        config.validate().unwrap();
    }

    #[test]
    fn entra_needs_tenant() {
        assert!(base(IdentityKind::AzureEntra).validate().is_err());
        let mut config = base(IdentityKind::AzureEntra);
        config.tenant = Some("tenant-1".into());
        config.validate().unwrap();
    }

    #[test]
    fn azure_key_uses_api_key_header() {
        let mut config = base(IdentityKind::AzureApiKey);
        config.token_ref = Some("ref-1".into());
        config.validate().unwrap();
        let (name, value) = config.bearer_header("secret").unwrap();
        assert_eq!(name, "api-key");
        assert_eq!(value, "secret");
    }

    #[test]
    fn anthropic_workload_needs_workspace() {
        assert!(base(IdentityKind::AnthropicWorkloadIdentity)
            .validate()
            .is_err());
        let mut config = base(IdentityKind::AnthropicWorkloadIdentity);
        config.workspace_id = Some("ws-1".into());
        config.validate().unwrap();
        let (name, value) = config.bearer_header("token123").unwrap();
        assert_eq!(name, "authorization");
        assert_eq!(value, "Bearer token123");
    }

    #[test]
    fn workload_without_token_path_unsupported() {
        let mut config = base(IdentityKind::GoogleWorkloadIdentity);
        config.project = Some("p".into());
        assert_eq!(
            config.bearer_header("x"),
            Err(IdentityError::UnsupportedPath)
        );
    }

    #[test]
    fn expiry_detected() {
        let mut config = base(IdentityKind::AzureApiKey);
        config.token_ref = Some("ref".into());
        config.expires_at = Some(1000);
        assert!(config.is_expired(2000));
        assert!(!config.is_expired(500));
    }

    #[test]
    fn identity_roundtrip() {
        let mut config = base(IdentityKind::GoogleOAuth);
        config.token_ref = Some("ref-1".into());
        let json = serde_json::to_string(&config).unwrap();
        let decoded: IdentityConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, decoded);
    }
}
