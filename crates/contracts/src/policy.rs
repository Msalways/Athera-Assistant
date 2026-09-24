use crate::requirements::CostTier;
use serde::{Deserialize, Serialize};

pub const AUTONOMY_ENVELOPE_SCHEMA_V1: &str = "aethra.autonomy-envelope.v1";

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("Envelope objective is required")]
    MissingObjective,
    #[error("Tool is outside the autonomy envelope: {0}")]
    ToolNotAllowed(String),
    #[error("Action budget exhausted")]
    BudgetExhausted,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    Auto,
    ConfirmExternalWrites,
    ConfirmAll,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutonomyEnvelope {
    pub schema: String,
    pub objective: String,
    pub success_condition: String,
    pub allowed_tools: Vec<String>,
    pub allowed_data_scopes: Vec<String>,
    pub max_actions: u32,
    pub max_cost_tier: CostTier,
    pub approval: ApprovalMode,
    pub stop_conditions: Vec<String>,
}

impl AutonomyEnvelope {
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.schema != AUTONOMY_ENVELOPE_SCHEMA_V1 {
            return Err(PolicyError::MissingObjective);
        }
        if self.objective.trim().is_empty() {
            return Err(PolicyError::MissingObjective);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    RequireApproval { summary: String },
    Deny { reason: String },
}

pub fn decide(
    envelope: &AutonomyEnvelope,
    tool_id: &str,
    risk: crate::Risk,
    actions_taken: u32,
    approved: bool,
) -> Result<PolicyDecision, PolicyError> {
    envelope.validate()?;
    if !envelope.allowed_tools.iter().any(|id| id == tool_id) {
        return Err(PolicyError::ToolNotAllowed(tool_id.into()));
    }
    if actions_taken >= envelope.max_actions {
        return Err(PolicyError::BudgetExhausted);
    }
    if matches!(risk, crate::Risk::Destructive) && !approved {
        return Ok(PolicyDecision::RequireApproval {
            summary: format!("Destructive tool requires explicit approval: {tool_id}"),
        });
    }
    if risk.requires_approval() {
        match envelope.approval {
            ApprovalMode::Auto => Ok(PolicyDecision::Allow),
            ApprovalMode::ConfirmExternalWrites | ApprovalMode::ConfirmAll => {
                if approved {
                    Ok(PolicyDecision::Allow)
                } else {
                    Ok(PolicyDecision::RequireApproval {
                        summary: format!("Tool requires approval: {tool_id}"),
                    })
                }
            }
        }
    } else {
        Ok(PolicyDecision::Allow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope() -> AutonomyEnvelope {
        AutonomyEnvelope {
            schema: AUTONOMY_ENVELOPE_SCHEMA_V1.into(),
            objective: "message Arun".into(),
            success_condition: "sent".into(),
            allowed_tools: vec!["sms.send".into()],
            allowed_data_scopes: vec!["contacts".into()],
            max_actions: 3,
            max_cost_tier: CostTier::Moderate,
            approval: ApprovalMode::ConfirmExternalWrites,
            stop_conditions: vec!["user cancels".into()],
        }
    }

    #[test]
    fn read_only_allowed_without_approval() {
        assert_eq!(
            decide(&envelope(), "sms.send", crate::Risk::ReadOnly, 0, false),
            Ok(PolicyDecision::Allow)
        );
    }

    #[test]
    fn external_write_needs_approval() {
        let decision = decide(
            &envelope(),
            "sms.send",
            crate::Risk::ExternalWrite,
            0,
            false,
        )
        .unwrap();
        assert!(matches!(decision, PolicyDecision::RequireApproval { .. }));
        assert_eq!(
            decide(&envelope(), "sms.send", crate::Risk::ExternalWrite, 0, true),
            Ok(PolicyDecision::Allow)
        );
    }

    #[test]
    fn destructive_always_needs_explicit_approval() {
        let mut auto = envelope();
        auto.approval = ApprovalMode::Auto;
        let decision = decide(&auto, "sms.send", crate::Risk::Destructive, 0, false).unwrap();
        assert!(matches!(decision, PolicyDecision::RequireApproval { .. }));
    }

    #[test]
    fn unknown_tool_denied() {
        assert_eq!(
            decide(&envelope(), "evil.tool", crate::Risk::ReadOnly, 0, false),
            Err(PolicyError::ToolNotAllowed("evil.tool".into()))
        );
    }

    #[test]
    fn exhausted_budget_denied() {
        assert_eq!(
            decide(&envelope(), "sms.send", crate::Risk::ReadOnly, 3, false),
            Err(PolicyError::BudgetExhausted)
        );
    }

    #[test]
    fn empty_objective_rejected() {
        let mut envelope = envelope();
        envelope.objective.clear();
        assert_eq!(envelope.validate(), Err(PolicyError::MissingObjective));
    }

    #[test]
    fn envelope_roundtrip() {
        let json = serde_json::to_string(&envelope()).unwrap();
        let decoded: AutonomyEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope(), decoded);
    }
}
