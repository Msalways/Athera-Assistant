use crate::requirements::{
    CostTier, DataEgressClass, PrivacyClass, RoutingProvenance, RoutingStrategy, TaskRequirements,
};

#[derive(Debug, Clone, PartialEq)]
pub struct RouteInput<'a> {
    pub requirements: &'a TaskRequirements,
    pub needle_ready: bool,
    pub cloud_ready: bool,
    pub cloud_provider_id: Option<&'a str>,
    pub user_choice: Option<RoutingStrategy>,
    pub now_millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteDecision {
    pub strategy: RoutingStrategy,
    pub provider_id: Option<String>,
    pub reason: String,
    pub egress: DataEgressClass,
}

pub fn route(input: &RouteInput) -> RouteDecision {
    let requirements = input.requirements;
    if let Some(choice) = input.user_choice {
        return explicit_choice(choice, input.cloud_provider_id);
    }
    if matches!(
        requirements.privacy_class,
        PrivacyClass::LocalOnly | PrivacyClass::StrictlyLocal
    ) {
        return RouteDecision {
            strategy: RoutingStrategy::DirectRust,
            provider_id: None,
            reason: "Privacy class keeps data on device".into(),
            egress: DataEgressClass::None,
        };
    }
    if !requirements.requires_tool_use && !requirements.requires_planning {
        if input.needle_ready {
            return RouteDecision {
                strategy: RoutingStrategy::Needle,
                provider_id: None,
                reason: "No tools or planning needed; Needle handles it".into(),
                egress: DataEgressClass::None,
            };
        }
        if input.cloud_ready {
            return cloud_decision(requirements, input.cloud_provider_id, "Needle unavailable");
        }
    }
    if input.cloud_ready {
        let reason = if requirements.requires_planning {
            "Planning required"
        } else {
            "Tool use required"
        };
        return cloud_decision(requirements, input.cloud_provider_id, reason);
    }
    if input.needle_ready {
        return RouteDecision {
            strategy: RoutingStrategy::Needle,
            provider_id: None,
            reason: "Cloud unavailable; best-effort local handling".into(),
            egress: DataEgressClass::None,
        };
    }
    RouteDecision {
        strategy: RoutingStrategy::OptionalOffline,
        provider_id: None,
        reason: "No provider available; deferred until online".into(),
        egress: DataEgressClass::None,
    }
}

fn explicit_choice(choice: RoutingStrategy, cloud_provider_id: Option<&str>) -> RouteDecision {
    match choice {
        RoutingStrategy::CloudResponder | RoutingStrategy::CloudPlanner => RouteDecision {
            strategy: choice,
            provider_id: cloud_provider_id.map(str::to_owned),
            reason: "Explicit user choice".into(),
            egress: DataEgressClass::ModelContext,
        },
        RoutingStrategy::DirectRust => RouteDecision {
            strategy: choice,
            provider_id: None,
            reason: "Explicit user choice".into(),
            egress: DataEgressClass::None,
        },
        RoutingStrategy::Needle => RouteDecision {
            strategy: choice,
            provider_id: None,
            reason: "Explicit user choice".into(),
            egress: DataEgressClass::None,
        },
        RoutingStrategy::OptionalOffline => RouteDecision {
            strategy: choice,
            provider_id: None,
            reason: "Explicit user choice".into(),
            egress: DataEgressClass::None,
        },
    }
}

fn cloud_decision(
    requirements: &TaskRequirements,
    cloud_provider_id: Option<&str>,
    reason: &str,
) -> RouteDecision {
    let strategy = if requirements.requires_planning {
        RoutingStrategy::CloudPlanner
    } else {
        RoutingStrategy::CloudResponder
    };
    let egress = if requirements.max_cost_tier == CostTier::Free {
        DataEgressClass::Anonymized
    } else {
        DataEgressClass::ModelContext
    };
    RouteDecision {
        strategy,
        provider_id: cloud_provider_id.map(str::to_owned),
        reason: reason.into(),
        egress,
    }
}

