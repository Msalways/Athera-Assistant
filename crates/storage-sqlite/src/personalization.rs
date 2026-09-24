//! Transactional revision history and durable learning queue.
use super::*;
use rusqlite::{OptionalExtension, Transaction};

fn current(tx: &Connection, id: Id) -> Result<AdaptiveRule> {
    decode(
        tx.query_row(
            "SELECT data FROM adaptive_rules WHERE id=?1",
            [id.to_string()],
            |r| r.get(0),
        )
        .map_err(|_| Error::Unavailable)?,
    )
}

pub(crate) fn save(tx: &Transaction<'_>, rule: &AdaptiveRule, decision: &str) -> Result<()> {
    rule.validate()?;
    tx.execute(
        "INSERT INTO personal_revisions(rule_id,version,data,decision) VALUES(?1,?2,?3,?4)",
        params![rule.id.to_string(), rule.version, encode(rule)?, decision],
    )
    .map_err(|_| Error::Conflict)?;
    tx.execute("INSERT INTO adaptive_rules(id,status,scope,updated_at,data) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET status=excluded.status,scope=excluded.scope,updated_at=excluded.updated_at,data=excluded.data", params![rule.id.to_string(), encode(&rule.status)?, encode(&rule.scope)?, timestamp(rule.updated_at)?, encode(rule)?]).map_err(|_| Error::Storage)?;
    // Compatibility view: proposal display always reflects the authoritative state.
    tx.execute("UPDATE rule_proposals SET status=?1,data=json_set(data,'$.rule',json(?2)) WHERE json_extract(data,'$.rule.id')=?3", params![encode(&rule.status)?, encode(rule)?, rule.id.to_string()]).map_err(|_| Error::Storage)?;
    Ok(())
}

fn observation_in(conn: &Connection, id: Id) -> Result<Observation> {
    decode(
        conn.query_row(
            "SELECT data FROM personal_observations WHERE id=?1",
            [id.to_string()],
            |r| r.get(0),
        )
        .map_err(|_| Error::Unavailable)?,
    )
}

fn validate_evidence(conn: &Connection, rule: &AdaptiveRule) -> Result<()> {
    if rule.source == RuleSource::Model && rule.evidence_ids.is_empty() {
        return Err(Error::Denied);
    }
    for id in &rule.evidence_ids {
        let evidence = observation_in(conn, *id)?;
        if let RuleScope::Conversation(conversation) = rule.scope {
            if evidence.conversation_id != conversation {
                return Err(Error::Denied);
            }
        }
    }
    Ok(())
}

