//! Application-owned personalization records. Timestamps use Unix milliseconds.
use crate::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersonalContext {
    pub rules: Vec<AdaptiveRule>,
    pub memories: Vec<conversation::PersonalMemory>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationKind {
    Correction,
    Remember,
    Rejection,
    Outcome,
    PreferenceFollowed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub id: Id,
    pub conversation_id: Id,
    pub source_id: Id,
    pub kind: ObservationKind,
    pub text: String,
    pub created_at: u64,
}

impl Observation {
    pub fn validate(&self) -> Result<()> {
        if self.text.trim().is_empty() || self.text.len() > 8_000 {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleDecision {
    Activate,
    Reject,
    Disable,
    Rollback { version: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleRevision {
    pub rule: AdaptiveRule,
    pub decision: String,
    pub historical_baseline: bool,
}

/// Recorded cases are trusted user/evaluation inputs, never model-authored assertions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillCase {
    pub observation_id: Id,
    pub expected: Vec<ToolCall>,
    pub simulated_results: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEvaluation {
    pub passed: bool,
    pub cases: usize,
    pub baseline_matches: usize,
    pub candidate_matches: usize,
    pub failures: Vec<String>,
    pub tool_bindings: Vec<ToolSpec>,
    #[serde(default)]
    pub records: Vec<SkillReplay>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillReplay {
    pub case: SkillCase,
    pub baseline: Vec<AgentAction>,
    pub candidate: Vec<AgentAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedSkill {
    pub rule_id: Id,
    pub spec: SkillSpec,
    pub evaluation: SkillEvaluation,
}

/// Each observation is consumed atomically with its candidate, or explicitly skipped.
/// Revision decisions use optimistic concurrency and append immutable snapshots.
pub trait PersonalizationStore: Send + Sync {
    fn personal_rules(
        &self,
        conversation: Id,
        workflow: Option<&str>,
        now: u64,
    ) -> Result<Vec<AdaptiveRule>>;
    fn personal_memories(&self) -> Result<Vec<conversation::PersonalMemory>>;
    fn propose_personal_rule(
        &self,
        proposal: &RuleProposal,
        observation: Option<Id>,
        skill: Option<&GeneratedSkill>,
    ) -> Result<RuleProposal>;
    fn decide_personal_rule(
        &self,
        id: Id,
        expected_version: u32,
        decision: RuleDecision,
        now: u64,
    ) -> Result<AdaptiveRule>;
    fn rule_history(&self, id: Id) -> Result<Vec<RuleRevision>>;
    fn record_observation(&self, observation: &Observation) -> Result<Observation>;
    fn observation(&self, id: Id) -> Result<Observation>;
    fn rule_feedback(&self, rule_id: Id) -> Result<Vec<Observation>>;
    fn pending_observations(&self, limit: usize) -> Result<Vec<Observation>>;
    fn skip_observation(&self, id: Id) -> Result<()>;
    fn record_personal_usage(
        &self,
        run_id: Id,
        conversation: Id,
        rules: &[AdaptiveRule],
    ) -> Result<()>;
    fn personal_usage(&self, run_id: Id) -> Result<Vec<RuleRevision>>;
    fn generated_skill(&self, rule_id: Id) -> Result<Option<GeneratedSkill>>;
    fn save_skill_evaluation(
        &self,
        rule_id: Id,
        expected_version: u32,
        evaluation: &SkillEvaluation,
    ) -> Result<()>;
}
