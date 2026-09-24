//! Versioned, user-governed adaptive rules. Model output may propose rules;
//! application code decides when a proposal becomes enabled.
use crate::{Error, Id, Result};
use serde::{Deserialize, Serialize};

pub const ADAPTIVE_RULE_SCHEMA_V1: &str = "aethra.adaptive-rule.v1";
pub const RULE_PROPOSAL_SCHEMA_V1: &str = "aethra.rule-proposal.v1";
const MAX_RULE_TEXT: usize = 2_000;
const MAX_EVIDENCE: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuleScope {
    Global,
    Conversation(Id),
    Workflow(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdaptiveRuleStatus {
    Proposed,
    Enabled,
    Disabled,
    Rejected,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuleSource {
    User,
    Model,
    Imported,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AdaptiveRule {
    pub schema: String,
    pub id: Id,
    pub version: u32,
    pub scope: RuleScope,
    pub status: AdaptiveRuleStatus,
    pub source: RuleSource,
    pub priority: u8,
    pub instruction: String,
    /// Optional stable preference dimension, e.g. response.length or meeting.time.
    #[serde(default)]
    pub preference_key: Option<String>,
    pub evidence_ids: Vec<Id>,
    pub created_at: u64,
    pub updated_at: u64,
    pub expires_at: Option<u64>,
    pub supersedes: Option<Id>,
}

impl AdaptiveRule {
    pub fn validate(&self) -> Result<()> {
        if self.preference_key.as_ref().is_some_and(|key| {
            key.is_empty()
                || key.len() > 128
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
        }) {
            return Err(Error::InvalidInput);
        }
        if self.schema != ADAPTIVE_RULE_SCHEMA_V1
            || self.version == 0
            || self.instruction.trim().is_empty()
            || self.instruction.chars().count() > MAX_RULE_TEXT
            || self.evidence_ids.len() > MAX_EVIDENCE
            || self.created_at > self.updated_at
            || self
                .expires_at
                .is_some_and(|expiry| expiry < self.created_at)
        {
            return Err(Error::InvalidInput);
        }
        if let RuleScope::Workflow(name) = &self.scope {
            if name.trim().is_empty() || name.chars().count() > 128 {
                return Err(Error::InvalidInput);
            }
        }
        if self.evidence_ids.iter().any(|id| {
            self.evidence_ids
                .iter()
                .filter(|other| *other == id)
                .count()
                > 1
        }) {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuleProposal {
    pub schema: String,
    pub id: Id,
    pub rule: AdaptiveRule,
    pub rationale: String,
    pub proposed_at: u64,
}

impl RuleProposal {
    pub fn validate(&self) -> Result<()> {
        if self.schema != RULE_PROPOSAL_SCHEMA_V1
            || self.rationale.trim().is_empty()
            || self.rationale.chars().count() > MAX_RULE_TEXT
        {
            return Err(Error::InvalidInput);
        }
        self.rule.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule() -> AdaptiveRule {
        AdaptiveRule {
            schema: ADAPTIVE_RULE_SCHEMA_V1.into(),
            id: Id::new_v4(),
            version: 1,
            scope: RuleScope::Global,
            status: AdaptiveRuleStatus::Proposed,
            source: RuleSource::Model,
            priority: 10,
            instruction: "Prefer concise answers".into(),
            preference_key: None,
            evidence_ids: vec![Id::new_v4()],
            created_at: 1,
            updated_at: 1,
            expires_at: None,
            supersedes: None,
        }
    }

    #[test]
    fn validates_bounded_rule_and_proposal() {
        let rule = rule();
        assert!(rule.validate().is_ok());
        let proposal = RuleProposal {
            schema: RULE_PROPOSAL_SCHEMA_V1.into(),
            id: Id::new_v4(),
            rule,
            rationale: "Repeated user correction".into(),
            proposed_at: 2,
        };
        assert!(proposal.validate().is_ok());
    }

    #[test]
    fn rejects_expired_or_duplicate_evidence() {
        let mut rule = rule();
        rule.expires_at = Some(0);
        assert_eq!(rule.validate(), Err(Error::InvalidInput));
        rule.expires_at = None;
        rule.evidence_ids.push(rule.evidence_ids[0]);
        assert_eq!(rule.validate(), Err(Error::InvalidInput));
    }
}
