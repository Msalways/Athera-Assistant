//! SQLite persistence and a shared, bounded FTS5 capability index.
mod conversation;
mod memory;
mod personalization;
mod research;

use assistant_contracts::credential::CredentialPurpose;
use assistant_contracts::*;
use rusqlite::{params, Connection};
use serde_json::Value;
use std::{path::Path, sync::Mutex};

pub(crate) const MAX_LIST_ITEMS: usize = 500;

pub struct SqliteStore {
    connection: Mutex<Connection>,
}

impl SqliteStore {
    /// Opens the database, applies migrations, and interrupts unfinished generation.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_connection(Connection::open(path).map_err(|_| Error::Storage)?)
    }

    /// Opens an in-memory database with the same schema as a file-backed store.
    pub fn memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory().map_err(|_| Error::Storage)?)
    }

    fn from_connection(mut connection: Connection) -> Result<Self> {
        connection
            .execute_batch(include_str!("../../../migrations/001_initial.sql"))
            .map_err(|_| Error::Storage)?;
        let tx = connection.transaction().map_err(|_| Error::Storage)?;
        tx.execute_batch(include_str!("../../../migrations/002_conversations.sql"))
            .map_err(|_| Error::Storage)?;
        tx.execute_batch(include_str!("../../../migrations/003_run_events.sql"))
            .map_err(|_| Error::Storage)?;
        let has_stream_events = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=4)",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|_| Error::Storage)?;
        if !has_stream_events {
            tx.execute_batch(include_str!("../../../migrations/004_stream_events.sql"))
                .map_err(|_| Error::Storage)?;
        }
        let has_adaptive_rules = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=5)",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|_| Error::Storage)?;
        if !has_adaptive_rules {
            tx.execute_batch(include_str!("../../../migrations/005_adaptive_rules.sql"))
                .map_err(|_| Error::Storage)?;
        }
        if !tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=6)",
                [],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|_| Error::Storage)?
        {
            tx.execute_batch(include_str!("../../../migrations/006_personalization.sql"))
                .map_err(|_| Error::Storage)?;
        }
        if !tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=7)",
                [],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|_| Error::Storage)?
        {
            tx.execute_batch(include_str!(
                "../../../migrations/007_provider_profiles.sql"
            ))
            .map_err(|_| Error::Storage)?;
        }
        if !tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=8)",
                [],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|_| Error::Storage)?
        {
            tx.execute_batch(include_str!("../../../migrations/008_routing_usage.sql"))
                .map_err(|_| Error::Storage)?;
        }
        if !tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=9)",
                [],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|_| Error::Storage)?
        {
            tx.execute_batch(include_str!("../../../migrations/009_task_blockers.sql"))
                .map_err(|_| Error::Storage)?;
        }
        if !tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=10)",
                [],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|_| Error::Storage)?
        {
            tx.execute_batch(include_str!("../../../migrations/010_jobs.sql"))
                .map_err(|_| Error::Storage)?;
        }
        tx.execute(
            "UPDATE messages SET status='interrupted' WHERE status='generating'",
            [],
        )
        .map_err(|_| Error::Storage)?;
        tx.commit().map_err(|_| Error::Storage)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
}

pub(crate) fn timestamp(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| Error::InvalidInput)
}

pub(crate) fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(crate) fn blocker_kind(blocker: &TaskBlocker) -> &'static str {
    match blocker {
        TaskBlocker::ProviderCredentialRequired { .. } => "provider_credential_required",
        TaskBlocker::ConnectorAuthorizationRequired { .. } => "connector_authorization_required",
        TaskBlocker::AndroidPermissionRequired { .. } => "android_permission_required",
        TaskBlocker::ApprovalRequired { .. } => "approval_required",
        TaskBlocker::ClarificationRequired { .. } => "clarification_required",
        TaskBlocker::DeviceConstraint { .. } => "device_constraint",
        TaskBlocker::CapabilityUnavailable { .. } => "capability_unavailable",
    }
}

