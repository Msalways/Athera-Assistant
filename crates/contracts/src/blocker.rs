//! Typed task blocker contracts.
//! Each blocker describes exactly why a task is paused and how to recover.
use serde::{Deserialize, Serialize};

/// Schema version for task blockers.
pub const TASK_BLOCKER_SCHEMA_V1: &str = "aethra.task-blocker.v1";

/// A typed reason why task progress is paused.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskBlocker {
    ProviderCredentialRequired {
        provider_id: String,
    },
    ConnectorAuthorizationRequired {
        connection_id: String,
        scopes: Vec<String>,
    },
    AndroidPermissionRequired {
        permission: String,
    },
    ApprovalRequired {
        proposal_id: String,
        exact_action: String,
    },
    ClarificationRequired {
        question: String,
        candidates: Vec<String>,
    },
    DeviceConstraint {
        network: Option<bool>,
        battery: Option<bool>,
        foreground: Option<bool>,
        storage: Option<bool>,
    },
    CapabilityUnavailable {
        capability_id: String,
    },
    /// The turn needed a reasoning model and none was reachable.
    ///
    /// Distinct from a provider fault on purpose: a missing key or a bad model is
    /// the user's setup to fix, while an unreachable provider will fail the same
    /// way on retry, so telling the user to go and check their settings would send
    /// them somewhere that cannot help.
    ReasoningUnavailable {
        detail: String,
    },
    /// The turn was kept on the device and no on-device model could answer it.
    /// Not an error: the request was honoured by declining to send it.
    HeldOnDevice {
        detail: String,
    },
}

impl TaskBlocker {
    pub fn recovery_action(&self) -> &'static str {
        match self {
            Self::ProviderCredentialRequired { .. } => {
                "Configure the provider API key in Settings."
            }
            Self::ConnectorAuthorizationRequired { .. } => "Connect the service to grant access.",
            Self::AndroidPermissionRequired { .. } => {
                "Grant the required permission in Android settings."
            }
            Self::ApprovalRequired { .. } => "Review and approve the proposed action.",
            Self::ClarificationRequired { .. } => "Answer the question to continue.",
            Self::DeviceConstraint { .. } => "Wait for the device constraint to resolve.",
            Self::CapabilityUnavailable { .. } => {
                "Connect a service that provides this capability."
            }
            Self::ReasoningUnavailable { .. } => {
                "Reconnect or configure a model provider, then run this again."
            }
            Self::HeldOnDevice { .. } => {
                "This was kept on your device and could not be answered here."
            }
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::ProviderCredentialRequired { provider_id } => {
                if provider_id.is_empty() {
                    return Err("provider_id is required");
                }
            }
            Self::ConnectorAuthorizationRequired { connection_id, .. } => {
                if connection_id.is_empty() {
                    return Err("connection_id is required");
                }
            }
            Self::AndroidPermissionRequired { permission } => {
                if permission.is_empty() {
                    return Err("permission is required");
                }
            }
            Self::ApprovalRequired {
                proposal_id,
                exact_action,
            } => {
                if proposal_id.is_empty() {
                    return Err("proposal_id is required");
                }
                if exact_action.is_empty() {
                    return Err("exact_action is required");
                }
            }
            Self::ClarificationRequired { question, .. } => {
                if question.is_empty() {
                    return Err("question is required");
                }
            }
            Self::DeviceConstraint { .. } => {}
            Self::CapabilityUnavailable { capability_id } => {
                if capability_id.is_empty() {
                    return Err("capability_id is required");
                }
            }
            Self::ReasoningUnavailable { detail } => {
                if detail.is_empty() {
                    return Err("detail is required");
                }
            }
            Self::HeldOnDevice { detail } => {
                if detail.is_empty() {
                    return Err("detail is required");
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_credential_blocker_roundtrip() {
        let b = TaskBlocker::ProviderCredentialRequired {
            provider_id: "nvidia-nim".into(),
        };
        b.validate().unwrap();
        let json = serde_json::to_string(&b).unwrap();
        let decoded: TaskBlocker = serde_json::from_str(&json).unwrap();
        assert_eq!(b, decoded);
    }

    #[test]
    fn approval_blocker_has_exact_action() {
        let b = TaskBlocker::ApprovalRequired {
            proposal_id: "abc".into(),
            exact_action: "Send SMS to +1234".into(),
        };
        b.validate().unwrap();
        assert_eq!(
            b.recovery_action(),
            "Review and approve the proposed action."
        );
    }

    #[test]
    fn clarification_blocker_cannot_have_empty_question() {
        let b = TaskBlocker::ClarificationRequired {
            question: "".into(),
            candidates: vec![],
        };
        assert_eq!(b.validate(), Err("question is required"));
    }

    #[test]
    fn device_constraint_blocker_always_valid() {
        let b = TaskBlocker::DeviceConstraint {
            network: Some(false),
            battery: None,
            foreground: None,
            storage: None,
        };
        b.validate().unwrap();
    }
}
