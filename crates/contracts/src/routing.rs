//! Where a turn should be answered, decided in Rust rather than by a model.
//!
//! The reason this lives here and not in a prompt is that it has to keep working
//! when the reasoning tier it would prefer is gone. A model asked to "decide
//! where to answer" cannot answer that question when it is itself the thing that
//! is unavailable, and it cannot be made to keep a secret. Both properties
//! matter: the decision must survive losing the cloud, and the decision to keep
//! something on the device must not be negotiable by the model that handles it.
//!
//! A turn has four possible outcomes, and the fourth is the one that usually goes
//! missing: refusing on the device because the answer must not leave it, even
//! though answering was possible elsewhere.

use serde::{Deserialize, Serialize};

pub const ROUTING_POLICY_SCHEMA_V1: &str = "aethra.routing-policy.v1";

/// The tiers that can produce an answer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningTier {
    /// On this device: routing, extraction, and anything that must not leave.
    LocalDevice,
    /// A remote reasoning model.
    Cloud,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RoutingDecision {
    /// Answer here, now, without sending anything anywhere.
    AnswerLocally { reason: String },
    /// Answer with a remote reasoning model.
    AnswerInCloud { reason: String },
    /// Must not leave this device, and no local model could answer. The request
    /// is honoured by declining rather than by leaking it.
    RefuseOnDevice { reason: String },
    /// Needs a reasoning model and none is reachable. Not a failure to try
    /// again: the same request will fail identically next time.
    RefuseNoReasoning { reason: String },
}

impl RoutingDecision {
    pub fn tier(&self) -> Option<ReasoningTier> {
        match self {
            Self::AnswerLocally { .. } => Some(ReasoningTier::LocalDevice),
            Self::AnswerInCloud { .. } => Some(ReasoningTier::Cloud),
            Self::RefuseOnDevice { .. } | Self::RefuseNoReasoning { .. } => None,
        }
    }

    /// Whether this outcome is an answer rather than a decline.
    pub fn answered(&self) -> bool {
        matches!(
            self,
            Self::AnswerLocally { .. } | Self::AnswerInCloud { .. }
        )
    }

    /// Whether anything in this decision is allowed to leave the device.
    pub fn leaves_device(&self) -> bool {
        matches!(self, Self::AnswerInCloud { .. })
    }

    /// What the user is told, and what they can do about it.
    ///
    /// The reason is included wherever it changes what the user should expect.
    /// "Answered on this device" alone hides the case that matters most: that the
    /// answer was local *because nothing else was reachable*, which is a
    /// materially weaker answer than those words imply.
    pub fn user_message(&self) -> String {
        match self {
            Self::AnswerLocally { reason } => format!("Answered on this device. {reason}"),
            Self::AnswerInCloud { .. } => "Answered by the configured model.".into(),
            Self::RefuseOnDevice { reason } => format!(
                "This stays on your device, and the on-device model could not answer it. {reason}"
            ),
            Self::RefuseNoReasoning { reason } => {
                format!("This needs a reasoning model, and none is available right now. {reason}")
            }
        }
    }
}

/// What the runtime knows when it has to choose.
#[derive(Debug, Clone)]
pub struct RoutingInputs {
    /// A local model that can actually answer, not merely observe.
    pub local_available: bool,
    /// A reachable remote reasoning model.
    pub cloud_available: bool,
    /// Policy says this turn's content may be sent to a remote model.
    pub may_leave_device: bool,
    /// Every capability this turn can reach is read-only.
    pub read_only: bool,
}

/// Choose a tier, in an order chosen so that a preference can never outrank a
/// constraint.
pub fn decide(inputs: &RoutingInputs) -> RoutingDecision {
    if !inputs.may_leave_device {
        return if inputs.local_available {
            RoutingDecision::AnswerLocally {
                reason: "Kept on this device.".into(),
            }
        } else {
            RoutingDecision::RefuseOnDevice {
                reason: "Nothing on this device could answer it.".into(),
            }
        };
    }
    if inputs.cloud_available {
        return RoutingDecision::AnswerInCloud {
            reason: if inputs.local_available {
                "A reasoning model is available and is better suited to this.".into()
            } else {
                "No on-device model could answer it.".into()
            },
        };
    }
    if inputs.local_available {
        return RoutingDecision::AnswerLocally {
            reason: "No reasoning model was reachable, so this stayed on the device.".into(),
        };
    }
    RoutingDecision::RefuseNoReasoning {
        reason: "Connect a provider and try again.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(local: bool, cloud: bool, may_leave: bool, read_only: bool) -> RoutingInputs {
        RoutingInputs {
            local_available: local,
            cloud_available: cloud,
            may_leave_device: may_leave,
            read_only,
        }
    }

    #[test]
    fn content_that_may_not_leave_never_reaches_the_cloud() {
        // Even with a cloud model sitting right there, willing and able.
        for cloud in [true, false] {
            let decision = decide(&inputs(true, cloud, false, true));
            assert_eq!(decision.tier(), Some(ReasoningTier::LocalDevice));
            assert!(!decision.leaves_device());
        }
    }

    #[test]
    fn content_that_may_not_leave_is_refused_rather_than_leaked() {
        // The outcome that usually goes missing: no local model, cloud available,
        // and the content still may not leave. It must decline.
        let decision = decide(&inputs(false, true, false, true));
        assert_eq!(decision.tier(), None);
        assert!(!decision.answered());
        assert!(!decision.leaves_device());
        assert!(matches!(decision, RoutingDecision::RefuseOnDevice { .. }));
    }

    #[test]
    fn a_preference_can_never_outrank_the_constraint() {
        // Local is available and cheap, but the content may not leave, so it wins
        // only because it is the sole option.
        assert!(decide(&inputs(true, false, false, false)).answered());
        // Cloud is better but forbidden.
        assert!(!decide(&inputs(true, true, false, false)).leaves_device());
    }

    #[test]
    fn a_reachable_reasoning_model_is_preferred_when_content_may_leave() {
        let decision = decide(&inputs(true, true, true, true));
        assert_eq!(decision.tier(), Some(ReasoningTier::Cloud));
    }

    #[test]
    fn losing_the_cloud_degrades_to_the_device_rather_than_pretending() {
        let decision = decide(&inputs(true, false, true, true));
        assert_eq!(decision.tier(), Some(ReasoningTier::LocalDevice));
        assert!(decision.user_message().contains("stayed on the device"));
    }

    #[test]
    fn losing_the_cloud_with_nothing_local_is_an_honest_decline() {
        let decision = decide(&inputs(false, false, true, true));
        assert!(!decision.answered());
        assert!(matches!(
            decision,
            RoutingDecision::RefuseNoReasoning { .. }
        ));
        assert!(decision.user_message().contains("none is available"));
    }

    #[test]
    fn read_only_work_is_what_allows_an_automatic_device_answer() {
        // Sensitive work is not routed on its own; read-only work may be.
        assert!(decide(&inputs(true, false, true, true)).answered());
        let sensitive = decide(&inputs(true, false, false, false));
        assert!(!sensitive.leaves_device());
    }
}
