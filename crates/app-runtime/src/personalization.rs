//! Thin user command boundary for the shared personalization service.
use super::*;
use assistant_core::personalization::{now, PersonalizationService};
use serde_json::{json, Value};

fn version(payload: &Value) -> Result<u32> {
    payload["expected_version"]
        .as_u64()
        .and_then(|v| v.try_into().ok())
        .ok_or(Error::InvalidInput)
}
fn confirmed(payload: &Value) -> Result<()> {
    if payload["confirmed"] != true {
        return Err(Error::Denied);
    }
    Ok(())
}
fn value<T: Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value).map_err(|_| Error::InvalidResponse)
}

impl Runtime {
    pub(crate) async fn personal_command(&self, name: &str, payload: Value) -> Result<Value> {
        let service = PersonalizationService::new(self.store.as_ref());
        let id = |key| companion_commands::id(&payload, key);
        match name {
            "list_adaptive_rules" => value(self.store.adaptive_rules(None, 100)?),
            "list_rule_proposals" => value(self.store.rule_proposals(None, 100)?),
            "rule_history" => value(self.store.rule_history(id("rule_id")?)?),
            "personal_usage" => value(self.store.personal_usage(id("run_id")?)?),
            "personal_rule_details" => {
                let rule = self.store.adaptive_rule(id("rule_id")?)?;
                let evidence = rule
                    .evidence_ids
                    .iter()
                    .map(|id| self.store.observation(*id))
                    .collect::<Result<Vec<_>>>()?;
                Ok(
                    json!({"rule":rule,"evidence":evidence,"feedback":self.store.rule_feedback(rule.id)?,"skill":self.store.generated_skill(rule.id)?}),
                )
            }
            "learning_settings" => {
                if payload.get("enabled").is_some() {
                    confirmed(&payload)?;
                    if !payload["enabled"].is_boolean() {
                        return Err(Error::InvalidInput);
                    }
                    self.store
                        .set_setting("learning_enabled", &payload["enabled"])?;
                    if payload["enabled"] == true {
                        kick_learning(self.assistant.read().await.clone());
                    }
                }
                Ok(
                    json!({"enabled":self.store.setting("learning_enabled")?.unwrap_or(json!(false)),"pending":self.store.pending_observations(100)?.len(),"last_error":self.store.setting("learning_last_error")?}),
                )
            }
            "propose_rule" | "remember_preference" => {
                let instruction = bounded_rule_text(&payload, "instruction", 2000)?;
                let rationale = payload["rationale"]
                    .as_str()
                    .unwrap_or("Explicit user preference")
                    .to_owned();
                if payload["source"].as_str().is_some_and(|s| s != "user") {
                    return Err(Error::Denied);
                }
                let scope = parse_rule_scope(payload["scope"].as_str().unwrap_or("global"))?;
                let evidence_ids = serde_json::from_value(
                    payload.get("evidence_ids").cloned().unwrap_or(json!([])),
                )
                .map_err(|_| Error::InvalidInput)?;
                let mut proposal = service.proposal(
                    instruction,
                    rationale,
                    scope,
                    RuleSource::User,
                    evidence_ids,
                );
                if let Some(key) = payload.get("preference_key") {
                    proposal.rule.preference_key =
                        serde_json::from_value(key.clone()).map_err(|_| Error::InvalidInput)?;
                }
                if let Some(priority) = payload.get("priority") {
                    proposal.rule.priority = serde_json::from_value(priority.clone())
                        .map_err(|_| Error::InvalidInput)?;
                }
                if let Some(expiry) = payload.get("expires_at") {
                    proposal.rule.expires_at =
                        serde_json::from_value(expiry.clone()).map_err(|_| Error::InvalidInput)?;
                }
                if let Some(previous) = payload.get("supersedes") {
                    proposal.rule.supersedes = serde_json::from_value(previous.clone())
                        .map_err(|_| Error::InvalidInput)?;
                }
                if name == "remember_preference" {
                    confirmed(&payload)?;
                }
                let mut saved = self.store.propose_personal_rule(&proposal, None, None)?;
                if name == "remember_preference" {
                    saved.rule = self.store.decide_personal_rule(
                        saved.rule.id,
                        1,
                        RuleDecision::Activate,
                        now(),
                    )?;
                }
                value(saved)
            }
            "review_rule_proposal" => {
                confirmed(&payload)?;
                let proposal = self.store.rule_proposal(id("proposal_id")?)?;
                let decision = match payload["approved"].as_bool().ok_or(Error::InvalidInput)? {
                    true => RuleDecision::Activate,
                    false => RuleDecision::Reject,
                };
                self.store.decide_personal_rule(
                    proposal.rule.id,
                    version(&payload)?,
                    decision,
                    now(),
                )?;
                value(self.store.rule_proposal(proposal.id)?)
            }
            "disable_adaptive_rule" | "rollback_rule" => {
                confirmed(&payload)?;
                let decision = if name == "rollback_rule" {
                    RuleDecision::Rollback {
                        version: payload["version"]
                            .as_u64()
                            .and_then(|v| v.try_into().ok())
                            .ok_or(Error::InvalidInput)?,
                    }
                } else {
                    RuleDecision::Disable
                };
                value(self.store.decide_personal_rule(
                    id("rule_id")?,
                    version(&payload)?,
                    decision,
                    now(),
                )?)
            }
            "record_observation" => {
                let kind: ObservationKind = serde_json::from_value(payload["kind"].clone())
                    .map_err(|_| Error::InvalidInput)?;
                if kind == ObservationKind::Outcome {
                    return Err(Error::Denied);
                }
                if kind == ObservationKind::Remember {
                    confirmed(&payload)?;
                }
                let evidence = self.store.record_observation(&Observation {
                    id: Id::new_v4(),
                    conversation_id: id("conversation_id")?,
                    source_id: id("source_id")?,
                    kind,
                    text: bounded_rule_text(&payload, "text", 2000)?,
                    created_at: now(),
                })?;
                if kind == ObservationKind::Remember {
                    confirmed(&payload)?;
                    let proposal = service.proposal(
                        evidence.text.clone(),
                        "Explicit remember request".into(),
                        RuleScope::Conversation(evidence.conversation_id),
                        RuleSource::User,
                        vec![evidence.id],
                    );
                    let saved =
                        self.store
                            .propose_personal_rule(&proposal, Some(evidence.id), None)?;
                    if saved.rule.status == AdaptiveRuleStatus::Proposed {
                        self.store.decide_personal_rule(
                            saved.rule.id,
                            saved.rule.version,
                            RuleDecision::Activate,
                            now(),
                        )?;
                    }
                } else if self.store.setting("learning_enabled")? == Some(json!(true)) {
                    let assistant = self.assistant.read().await.clone();
                    kick_learning(assistant);
                }
                value(evidence)
            }
            "process_learning" => {
                confirmed(&payload)?;
                value(self.assistant.read().await.clone().learn_pending().await?)
            }
            "propose_skill" => {
                // Manual/imported declarative skills still require evaluation and review.
                let mut spec: SkillSpec = serde_json::from_value(payload["skill"].clone())
                    .map_err(|_| Error::InvalidInput)?;
                validate_skill(&spec)?;
                let evidence = self.store.observation(id("observation_id")?)?;
                let proposal = service.proposal(
                    spec.instructions.clone(),
                    bounded_rule_text(&payload, "rationale", 2000)?,
                    RuleScope::Conversation(evidence.conversation_id),
                    RuleSource::User,
                    vec![evidence.id],
                );
                spec.id = format!("personal:{}", proposal.rule.id);
                spec.version = "1".into();
                spec.enabled = false;
                let skill = GeneratedSkill {
                    rule_id: proposal.rule.id,
                    spec,
                    evaluation: SkillEvaluation {
                        passed: false,
                        cases: 0,
                        baseline_matches: 0,
                        candidate_matches: 0,
                        failures: vec!["Evaluation required".into()],
                        tool_bindings: vec![],
                        records: vec![],
                    },
                };
                value(
                    self.store
                        .propose_personal_rule(&proposal, None, Some(&skill))?,
                )
            }
            "evaluate_skill" => {
                confirmed(&payload)?;
                let cases: Vec<SkillCase> = serde_json::from_value(payload["cases"].clone())
                    .map_err(|_| Error::InvalidInput)?;
                value(
                    self.assistant
                        .read()
                        .await
                        .clone()
                        .evaluate_generated_skill(id("rule_id")?, version(&payload)?, &cases)
                        .await?,
                )
            }
            _ => Err(Error::InvalidInput),
        }
    }
}

pub(crate) fn kick_learning(assistant: Arc<Assistant>) {
    if assistant.store.setting("learning_enabled").ok().flatten() != Some(json!(true)) {
        return;
    }
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let result = assistant.learn_pending().await;
            let error = result.err().map(|e| e.to_string());
            let _ = assistant
                .store
                .set_setting("learning_last_error", &json!(error));
        });
    }
}

fn validate_skill(skill: &SkillSpec) -> Result<()> {
    if skill.name.trim().is_empty()
        || skill.name.len() > 200
        || skill.description.len() > 2000
        || skill.instructions.trim().is_empty()
        || skill.instructions.len() > 2000
        || skill.tool_requirements.is_empty()
        || skill.tool_requirements.len() > 8
    {
        return Err(Error::InvalidInput);
    }
    Ok(())
}
