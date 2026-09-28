//! A recorded observation of what the on-device model would have done.
//!
//! Shadow mode runs the local model alongside the cloud model on every turn and
//! records its proposal without acting on it. The point is to answer one
//! question with real data instead of assumption: does the local model's
//! confidence transfer to *our* tool surface? Until that is measured, a local
//! fallback would be a guess.
//!
//! Deliberately, the user's own words are not stored. What is kept is the local
//! model's proposal, its confidence, and what the cloud model actually did, which
//! is enough to tell whether the local model agreed with the answer the user
//! received, without keeping a second copy of their text.
use crate::{Id, Result};
use serde::{Deserialize, Serialize};

pub const LOCAL_PROBE_SCHEMA_V1: &str = "aethra.local-probe.v1";

/// One shadow observation of the on-device model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalProbe {
    pub schema: String,
    pub id: Id,
    pub task_id: Id,
    pub conversation_id: Id,
    /// The local model's proposed tool, or `None` when it proposed nothing.
    pub proposed_tool: Option<String>,
    /// Arguments the local model filled in, grammar-guaranteed to match schema.
    #[serde(default)]
    pub proposed_arguments: serde_json::Value,
    /// Calibrated confidence, when the loaded weights carry a calibration head.
    /// `None` is meaningful: it means the score cannot be trusted at all.
    pub confidence: Option<f64>,
    pub latency_ms: u64,
    /// What the cloud model actually did, when it used a tool. Compared against
    /// the local proposal to see whether the two agreed.
    pub answered_with_tool: Option<String>,
    pub created_at: u64,
}

impl LocalProbe {
    pub fn new(task_id: Id, conversation_id: Id) -> Self {
        Self {
            schema: LOCAL_PROBE_SCHEMA_V1.into(),
            id: Id::new_v4(),
            task_id,
            conversation_id,
            proposed_tool: None,
            proposed_arguments: serde_json::Value::Null,
            confidence: None,
            latency_ms: 0,
            answered_with_tool: None,
            created_at: 0,
        }
    }

    /// Whether the local model chose the same capability the cloud model used.
    ///
    /// `None` when the turn cannot be judged: if the cloud used no tool there is
    /// nothing to agree with, and that case must not be counted as agreement.
    pub fn agreed(&self) -> Option<bool> {
        let cloud = self.answered_with_tool.as_deref()?;
        Some(self.proposed_tool.as_deref() == Some(cloud))
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema != LOCAL_PROBE_SCHEMA_V1 || self.task_id.is_nil() {
            return Err(crate::Error::InvalidInput);
        }
        if let Some(confidence) = self.confidence {
            if !(0.0..=1.0).contains(&confidence) {
                return Err(crate::Error::InvalidInput);
            }
        }
        if let Some(tool) = &self.proposed_tool {
            if tool.is_empty() || tool.len() > 200 {
                return Err(crate::Error::InvalidInput);
            }
        }
        Ok(())
    }
}

/// Aggregate agreement, used to decide whether the local model is trustworthy
/// on this product's tools yet.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LocalProbeStats {
    pub observed: u32,
    /// Turns where the cloud used a tool, so agreement is judgeable.
    pub judgeable: u32,
    pub agreed: u32,
    /// Proposals the local model withheld, i.e. it proposed nothing.
    pub abstained: u32,
    /// Observations with no confidence score, which cannot be gated on.
    pub uncalibrated: u32,
    /// Agreement rate at or above the configured gate.
    pub gated_agreed: u32,
    pub gated_judgeable: u32,
    /// The gate the numbers above were computed against.
    pub threshold: f64,
}

impl LocalProbeStats {
    pub fn agreement_rate(&self) -> Option<f64> {
        (self.judgeable > 0).then(|| self.agreed as f64 / self.judgeable as f64)
    }

    /// Agreement among the proposals that were confident enough to act on.
    pub fn gated_agreement_rate(&self) -> Option<f64> {
        (self.gated_judgeable > 0).then(|| self.gated_agreed as f64 / self.gated_judgeable as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(tool: Option<&str>, confidence: Option<f64>, cloud: Option<&str>) -> LocalProbe {
        let mut p = LocalProbe::new(Id::new_v4(), Id::new_v4());
        p.proposed_tool = tool.map(str::to_owned);
        p.confidence = confidence;
        p.answered_with_tool = cloud.map(str::to_owned);
        p
    }

    #[test]
    fn agreement_is_only_defined_when_the_cloud_used_a_tool() {
        assert_eq!(
            probe(Some("tool_0"), Some(0.9), Some("tool_0")).agreed(),
            Some(true)
        );
        assert_eq!(
            probe(Some("tool_1"), Some(0.9), Some("tool_0")).agreed(),
            Some(false)
        );
        // The cloud answered in prose, so there is nothing to agree with.
        assert_eq!(probe(Some("tool_0"), Some(0.9), None).agreed(), None);
    }

    #[test]
    fn a_missing_confidence_is_recorded_as_uncalibrated_not_as_zero() {
        let p = probe(Some("tool_0"), None, Some("tool_0"));
        assert!(p.agreed().is_some());
        assert_eq!(
            p.confidence, None,
            "a missing score is not the same as no confidence"
        );
    }

    #[test]
    fn an_out_of_range_confidence_is_rejected() {
        let mut p = probe(Some("tool_0"), Some(1.4), Some("tool_0"));
        assert!(p.validate().is_err());
        p.confidence = Some(0.4);
        assert!(p.validate().is_ok());
    }

    #[test]
    fn rates_are_absent_until_there_is_something_to_divide() {
        assert_eq!(LocalProbeStats::default().agreement_rate(), None);
        let stats = LocalProbeStats {
            observed: 3,
            judgeable: 2,
            agreed: 1,
            ..Default::default()
        };
        assert_eq!(stats.agreement_rate(), Some(0.5));
    }
}