pub(crate) fn read_limit(limit: usize) -> i64 {
    limit.min(MAX_LIST_ITEMS) as i64
}

pub(crate) fn parse_id(value: &str) -> Result<Id> {
    Id::parse_str(value).map_err(|_| Error::Storage)
}

pub(crate) fn encode<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|_| Error::Storage)
}

fn decode<T: serde::de::DeserializeOwned>(value: String) -> Result<T> {
    serde_json::from_str(&value).map_err(|_| Error::Storage)
}

impl Store for SqliteStore {
    fn setting(&self, key: &str) -> Result<Option<Value>> {
        use rusqlite::OptionalExtension;
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let value: Option<String> = conn
            .query_row("SELECT data FROM settings WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|_| Error::Storage)?;
        value.map(decode).transpose()
    }

    fn set_setting(&self, key: &str, value: &Value) -> Result<()> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.execute("INSERT INTO settings(key,data) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET data=excluded.data", params![key, encode(value)?]).map_err(|_| Error::Storage)?;
        Ok(())
    }

    fn put_adaptive_rule(&self, rule: &AdaptiveRule) -> Result<()> {
        rule.validate()?;
        let mut conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let tx = conn.transaction().map_err(|_| Error::Storage)?;
        personalization::save(&tx, rule, "trusted_import")?;
        tx.commit().map_err(|_| Error::Storage)
    }

    fn adaptive_rule(&self, id: Id) -> Result<AdaptiveRule> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        decode(
            conn.query_row(
                "SELECT data FROM adaptive_rules WHERE id=?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .map_err(|_| Error::Unavailable)?,
        )
    }

    fn set_adaptive_rule_status(
        &self,
        id: Id,
        status: AdaptiveRuleStatus,
        updated_at: u64,
    ) -> Result<()> {
        let rule = self.adaptive_rule(id)?;
        let decision = match status {
            AdaptiveRuleStatus::Enabled => RuleDecision::Activate,
            AdaptiveRuleStatus::Rejected => RuleDecision::Reject,
            AdaptiveRuleStatus::Disabled => RuleDecision::Disable,
            AdaptiveRuleStatus::Proposed => return Err(Error::InvalidInput),
        };
        self.decide_personal_rule(id, rule.version, decision, updated_at)?;
        Ok(())
    }

