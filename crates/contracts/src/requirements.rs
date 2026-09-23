//! Task requirements and routing provenance types.
//! These describe what a task needs and how the router chose a strategy.
use serde::{Deserialize, Serialize};

/// Schema version for task requirements.
pub const TASK_REQUIREMENTS_SCHEMA_V1: &str = "aethra.task-requirements.v1";

/// Deterministic description of what a task requires.
/// The router uses this to choose an execution strategy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskRequirements {
    pub schema: String,
    pub privacy_class: PrivacyClass,
    pub capability_needs: Vec<String>,
    pub latency_budget_ms: Option<u64>,
    pub max_cost_tier: CostTier,
    pub requires_planning: bool,
    pub requires_tool_use: bool,
    pub requires_external_data: bool,
}

impl TaskRequirements {
    pub fn for_greeting() -> Self {
        Self {
            schema: TASK_REQUIREMENTS_SCHEMA_V1.into(),
            privacy_class: PrivacyClass::LocalOnly,
            capability_needs: vec![],
            latency_budget_ms: Some(3000),
            max_cost_tier: CostTier::Free,
            requires_planning: false,
            requires_tool_use: false,
            requires_external_data: false,
        }
    }

    pub fn for_conversation() -> Self {
        Self {
            schema: TASK_REQUIREMENTS_SCHEMA_V1.into(),
            privacy_class: PrivacyClass::MayLeaveDevice,
            capability_needs: vec![],
            latency_budget_ms: Some(10000),
            max_cost_tier: CostTier::Moderate,
            requires_planning: false,
            requires_tool_use: false,
            requires_external_data: false,
        }
    }

    pub fn for_action(capabilities: Vec<String>) -> Self {
        Self {
            schema: TASK_REQUIREMENTS_SCHEMA_V1.into(),
            privacy_class: PrivacyClass::MayLeaveDevice,
            capability_needs: capabilities,
            latency_budget_ms: Some(30000),
            max_cost_tier: CostTier::Moderate,
            requires_planning: true,
            requires_tool_use: true,
            requires_external_data: false,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != TASK_REQUIREMENTS_SCHEMA_V1 {
            return Err("unsupported task requirements schema");
        }
        Ok(())
    }
}

/// How sensitive the data in this task is.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyClass {
    /// Never leaves the device.
    LocalOnly,
    /// May be sent to a cloud provider.
    MayLeaveDevice,
    /// Requires explicit user consent for cloud handoff.
    RequiresConsent,
    /// Never leaves the device under any circumstances.
    StrictlyLocal,
}

/// Cost tier for provider selection.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CostTier {
    Free,
    Low,
    Moderate,
    High,
}

/// Where a task was routed and why.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutingProvenance {
    pub strategy: RoutingStrategy,
    pub provider_id: Option<String>,
    pub reason: String,
    pub data_egress_class: DataEgressClass,
    pub latency_ms: Option<u64>,
    pub routed_at: u64,
}

/// The execution strategy chosen by the router.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoutingStrategy {
    DirectRust,
    Needle,
    CloudResponder,
    CloudPlanner,
    OptionalOffline,
}

/// What data left (or may leave) the device.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DataEgressClass {
    None,
    Anonymized,
    ModelContext,
    FullContext,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_requirements_roundtrip() {
        let r = TaskRequirements::for_greeting();
        r.validate().unwrap();
        let json = serde_json::to_string(&r).unwrap();
        let decoded: TaskRequirements = serde_json::from_str(&json).unwrap();
        assert_eq!(r, decoded);
    }

    #[test]
    fn routing_provenance_roundtrip() {
        let p = RoutingProvenance {
            strategy: RoutingStrategy::CloudResponder,
            provider_id: Some("nvidia-nim".into()),
            reason: "Complex language task".into(),
            data_egress_class: DataEgressClass::ModelContext,
            latency_ms: Some(1200),
            routed_at: 1700000000000,
        };
        let json = serde_json::to_string(&p).unwrap();
        let decoded: RoutingProvenance = serde_json::from_str(&json).unwrap();
        assert_eq!(p, decoded);
    }

    #[test]
    fn action_requirements_declare_capabilities() {
        let r = TaskRequirements::for_action(vec!["contacts.search".into(), "sms.send".into()]);
        assert!(r.requires_tool_use);
        assert!(r.requires_planning);
        assert_eq!(r.capability_needs.len(), 2);
    }
}
