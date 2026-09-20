//! SQLite persistence and a shared, bounded FTS5 capability index.
mod conversation;
mod memory;
mod research;

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
