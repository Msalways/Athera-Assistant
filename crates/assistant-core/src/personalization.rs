//! Shared, bounded personalization used by conversation and action execution.
use assistant_contracts::*;
use std::sync::Arc;

pub type PersonalDataGuard = Arc<dyn Fn(&serde_json::Value) -> Result<()> + Send + Sync>;

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub struct PersonalizationService<'a> {
    store: &'a dyn Store,
}

impl<'a> PersonalizationService<'a> {
    pub fn new(store: &'a dyn Store) -> Self {
        Self { store }
    }

    /// Scope is resolved before limiting; workflow IDs must come from the caller's
    /// workflow state, never from substring matching against arbitrary user text.
    pub fn context(
        &self,
        conversation: Id,
        workflow: Option<&str>,
        query: &str,
        temporary: bool,
    ) -> Result<PersonalContext> {
        if temporary {
            return Ok(PersonalContext::default());
        }
        let mut rules = Vec::new();
        let mut bytes = 0;
        let mut keys = std::collections::HashSet::new();
        for rule in self.store.personal_rules(conversation, workflow, now())? {
            if rule.validate().is_err() || self.store.generated_skill(rule.id)?.is_some() {
                continue;
            }
            if rule
                .preference_key
                .as_ref()
                .is_some_and(|key| keys.contains(key))
            {
                continue;
            }
            let size = serde_json::to_vec(&rule)
                .map_err(|_| Error::InvalidInput)?
                .len();
            if bytes + size > 6_000 {
                continue;
            }
            bytes += size;
            if let Some(key) = &rule.preference_key {
                keys.insert(key.clone());
            }
            rules.push(rule);
            if rules.len() == 8 {
                break;
            }
        }
        let words: Vec<_> = query
            .split_whitespace()
            .filter(|w| w.len() > 2)
            .map(str::to_lowercase)
            .collect();
        let memories = self
            .store
            .personal_memories()?
            .into_iter()
            .filter(|m| words.iter().any(|w| m.text.to_lowercase().contains(w)))
            .take(4)
            .collect();
        Ok(PersonalContext { rules, memories })
    }

    pub fn proposal(
        &self,
        instruction: String,
        rationale: String,
        scope: RuleScope,
        source: RuleSource,
        evidence_ids: Vec<Id>,
    ) -> RuleProposal {
        let now = now();
        RuleProposal {
            schema: RULE_PROPOSAL_SCHEMA_V1.into(),
            id: Id::new_v4(),
            rationale,
            proposed_at: now,
            rule: AdaptiveRule {
                schema: ADAPTIVE_RULE_SCHEMA_V1.into(),
                id: Id::new_v4(),
                version: 1,
                scope,
                status: AdaptiveRuleStatus::Proposed,
                source,
                priority: 50,
                instruction,
                preference_key: None,
                evidence_ids,
                created_at: now,
                updated_at: now,
                expires_at: None,
                supersedes: None,
            },
        }
    }

    /// All callers get the same evidence checks and application-owned identity.
    pub fn propose_model(
        &self,
        draft: RuleProposal,
        observation: Option<Id>,
        guard: &PersonalDataGuard,
    ) -> Result<RuleProposal> {
        guard(&serde_json::to_value(&draft).map_err(|_| Error::InvalidInput)?)?;
        let mut proposal = self.proposal(
            draft.rule.instruction,
            draft.rationale,
            draft.rule.scope,
            RuleSource::Model,
            draft.rule.evidence_ids,
        );
        proposal.rule.expires_at = draft.rule.expires_at;
        proposal.rule.preference_key = draft.rule.preference_key;
        proposal.rule.supersedes = draft.rule.supersedes;
        proposal.validate()?;
        self.store
            .propose_personal_rule(&proposal, observation, None)
    }

    pub fn skill(&self, id: &str) -> Result<SkillSpec> {
        let rule_id = id
            .strip_prefix("personal:")
            .and_then(|id| Id::parse_str(id).ok())
            .ok_or(Error::InvalidInput)?;
        let rule = self.store.adaptive_rule(rule_id)?;
        if rule.status != AdaptiveRuleStatus::Enabled
            || rule.expires_at.is_some_and(|expiry| expiry <= now())
        {
            return Err(Error::Denied);
        }
        let skill = self
            .store
            .generated_skill(rule_id)?
            .ok_or(Error::Unavailable)?;
        if !skill.evaluation.passed {
            return Err(Error::Denied);
        }
        for tool in &skill.evaluation.tool_bindings {
            if self.store.capability(&tool.id).ok() != Some(Capability::Tool(tool.clone()))
                || !tool.enabled
            {
                // A changed dependency invalidates the evaluated invariant.
                self.store.decide_personal_rule(
                    rule_id,
                    rule.version,
                    RuleDecision::Disable,
                    now(),
                )?;
                return Err(Error::Conflict);
            }
        }
        let mut spec = skill.spec;
        spec.enabled = true;
        spec.version = rule.version.to_string();
        Ok(spec)
    }
    pub fn propose_skill(
        &self,
        mut spec: SkillSpec,
        rationale: String,
        evidence_id: Id,
        guard: &PersonalDataGuard,
    ) -> Result<RuleProposal> {
        guard(&serde_json::json!({"skill":spec,"rationale":rationale}))?;
        if spec.name.trim().is_empty()
            || spec.name.len() > 200
            || spec.description.len() > 2000
            || spec.instructions.trim().is_empty()
            || spec.instructions.len() > 2000
            || spec.tool_requirements.is_empty()
            || spec.tool_requirements.len() > 8
        {
            return Err(Error::InvalidInput);
        }
        let evidence = self.store.observation(evidence_id)?;
        let proposal = self.proposal(
            spec.instructions.clone(),
            rationale,
            RuleScope::Conversation(evidence.conversation_id),
            RuleSource::Model,
            vec![evidence_id],
        );
        spec.id = format!("personal:{}", proposal.rule.id);
        spec.version = "1".into();
        spec.enabled = false;
        let generated = GeneratedSkill {
            rule_id: proposal.rule.id,
            spec,
            evaluation: SkillEvaluation {
                passed: false,
                cases: 0,
                baseline_matches: 0,
                candidate_matches: 0,
                failures: vec!["Replay evaluation required".into()],
                tool_bindings: vec![],
                records: vec![],
            },
        };
        self.store
            .propose_personal_rule(&proposal, Some(evidence_id), Some(&generated))
    }
}