pub fn provenance_for(
    task_summary: &str,
    decision: &RouteDecision,
    now_millis: u64,
) -> RoutingProvenance {
    RoutingProvenance {
        strategy: decision.strategy,
        provider_id: decision.provider_id.clone(),
        reason: format!("{}: {}", task_summary, decision.reason),
        data_egress_class: decision.egress,
        latency_ms: None,
        routed_at: now_millis,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(requirements: &TaskRequirements) -> RouteInput<'_> {
        RouteInput {
            requirements,
            needle_ready: true,
            cloud_ready: true,
            cloud_provider_id: Some("openai"),
            user_choice: None,
            now_millis: 1000,
        }
    }

    #[test]
    fn greeting_stays_local() {
        let requirements = TaskRequirements::for_greeting();
        let decision = route(&input(&requirements));
        assert_eq!(decision.strategy, RoutingStrategy::DirectRust);
        assert_eq!(decision.egress, DataEgressClass::None);
        assert!(decision.provider_id.is_none());
    }

    #[test]
    fn strictly_local_never_leaves() {
        let mut requirements = TaskRequirements::for_conversation();
        requirements.privacy_class = PrivacyClass::StrictlyLocal;
        let decision = route(&input(&requirements));
        assert_eq!(decision.strategy, RoutingStrategy::DirectRust);
    }

    #[test]
    fn simple_conversation_prefers_needle() {
        let requirements = TaskRequirements::for_conversation();
        let decision = route(&input(&requirements));
        assert_eq!(decision.strategy, RoutingStrategy::Needle);
    }

    #[test]
    fn action_routes_to_cloud_planner() {
        let requirements = TaskRequirements::for_action(vec!["sms.send".into()]);
        let decision = route(&input(&requirements));
        assert_eq!(decision.strategy, RoutingStrategy::CloudPlanner);
        assert_eq!(decision.provider_id.as_deref(), Some("openai"));
        assert_eq!(decision.egress, DataEgressClass::ModelContext);
    }

    #[test]
    fn cloud_down_falls_back_to_needle() {
        let requirements = TaskRequirements::for_action(vec!["sms.send".into()]);
        let mut route_input = input(&requirements);
        route_input.cloud_ready = false;
        let decision = route(&route_input);
        assert_eq!(decision.strategy, RoutingStrategy::Needle);
    }

    #[test]
    fn nothing_ready_defers_offline() {
        let requirements = TaskRequirements::for_conversation();
        let mut route_input = input(&requirements);
        route_input.needle_ready = false;
        route_input.cloud_ready = false;
        let decision = route(&route_input);
        assert_eq!(decision.strategy, RoutingStrategy::OptionalOffline);
    }

    #[test]
    fn explicit_choice_wins() {
        let requirements = TaskRequirements::for_greeting();
        let mut route_input = input(&requirements);
        route_input.user_choice = Some(RoutingStrategy::CloudResponder);
        let decision = route(&route_input);
        assert_eq!(decision.strategy, RoutingStrategy::CloudResponder);
    }

    #[test]
    fn consent_required_avoids_auto_cloud() {
        let mut requirements = TaskRequirements::for_conversation();
        requirements.privacy_class = PrivacyClass::RequiresConsent;
        let decision = route(&input(&requirements));
        assert_eq!(decision.strategy, RoutingStrategy::Needle);
    }

    #[test]
    fn provenance_carries_decision() {
        let requirements = TaskRequirements::for_action(vec!["sms.send".into()]);
        let decision = route(&input(&requirements));
        let provenance = provenance_for("send sms", &decision, 2000);
        assert_eq!(provenance.strategy, RoutingStrategy::CloudPlanner);
        assert_eq!(provenance.routed_at, 2000);
        assert!(provenance.reason.contains("send sms"));
    }
}