    fn adaptive_rules(&self, scope: Option<&RuleScope>, limit: usize) -> Result<Vec<AdaptiveRule>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt = conn.prepare("SELECT data FROM adaptive_rules WHERE status=?1 ORDER BY json_extract(data,'$.priority') DESC, updated_at DESC LIMIT ?2").map_err(|_| Error::Storage)?;
        let rows = stmt
            .query_map(
                params![encode(&AdaptiveRuleStatus::Enabled)?, read_limit(limit)],
                |r| r.get::<_, String>(0),
            )
            .map_err(|_| Error::Storage)?;
        rows.map(|row| decode(row.map_err(|_| Error::Storage)?))
            .filter(|rule: &Result<AdaptiveRule>| match (scope, rule) {
                (Some(expected), Ok(rule)) => &rule.scope == expected,
                _ => true,
            })
            .collect()
    }

    fn put_rule_proposal(&self, proposal: &RuleProposal) -> Result<()> {
        proposal.validate()?;
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.execute("INSERT INTO rule_proposals(id,status,proposed_at,data) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET status=excluded.status,proposed_at=excluded.proposed_at,data=excluded.data", params![proposal.id.to_string(), encode(&proposal.rule.status)?, timestamp(proposal.proposed_at)?, encode(proposal)?]).map_err(|_| Error::Storage)?;
        Ok(())
    }

    fn rule_proposal(&self, id: Id) -> Result<RuleProposal> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        decode(
            conn.query_row(
                "SELECT data FROM rule_proposals WHERE id=?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .map_err(|_| Error::Unavailable)?,
        )
    }

    fn rule_proposals(
        &self,
        status: Option<AdaptiveRuleStatus>,
        limit: usize,
    ) -> Result<Vec<RuleProposal>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt = conn
            .prepare("SELECT data FROM rule_proposals ORDER BY proposed_at DESC LIMIT ?1")
            .map_err(|_| Error::Storage)?;
        let rows = stmt
            .query_map([read_limit(limit)], |r| r.get::<_, String>(0))
            .map_err(|_| Error::Storage)?;
        rows.map(|row| decode(row.map_err(|_| Error::Storage)?))
            .filter(|proposal: &Result<RuleProposal>| match (status, proposal) {
                (Some(expected), Ok(proposal)) => proposal.rule.status == expected,
                _ => true,
            })
            .collect()
    }

    fn save_provider_profile(&self, profile: &ProviderProfile) -> Result<()> {
        profile.validate().map_err(|_| Error::InvalidInput)?;
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.execute(
            "INSERT INTO provider_profiles(provider_id, auth_option_id, non_secret_config, schema_version, enabled, display_name, created_at, updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(provider_id) DO UPDATE SET auth_option_id=excluded.auth_option_id, non_secret_config=excluded.non_secret_config, schema_version=excluded.schema_version, enabled=excluded.enabled, display_name=excluded.display_name, updated_at=excluded.updated_at",
            params![
                profile.provider_id,
                profile.auth_option_id,
                encode(&profile.non_secret_config)?,
                1i64,
                profile.enabled as i64,
                profile.display_name,
                timestamp(profile.created_at)?,
                timestamp(profile.updated_at)?,
            ],
        )
        .map_err(|_| Error::Storage)?;
        Ok(())
    }

    fn provider_profile(&self, provider_id: &str) -> Result<Option<ProviderProfile>> {
        use rusqlite::OptionalExtension;
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let row = conn
            .query_row(
                "SELECT provider_id, auth_option_id, non_secret_config, schema_version, enabled, display_name, created_at, updated_at FROM provider_profiles WHERE provider_id=?1",
                [provider_id],
                |row| {
                    let pid: String = row.get(0)?;
                    let auth: String = row.get(1)?;
                    let config: String = row.get(2)?;
                    let _schema_v: i64 = row.get(3)?;
                    let enabled: bool = row.get::<_, i64>(4)? != 0;
                    let display: Option<String> = row.get(5)?;
                    let created: i64 = row.get(6)?;
                    let updated: i64 = row.get(7)?;
                    Ok(ProviderProfile {
                        schema: PROVIDER_PROFILE_SCHEMA_V1.into(),
                        provider_id: pid,
                        auth_option_id: auth,
                        non_secret_config: serde_json::from_str(&config).unwrap_or_default(),
                        enabled,
                        display_name: display,
                        created_at: created as u64,
                        updated_at: updated as u64,
                    })
                },
            )
            .optional()
            .map_err(|_| Error::Storage)?;
        Ok(row)
    }

    fn provider_profiles(&self) -> Result<Vec<ProviderProfile>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt = conn
            .prepare("SELECT provider_id, auth_option_id, non_secret_config, schema_version, enabled, display_name, created_at, updated_at FROM provider_profiles ORDER BY provider_id")
            .map_err(|_| Error::Storage)?;
        let rows = stmt
            .query_map([], |row| {
                let pid: String = row.get(0)?;
                let auth: String = row.get(1)?;
                let config: String = row.get(2)?;
                let _schema_v: i64 = row.get(3)?;
                let enabled: bool = row.get::<_, i64>(4)? != 0;
                let display: Option<String> = row.get(5)?;
                let created: i64 = row.get(6)?;
                let updated: i64 = row.get(7)?;
                Ok(ProviderProfile {
                    schema: PROVIDER_PROFILE_SCHEMA_V1.into(),
                    provider_id: pid,
                    auth_option_id: auth,
                    non_secret_config: serde_json::from_str(&config).unwrap_or_default(),
                    enabled,
                    display_name: display,
                    created_at: created as u64,
                    updated_at: updated as u64,
                })
            })
            .map_err(|_| Error::Storage)?;
        rows.map(|r| r.map_err(|_| Error::Storage)).collect()
    }

    fn delete_provider_profile(&self, provider_id: &str) -> Result<()> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.execute(
            "DELETE FROM provider_profiles WHERE provider_id=?1",
            [provider_id],
        )
        .map_err(|_| Error::Storage)?;
        Ok(())
    }

    fn save_credential_metadata(&self, meta: &CredentialMetadata) -> Result<()> {
        meta.validate().map_err(|_| Error::InvalidInput)?;
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.execute(
            "INSERT INTO credential_metadata(credential_handle, owner_id, purpose, provider_id, connection_id, created_at, expires_at, last_verified_at, status) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(credential_handle) DO UPDATE SET owner_id=excluded.owner_id, purpose=excluded.purpose, provider_id=excluded.provider_id, connection_id=excluded.connection_id, expires_at=excluded.expires_at, last_verified_at=excluded.last_verified_at, status=excluded.status",
            params![
                meta.credential_handle,
                meta.owner_id,
                encode(&meta.purpose)?,
                meta.provider_id,
                meta.connection_id,
                timestamp(meta.created_at)?,
                meta.expires_at.map(timestamp).transpose()?,
                meta.last_verified_at.map(timestamp).transpose()?,
                encode(&meta.status)?,
            ],
        )
        .map_err(|_| Error::Storage)?;
        Ok(())
    }

    fn credential_metadata(&self, handle: &str) -> Result<Option<CredentialMetadata>> {
        use rusqlite::OptionalExtension;
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let row = conn
            .query_row(
                "SELECT credential_handle, owner_id, purpose, provider_id, connection_id, created_at, expires_at, last_verified_at, status FROM credential_metadata WHERE credential_handle=?1",
                [handle],
                |row| {
                    let h: String = row.get(0)?;
                    let owner: String = row.get(1)?;
                    let purpose: String = row.get(2)?;
                    let provider: Option<String> = row.get(3)?;
                    let conn: Option<String> = row.get(4)?;
                    let created: i64 = row.get(5)?;
                    let expires: Option<i64> = row.get(6)?;
                    let verified: Option<i64> = row.get(7)?;
                    let status: String = row.get(8)?;
                    Ok(CredentialMetadata {
                        schema: CREDENTIAL_METADATA_SCHEMA_V1.into(),
                        credential_handle: h,
                        owner_id: owner,
                        purpose: serde_json::from_str(&purpose).unwrap_or(CredentialPurpose::ProviderAuth),
                        provider_id: provider,
                        connection_id: conn,
                        created_at: created as u64,
                        expires_at: expires.map(|v| v as u64),
                        last_verified_at: verified.map(|v| v as u64),
                        status: serde_json::from_str(&status).unwrap_or(CredentialLifecycleStatus::Active),
                    })
                },
            )
            .optional()
            .map_err(|_| Error::Storage)?;
        Ok(row)
    }

    fn credential_metadata_for_owner(&self, owner_id: &str) -> Result<Vec<CredentialMetadata>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt = conn
            .prepare("SELECT credential_handle, owner_id, purpose, provider_id, connection_id, created_at, expires_at, last_verified_at, status FROM credential_metadata WHERE owner_id=?1 ORDER BY created_at DESC")
            .map_err(|_| Error::Storage)?;
        let rows = stmt
            .query_map([owner_id], |row| {
                let h: String = row.get(0)?;
                let owner: String = row.get(1)?;
                let purpose: String = row.get(2)?;
                let provider: Option<String> = row.get(3)?;
                let conn: Option<String> = row.get(4)?;
                let created: i64 = row.get(5)?;
                let expires: Option<i64> = row.get(6)?;
                let verified: Option<i64> = row.get(7)?;
                let status: String = row.get(8)?;
                Ok(CredentialMetadata {
                    schema: CREDENTIAL_METADATA_SCHEMA_V1.into(),
                    credential_handle: h,
                    owner_id: owner,
                    purpose: serde_json::from_str(&purpose)
                        .unwrap_or(CredentialPurpose::ProviderAuth),
                    provider_id: provider,
                    connection_id: conn,
                    created_at: created as u64,
                    expires_at: expires.map(|v| v as u64),
                    last_verified_at: verified.map(|v| v as u64),
                    status: serde_json::from_str(&status)
                        .unwrap_or(CredentialLifecycleStatus::Active),
                })
            })
            .map_err(|_| Error::Storage)?;
        rows.map(|r| r.map_err(|_| Error::Storage)).collect()
    }

    fn delete_credential_metadata(&self, handle: &str) -> Result<()> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.execute(
            "DELETE FROM credential_metadata WHERE credential_handle=?1",
            [handle],
        )
        .map_err(|_| Error::Storage)?;
        Ok(())
    }

    fn delete_all_credential_metadata(&self, owner_id: &str) -> Result<()> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.execute(
            "DELETE FROM credential_metadata WHERE owner_id=?1",
            [owner_id],
        )
        .map_err(|_| Error::Storage)?;
        Ok(())
    }

    fn save_routing_provenance(&self, task_id: Id, provenance: &RoutingProvenance) -> Result<()> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.execute(
            "INSERT INTO routing_provenance(task_id, strategy, provider_id, reason, egress, latency_ms, routed_at, data) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(task_id) DO UPDATE SET strategy=excluded.strategy, provider_id=excluded.provider_id, reason=excluded.reason, egress=excluded.egress, latency_ms=excluded.latency_ms, routed_at=excluded.routed_at, data=excluded.data",
            params![
                task_id.to_string(),
                encode(&provenance.strategy)?,
                provenance.provider_id,
                provenance.reason,
                encode(&provenance.data_egress_class)?,
                provenance.latency_ms.map(timestamp).transpose()?,
                timestamp(provenance.routed_at)?,
                encode(provenance)?,
            ],
        )
        .map_err(|_| Error::Storage)?;
        Ok(())
    }

    fn routing_provenance(&self, task_id: Id) -> Result<Option<RoutingProvenance>> {
        use rusqlite::OptionalExtension;
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let row = conn
            .query_row(
                "SELECT data FROM routing_provenance WHERE task_id=?1",
                [task_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| Error::Storage)?;
        row.map(decode).transpose()
    }

    fn record_model_usage(&self, usage: &StoredModelUsage) -> Result<()> {
        usage.validate().map_err(|_| Error::InvalidInput)?;
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.execute(
            "INSERT INTO model_usage(task_id, prompt_tokens, completion_tokens, total_tokens, model_id, recorded_at) VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                usage.task_id,
                usage.usage.prompt_tokens as i64,
                usage.usage.completion_tokens as i64,
                usage.usage.total_tokens as i64,
                usage.model_id,
                timestamp(usage.recorded_at)?,
            ],
        )
        .map_err(|_| Error::Storage)?;
        Ok(())
    }

    fn model_usage_for_task(&self, task_id: &str) -> Result<Vec<StoredModelUsage>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt = conn
            .prepare("SELECT task_id, prompt_tokens, completion_tokens, total_tokens, model_id, recorded_at FROM model_usage WHERE task_id=?1 ORDER BY id")
            .map_err(|_| Error::Storage)?;
        let rows = stmt
            .query_map([task_id], |row| {
                Ok(StoredModelUsage {
                    task_id: row.get(0)?,
                    usage: ModelUsage {
                        prompt_tokens: row.get::<_, i64>(1)? as u32,
                        completion_tokens: row.get::<_, i64>(2)? as u32,
                        total_tokens: row.get::<_, i64>(3)? as u32,
                    },
                    model_id: row.get(4)?,
                    recorded_at: row.get::<_, i64>(5)? as u64,
                })
            })
            .map_err(|_| Error::Storage)?;
        rows.map(|r| r.map_err(|_| Error::Storage)).collect()
    }

    fn save_task_blocker(&self, task_id: Id, blocker: &TaskBlocker) -> Result<()> {
        blocker.validate().map_err(|_| Error::InvalidInput)?;
        let now = crate::now_millis();
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.execute(
            "INSERT INTO task_blockers(task_id, kind, data, created_at, updated_at) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(task_id) DO UPDATE SET kind=excluded.kind, data=excluded.data, updated_at=excluded.updated_at",
            params![
                task_id.to_string(),
                encode(&blocker_kind(blocker))?,
                encode(blocker)?,
                timestamp(now)?,
                timestamp(now)?,
            ],
        )
        .map_err(|_| Error::Storage)?;
        Ok(())
    }

    fn task_blocker(&self, task_id: Id) -> Result<Option<TaskBlocker>> {
        use rusqlite::OptionalExtension;
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let row = conn
            .query_row(
                "SELECT data FROM task_blockers WHERE task_id=?1",
                [task_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| Error::Storage)?;
        row.map(decode).transpose()
    }

    fn delete_task_blocker(&self, task_id: Id) -> Result<()> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.execute(
            "DELETE FROM task_blockers WHERE task_id=?1",
            [task_id.to_string()],
        )
        .map_err(|_| Error::Storage)?;
        Ok(())
    }

    fn save_job(&self, job: &DurableJob) -> Result<()> {
        job.validate().map_err(|_| Error::InvalidInput)?;
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.execute(
            "INSERT INTO jobs(id, task_id, objective, success_condition, status, run_at, condition, allowed_tools, max_actions, actions_taken, created_at, updated_at, data) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13) ON CONFLICT(id) DO UPDATE SET task_id=excluded.task_id, objective=excluded.objective, success_condition=excluded.success_condition, status=excluded.status, run_at=excluded.run_at, condition=excluded.condition, allowed_tools=excluded.allowed_tools, max_actions=excluded.max_actions, actions_taken=excluded.actions_taken, updated_at=excluded.updated_at, data=excluded.data",
            params![
                job.id,
                job.task_id,
                job.objective,
                job.success_condition,
                encode(&job.status)?,
                job.trigger.run_at.map(timestamp).transpose()?,
                job.trigger.condition,
                encode(&job.allowed_tools)?,
                job.max_actions as i64,
                job.actions_taken as i64,
                timestamp(job.created_at)?,
                timestamp(job.updated_at)?,
                encode(job)?,
            ],
        )
        .map_err(|_| Error::Storage)?;
        Ok(())
    }

    fn job(&self, id: &str) -> Result<Option<DurableJob>> {
        use rusqlite::OptionalExtension;
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let row = conn
            .query_row("SELECT data FROM jobs WHERE id=?1", [id], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .map_err(|_| Error::Storage)?;
        row.map(decode).transpose()
    }

    fn due_jobs(&self, now_millis: u64) -> Result<Vec<DurableJob>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt = conn
            .prepare("SELECT data FROM jobs WHERE status=?1 AND run_at IS NOT NULL AND run_at<=?2 ORDER BY run_at")
            .map_err(|_| Error::Storage)?;
        let rows = stmt
            .query_map(
                params![encode(&JobStatus::Scheduled)?, timestamp(now_millis)?],
                |row| row.get::<_, String>(0),
            )
            .map_err(|_| Error::Storage)?;
        rows.map(|row| decode(row.map_err(|_| Error::Storage)?))
            .collect()
    }

    fn save_task(&self, task: &Task) -> Result<()> {
        let mut conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let tx = conn.transaction().map_err(|_| Error::Storage)?;
        tx.execute("INSERT INTO tasks(id,conversation_id,data) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET data=excluded.data", params![task.id.to_string(), task.input.conversation_id.to_string(), encode(task)?]).map_err(|_| Error::Storage)?;
        let terminal_exists = task.status.terminal()
            && tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM run_events WHERE task_id=?1 AND status IN (?2,?3,?4))",
                    params![
                        task.id.to_string(),
                        encode(&TaskStatus::Completed)?,
                        encode(&TaskStatus::Failed)?,
                        encode(&TaskStatus::Cancelled)?
                    ],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(|_| Error::Storage)?;
        if terminal_exists {
            return tx.commit().map_err(|_| Error::Storage);
        }
        if task.status == TaskStatus::Completed {
            if let Some(output) = &task.output {
                tx.execute(
                    "INSERT INTO run_events(task_id,status,step,kind,message,output) VALUES(?1,?2,?3,?4,'',?5)",
                    params![
                        task.id.to_string(),
                        encode(&TaskStatus::Running)?,
                        task.step,
                        encode(&RunEventKind::OutputUpsert)?,
                        encode(output)?
                    ],
                )
                .map_err(|_| Error::Storage)?;
            }
        }
        let kind = event_kind(task.status);
        tx.execute(
            "INSERT INTO run_events(task_id,status,step,kind,message,output) VALUES(?1,?2,?3,?4,?5,NULL)",
            params![
                task.id.to_string(),
                encode(&task.status)?,
                task.step,
                encode(&kind)?,
                task.message
            ],
        )
        .map_err(|_| Error::Storage)?;
        tx.commit().map_err(|_| Error::Storage)
    }

    fn task(&self, id: Id) -> Result<Task> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        decode(
            conn.query_row(
                "SELECT data FROM tasks WHERE id=?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .map_err(|_| Error::Storage)?,
        )
    }

    fn tasks(&self) -> Result<Vec<Task>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt = conn
            .prepare("SELECT data FROM tasks ORDER BY rowid DESC LIMIT 200")
            .map_err(|_| Error::Storage)?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|_| Error::Storage)?;
        rows.map(|r| decode(r.map_err(|_| Error::Storage)?))
            .collect()
    }

    fn unfinished_tasks(&self) -> Result<Vec<Task>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt = conn
            .prepare("SELECT data FROM tasks ORDER BY rowid")
            .map_err(|_| Error::Storage)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|_| Error::Storage)?;
        let tasks: Vec<Task> = rows
            .map(|row| decode(row.map_err(|_| Error::Storage)?))
            .collect::<Result<_>>()?;
        Ok(tasks
            .into_iter()
            .filter(|task| !task.status.terminal())
            .collect())
    }

    fn save_result(&self, id: Id, value: &Value) -> Result<()> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.execute(
            "INSERT INTO tool_results(id,data) VALUES(?1,?2)",
            params![id.to_string(), encode(value)?],
        )
        .map_err(|_| Error::Storage)?;
        Ok(())
    }

    fn result(&self, id: Id) -> Result<Value> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        decode(
            conn.query_row(
                "SELECT data FROM tool_results WHERE id=?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .map_err(|_| Error::Storage)?,
        )
    }

    fn put_capability(&self, spec: &Capability) -> Result<()> {
        let mut conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let tx = conn.transaction().map_err(|_| Error::Storage)?;
        tx.execute("INSERT INTO capabilities(id,data) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data", params![spec.id(), encode(spec)?]).map_err(|_| Error::Storage)?;
        tx.execute("DELETE FROM capability_search WHERE id=?1", [spec.id()])
            .map_err(|_| Error::Storage)?;
        if spec.enabled() {
            let kind = match spec {
                Capability::Tool(_) => "tool",
                Capability::Skill(_) => "skill",
            };
            tx.execute(
                "INSERT INTO capability_search(id,kind,name,description) VALUES(?1,?2,?3,?4)",
                params![spec.id(), kind, spec.name(), spec.description()],
            )
            .map_err(|_| Error::Storage)?;
        }
        tx.commit().map_err(|_| Error::Storage)
    }

    fn capability(&self, id: &str) -> Result<Capability> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        decode(
            conn.query_row("SELECT data FROM capabilities WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .map_err(|_| Error::Unavailable)?,
        )
    }

    fn capabilities(&self) -> Result<Vec<Capability>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt = conn
            .prepare("SELECT data FROM capabilities ORDER BY id")
            .map_err(|_| Error::Storage)?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|_| Error::Storage)?;
        rows.map(|r| decode(r.map_err(|_| Error::Storage)?))
            .collect()
    }

    fn search(&self, query: &str, limit: usize) -> Result<Vec<Candidate>> {
        let tokens: Vec<_> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .take(24)
            .map(|s| format!("\"{s}\"*"))
            .collect();
        if tokens.is_empty() {
            return Ok(vec![]);
        }
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt = conn.prepare("SELECT id,kind,description FROM capability_search WHERE capability_search MATCH ?1 ORDER BY rank LIMIT ?2").map_err(|_| Error::Storage)?;
        let rows = stmt
            .query_map(params![tokens.join(" OR "), limit.min(20)], |r| {
                Ok(Candidate {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    description: r.get(2)?,
                })
            })
            .map_err(|_| Error::Storage)?;
        rows.map(|r| r.map_err(|_| Error::Storage)).collect()
    }

    fn events(&self, after: u64) -> Result<Vec<AssistantEvent>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt = conn.prepare("SELECT sequence,task_id,status,step,kind,message,output,worker_id,text_delta FROM run_events WHERE sequence>?1 ORDER BY sequence LIMIT 200").map_err(|_| Error::Storage)?;
        let rows = stmt
            .query_map([after], |r| {
                Ok((
                    r.get::<_, u64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, u32>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, Option<String>>(8)?,
                ))
            })
            .map_err(|_| Error::Storage)?;
        rows.map(|r| {
            let (sequence, id, status, step, kind, message, output, worker_id, text_delta) =
                r.map_err(|_| Error::Storage)?;
            let run_id = Id::parse_str(&id).map_err(|_| Error::Storage)?;
            let status: TaskStatus = decode(status)?;
            Ok(AssistantEvent {
                schema: RUN_EVENT_SCHEMA_V1.into(),
                event_id: format!("{run_id}:{sequence}"),
                sequence,
                run_id,
                task_id: run_id,
                worker_id: worker_id.map(|id| parse_id(&id)).transpose()?,
                kind: kind
                    .map(decode)
                    .transpose()?
                    .unwrap_or_else(|| event_kind(status)),
                status,
                step,
                message,
                text_delta,
                output: output.map(decode).transpose()?,
            })
        })
        .collect()
    }

    fn append_event(&self, event: &NewRunEvent) -> Result<()> {
        event.validate()?;
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.execute(
            "INSERT INTO run_events(task_id,status,step,kind,message,output,worker_id,text_delta) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                event.run_id.to_string(),
                encode(&event.status)?,
                event.step,
                encode(&event.kind)?,
                event.message,
                event.output.as_ref().map(encode).transpose()?,
                event.worker_id.map(|id| id.to_string()),
                event.text_delta
            ],
        )
        .map_err(|_| Error::Storage)?;
        Ok(())
    }

    fn event_bounds(&self) -> Result<Option<EventBounds>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let (oldest, newest) = conn
            .query_row(
                "SELECT MIN(sequence), MAX(sequence) FROM run_events",
                [],
                |row| Ok((row.get::<_, Option<u64>>(0)?, row.get::<_, Option<u64>>(1)?)),
            )
            .map_err(|_| Error::Storage)?;
        Ok(oldest
            .zip(newest)
            .map(|(oldest, newest)| EventBounds { oldest, newest }))
    }
}

fn event_kind(status: TaskStatus) -> RunEventKind {
    match status {
        TaskStatus::Created => RunEventKind::RunStarted,
        TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled => {
            RunEventKind::RunTerminal
        }
        TaskStatus::WaitingForAuth
        | TaskStatus::WaitingForApproval
        | TaskStatus::WaitingForUser
        | TaskStatus::WaitingForResolution => RunEventKind::RunPaused,
        TaskStatus::Running => RunEventKind::TaskState,
    }
}