impl PersonalizationStore for SqliteStore {
    fn rule_feedback(&self, rule_id: Id) -> Result<Vec<Observation>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt=conn.prepare("SELECT o.data FROM personal_observations o WHERE EXISTS(SELECT 1 FROM personal_usage u WHERE u.run_id=o.source_id AND u.rule_id=?1) ORDER BY o.rowid DESC LIMIT 100").map_err(|_| Error::Storage)?;
        let rows = stmt
            .query_map([rule_id.to_string()], |r| r.get::<_, String>(0))
            .map_err(|_| Error::Storage)?;
        rows.map(|r| decode(r.map_err(|_| Error::Storage)?))
            .collect()
    }
    fn save_skill_evaluation(
        &self,
        rule_id: Id,
        expected_version: u32,
        evaluation: &SkillEvaluation,
    ) -> Result<()> {
        let mut conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let tx = conn.transaction().map_err(|_| Error::Storage)?;
        let rule = current(&tx, rule_id)?;
        if rule.version != expected_version || rule.status != AdaptiveRuleStatus::Proposed {
            return Err(Error::Conflict);
        }
        let mut skill: GeneratedSkill = decode(
            tx.query_row(
                "SELECT data FROM generated_skills WHERE rule_id=?1",
                [rule_id.to_string()],
                |r| r.get(0),
            )
            .map_err(|_| Error::Unavailable)?,
        )?;
        skill.evaluation = evaluation.clone();
        tx.execute(
            "INSERT INTO skill_evaluations(rule_id,version,data) VALUES(?1,?2,?3)",
            params![rule_id.to_string(), expected_version, encode(evaluation)?],
        )
        .map_err(|_| Error::Storage)?;
        tx.execute(
            "UPDATE generated_skills SET data=?1 WHERE rule_id=?2",
            params![encode(&skill)?, rule_id.to_string()],
        )
        .map_err(|_| Error::Storage)?;
        tx.commit().map_err(|_| Error::Storage)
    }
    fn personal_rules(
        &self,
        conversation: Id,
        workflow: Option<&str>,
        now: u64,
    ) -> Result<Vec<AdaptiveRule>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt = conn.prepare("SELECT data FROM adaptive_rules WHERE status=?1 AND (scope=?2 OR scope=?3 OR scope=?4) AND (json_extract(data,'$.expires_at') IS NULL OR json_extract(data,'$.expires_at')>?5) ORDER BY CASE WHEN scope=?3 THEN 2 WHEN scope=?4 THEN 1 ELSE 0 END DESC,json_extract(data,'$.priority') DESC,updated_at DESC,id LIMIT 100").map_err(|_| Error::Storage)?;
        let rows = stmt
            .query_map(
                params![
                    encode(&AdaptiveRuleStatus::Enabled)?,
                    encode(&RuleScope::Global)?,
                    encode(&RuleScope::Conversation(conversation))?,
                    workflow
                        .map(|w| encode(&RuleScope::Workflow(w.to_owned())))
                        .transpose()?,
                    timestamp(now)?
                ],
                |r| r.get::<_, String>(0),
            )
            .map_err(|_| Error::Storage)?;
        rows.map(|r| decode(r.map_err(|_| Error::Storage)?))
            .collect()
    }

    fn personal_memories(&self) -> Result<Vec<assistant_contracts::conversation::PersonalMemory>> {
        self.list_memories(100)
    }

    fn propose_personal_rule(
        &self,
        proposal: &RuleProposal,
        observation: Option<Id>,
        skill: Option<&GeneratedSkill>,
    ) -> Result<RuleProposal> {
        proposal.validate()?;
        if proposal.rule.version != 1 || proposal.rule.status != AdaptiveRuleStatus::Proposed {
            return Err(Error::InvalidInput);
        }
        let mut conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let tx = conn.transaction().map_err(|_| Error::Storage)?;
        if let Some(id) = observation {
            let existing: Option<String> = tx.query_row("SELECT p.data FROM learning_candidates c JOIN rule_proposals p ON p.id=c.proposal_id WHERE c.observation_id=?1", [id.to_string()], |r| r.get(0)).optional().map_err(|_| Error::Storage)?;
            if let Some(existing) = existing {
                return decode(existing);
            }
            let processed: bool = tx
                .query_row(
                    "SELECT processed FROM personal_observations WHERE id=?1",
                    [id.to_string()],
                    |r| r.get(0),
                )
                .map_err(|_| Error::Unavailable)?;
            if processed || !proposal.rule.evidence_ids.contains(&id) {
                return Err(Error::Conflict);
            }
        }
        validate_evidence(&tx, &proposal.rule)?;
        if proposal.rule.source == RuleSource::Model && skill.is_none() {
            let existing: Option<String> = tx.query_row("SELECT p.data FROM rule_proposals p JOIN adaptive_rules a ON a.id=json_extract(p.data,'$.rule.id') WHERE a.scope=?1 AND lower(trim(json_extract(a.data,'$.instruction')))=lower(trim(?2)) AND json_extract(a.data,'$.preference_key') IS ?3 AND json_extract(a.data,'$.supersedes') IS ?4 AND NOT EXISTS(SELECT 1 FROM generated_skills s WHERE s.rule_id=a.id) ORDER BY p.proposed_at DESC LIMIT 1",params![encode(&proposal.rule.scope)?,proposal.rule.instruction,proposal.rule.preference_key,proposal.rule.supersedes.map(|id|id.to_string())],|r|r.get(0)).optional().map_err(|_| Error::Storage)?;
            if let Some(existing) = existing {
                let existing: RuleProposal = decode(existing)?;
                if let Some(id) = observation {
                    tx.execute(
                        "INSERT INTO learning_candidates(observation_id,proposal_id) VALUES(?1,?2)",
                        params![id.to_string(), existing.id.to_string()],
                    )
                    .map_err(|_| Error::Conflict)?;
                    tx.execute(
                        "UPDATE personal_observations SET processed=1 WHERE id=?1",
                        [id.to_string()],
                    )
                    .map_err(|_| Error::Storage)?;
                }
                tx.commit().map_err(|_| Error::Storage)?;
                return Ok(existing);
            }
        }
        if let Some(id) = proposal.rule.supersedes {
            let previous = current(&tx, id)?;
            if previous.scope != proposal.rule.scope
                || previous.status != AdaptiveRuleStatus::Enabled
            {
                return Err(Error::Conflict);
            }
            tx.execute("INSERT INTO personal_replacements(rule_id,previous_id,previous_version) VALUES(?1,?2,?3)", params![proposal.rule.id.to_string(), id.to_string(), previous.version]).map_err(|_| Error::Storage)?;
        }
        save(&tx, &proposal.rule, "proposed")?;
        tx.execute(
            "INSERT INTO rule_proposals(id,status,proposed_at,data) VALUES(?1,?2,?3,?4)",
            params![
                proposal.id.to_string(),
                encode(&proposal.rule.status)?,
                timestamp(proposal.proposed_at)?,
                encode(proposal)?
            ],
        )
        .map_err(|_| Error::Conflict)?;
        if let Some(skill) = skill {
            if skill.rule_id != proposal.rule.id {
                return Err(Error::InvalidInput);
            }
            tx.execute(
                "INSERT INTO generated_skills(rule_id,data) VALUES(?1,?2)",
                params![skill.rule_id.to_string(), encode(skill)?],
            )
            .map_err(|_| Error::Storage)?;
        }
        if let Some(id) = observation {
            tx.execute(
                "INSERT INTO learning_candidates(observation_id,proposal_id) VALUES(?1,?2)",
                params![id.to_string(), proposal.id.to_string()],
            )
            .map_err(|_| Error::Conflict)?;
            tx.execute(
                "UPDATE personal_observations SET processed=1 WHERE id=?1",
                [id.to_string()],
            )
            .map_err(|_| Error::Storage)?;
        }
        tx.commit().map_err(|_| Error::Storage)?;
        Ok(proposal.clone())
    }

    fn decide_personal_rule(
        &self,
        id: Id,
        expected_version: u32,
        decision: RuleDecision,
        now: u64,
    ) -> Result<AdaptiveRule> {
        let mut conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let tx = conn.transaction().map_err(|_| Error::Storage)?;
        let mut rule = current(&tx, id)?;
        if rule.version != expected_version {
            return Err(Error::Conflict);
        }
        let label = match decision {
            RuleDecision::Activate => {
                if rule.status != AdaptiveRuleStatus::Proposed {
                    return Err(Error::Conflict);
                }
                rule.status = AdaptiveRuleStatus::Enabled;
                "activated"
            }
            RuleDecision::Reject => {
                if rule.status != AdaptiveRuleStatus::Proposed {
                    return Err(Error::Conflict);
                }
                rule.status = AdaptiveRuleStatus::Rejected;
                "rejected"
            }
            RuleDecision::Disable => {
                if rule.status != AdaptiveRuleStatus::Enabled {
                    return Err(Error::Conflict);
                }
                rule.status = AdaptiveRuleStatus::Disabled;
                "disabled"
            }
            RuleDecision::Rollback { version } => {
                let old: AdaptiveRule = decode(
                    tx.query_row(
                        "SELECT data FROM personal_revisions WHERE rule_id=?1 AND version=?2",
                        params![id.to_string(), version],
                        |r| r.get(0),
                    )
                    .map_err(|_| Error::Unavailable)?,
                )?;
                if version >= expected_version || old.status != AdaptiveRuleStatus::Enabled {
                    return Err(Error::InvalidInput);
                }
                rule = old;
                "rollback"
            }
        };
        if rule.status == AdaptiveRuleStatus::Enabled {
            if rule.expires_at.is_some_and(|expiry| expiry <= now) {
                return Err(Error::InvalidInput);
            }
            if let Some(previous) = rule.supersedes {
                let expected: u32 = tx
                    .query_row(
                        "SELECT previous_version FROM personal_replacements WHERE rule_id=?1",
                        [id.to_string()],
                        |r| r.get(0),
                    )
                    .map_err(|_| Error::Conflict)?;
                let mut old = current(&tx, previous)?;
                if label == "activated" {
                    if old.version != expected || old.status != AdaptiveRuleStatus::Enabled {
                        return Err(Error::Conflict);
                    }
                    old.status = AdaptiveRuleStatus::Disabled;
                    old.version += 1;
                    old.updated_at = now;
                    save(&tx, &old, "replaced")?;
                } else if old.status == AdaptiveRuleStatus::Enabled {
                    return Err(Error::Conflict);
                }
            }
            // Restoring a replaced rule must atomically retire its active replacement.
            if label == "rollback" {
                let replacements = {
                    let mut stmt = tx.prepare("WITH RECURSIVE descendants(id) AS (SELECT id FROM adaptive_rules WHERE json_extract(data,'$.supersedes')=?2 UNION SELECT a.id FROM adaptive_rules a JOIN descendants d ON json_extract(a.data,'$.supersedes')=d.id) SELECT data FROM adaptive_rules WHERE status=?1 AND id IN (SELECT id FROM descendants)").map_err(|_| Error::Storage)?;
                    let rows = stmt
                        .query_map(
                            params![encode(&AdaptiveRuleStatus::Enabled)?, id.to_string()],
                            |r| r.get::<_, String>(0),
                        )
                        .map_err(|_| Error::Storage)?;
                    rows.map(|r| decode::<AdaptiveRule>(r.map_err(|_| Error::Storage)?))
                        .collect::<Result<Vec<_>>>()?
                };
                for mut replacement in replacements {
                    replacement.status = AdaptiveRuleStatus::Disabled;
                    replacement.version += 1;
                    replacement.updated_at = now;
                    save(&tx, &replacement, "rollback_replaced")?;
                }
            }
            let generated: Option<String> = tx
                .query_row(
                    "SELECT data FROM generated_skills WHERE rule_id=?1",
                    [id.to_string()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|_| Error::Storage)?;
            if let Some(key) = &rule.preference_key {
                let conflict: bool = tx
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM adaptive_rules WHERE status=?1 AND scope=?2 AND json_extract(data,'$.preference_key')=?3 AND id<>?4 AND id<>?5 AND (json_extract(data,'$.expires_at') IS NULL OR json_extract(data,'$.expires_at')>?6))",
                        params![
                            encode(&AdaptiveRuleStatus::Enabled)?,
                            encode(&rule.scope)?,
                            key,
                            id.to_string(),
                            rule.supersedes.unwrap_or(Id::nil()).to_string(),
                            timestamp(now)?
                        ],
                        |r| r.get(0),
                    )
                    .map_err(|_| Error::Storage)?;
                if conflict {
                    return Err(Error::Conflict);
                }
            }
            if let Some(data) = generated {
                let skill: GeneratedSkill = decode(data)?;
                if !skill.evaluation.passed || skill.evaluation.cases == 0 {
                    return Err(Error::Denied);
                }
                for binding in &skill.evaluation.tool_bindings {
                    let capability: Capability = decode(
                        tx.query_row(
                            "SELECT data FROM capabilities WHERE id=?1",
                            [&binding.id],
                            |r| r.get(0),
                        )
                        .map_err(|_| Error::Unavailable)?,
                    )?;
                    if capability != Capability::Tool(binding.clone()) || !binding.enabled {
                        return Err(Error::Conflict);
                    }
                }
            }
        }
        rule.version = expected_version.checked_add(1).ok_or(Error::Conflict)?;
        rule.updated_at = now;
        save(&tx, &rule, label)?;
        tx.commit().map_err(|_| Error::Storage)?;
        Ok(rule)
    }

    fn rule_history(&self, id: Id) -> Result<Vec<RuleRevision>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt = conn.prepare("SELECT data,decision,baseline FROM personal_revisions WHERE rule_id=?1 ORDER BY version DESC").map_err(|_| Error::Storage)?;
        let rows = stmt
            .query_map([id.to_string()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, bool>(2)?,
                ))
            })
            .map_err(|_| Error::Storage)?;
        rows.map(|r| {
            let (data, decision, historical_baseline) = r.map_err(|_| Error::Storage)?;
            Ok(RuleRevision {
                rule: decode(data)?,
                decision,
                historical_baseline,
            })
        })
        .collect()
    }

    fn record_observation(&self, observation: &Observation) -> Result<Observation> {
        observation.validate()?;
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        // Only persisted source records are eligible: temporary chats have no row.
        let task: Option<String> = conn
            .query_row(
                "SELECT data FROM tasks WHERE id=?1 AND conversation_id=?2",
                params![
                    observation.source_id.to_string(),
                    observation.conversation_id.to_string()
                ],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| Error::Storage)?;
        let message: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM messages WHERE id=?1 AND conversation_id=?2 AND status='complete')", params![observation.source_id.to_string(),observation.conversation_id.to_string()], |r| r.get(0)).map_err(|_| Error::Storage)?;
        if !message
            && !task
                .map(decode::<Task>)
                .transpose()?
                .is_some_and(|t| t.status.terminal())
        {
            return Err(Error::Denied);
        }
        let existing: Option<String> = conn.query_row("SELECT data FROM personal_observations WHERE id=?1 OR (source_id=?2 AND kind=?3 AND json_extract(data,'$.text')=?4)", params![observation.id.to_string(),observation.source_id.to_string(),encode(&observation.kind)?,observation.text], |r| r.get(0)).optional().map_err(|_| Error::Storage)?;
        if let Some(data) = existing {
            let old: Observation = decode(data)?;
            if old.source_id != observation.source_id
                || old.kind != observation.kind
                || old.text != observation.text
            {
                return Err(Error::Conflict);
            }
            return Ok(old);
        }
        conn.execute("INSERT INTO personal_observations(id,conversation_id,source_id,kind,data) VALUES(?1,?2,?3,?4,?5)", params![observation.id.to_string(),observation.conversation_id.to_string(),observation.source_id.to_string(),encode(&observation.kind)?,encode(observation)?]).map_err(|_| Error::Storage)?;
        Ok(observation.clone())
    }

    fn observation(&self, id: Id) -> Result<Observation> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        observation_in(&conn, id)
    }

    fn pending_observations(&self, limit: usize) -> Result<Vec<Observation>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt = conn.prepare("SELECT data FROM personal_observations WHERE processed=0 AND kind<>'\"outcome\"' ORDER BY rowid LIMIT ?1").map_err(|_| Error::Storage)?;
        let rows = stmt
            .query_map([read_limit(limit)], |r| r.get::<_, String>(0))
            .map_err(|_| Error::Storage)?;
        rows.map(|r| decode(r.map_err(|_| Error::Storage)?))
            .collect()
    }

    fn skip_observation(&self, id: Id) -> Result<()> {
        self.connection
            .lock()
            .map_err(|_| Error::Storage)?
            .execute(
                "UPDATE personal_observations SET processed=1 WHERE id=?1",
                [id.to_string()],
            )
            .map_err(|_| Error::Storage)?;
        Ok(())
    }

    fn record_personal_usage(
        &self,
        run_id: Id,
        conversation: Id,
        rules: &[AdaptiveRule],
    ) -> Result<()> {
        let mut conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let tx = conn.transaction().map_err(|_| Error::Storage)?;
        for rule in rules {
            tx.execute("INSERT OR IGNORE INTO personal_usage(run_id,conversation_id,rule_id,version) VALUES(?1,?2,?3,?4)", params![run_id.to_string(),conversation.to_string(),rule.id.to_string(),rule.version]).map_err(|_| Error::Storage)?;
        }
        tx.commit().map_err(|_| Error::Storage)
    }

    fn personal_usage(&self, run_id: Id) -> Result<Vec<RuleRevision>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt = conn.prepare("SELECT r.data,r.decision,r.baseline FROM personal_usage u JOIN personal_revisions r ON r.rule_id=u.rule_id AND r.version=u.version WHERE u.run_id=?1 ORDER BY u.rule_id,u.version").map_err(|_| Error::Storage)?;
        let rows = stmt
            .query_map([run_id.to_string()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, bool>(2)?,
                ))
            })
            .map_err(|_| Error::Storage)?;
        rows.map(|r| {
            let (data, decision, historical_baseline) = r.map_err(|_| Error::Storage)?;
            Ok(RuleRevision {
                rule: decode(data)?,
                decision,
                historical_baseline,
            })
        })
        .collect()
    }

    fn generated_skill(&self, rule_id: Id) -> Result<Option<GeneratedSkill>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.query_row(
            "SELECT data FROM generated_skills WHERE rule_id=?1",
            [rule_id.to_string()],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|_| Error::Storage)?
        .map(decode)
        .transpose()
    }
}
