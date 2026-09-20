use crate::{encode, parse_id, read_limit, timestamp, SqliteStore, MAX_LIST_ITEMS};
use assistant_contracts::{
    conversation::{NoteRevision, ResearchSession, SourceReference},
    Error, Id, Result,
};
use rusqlite::{params, Connection, OptionalExtension};

impl SqliteStore {
    /// Creates a research session and its supplied immutable note revisions.
    pub fn create_research_session(&self, session: &ResearchSession) -> Result<()> {
        let mut conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let tx = conn.transaction().map_err(|_| Error::Storage)?;
        let inserted = insert_research_session(&tx, session, false)?;
        if !inserted {
            return Err(Error::InvalidInput);
        }
        insert_notes(&tx, session.id, &session.notes)?;
        tx.commit().map_err(|_| Error::Storage)
    }

    /// Updates research metadata and inserts previously unseen note revisions.
    pub fn upsert_research_session(&self, session: &ResearchSession) -> Result<()> {
        let mut conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let tx = conn.transaction().map_err(|_| Error::Storage)?;
        let inserted = insert_research_session(&tx, session, true)?;
        if !inserted {
            return Err(Error::InvalidInput);
        }
        insert_notes(&tx, session.id, &session.notes)?;
        tx.commit().map_err(|_| Error::Storage)
    }

    /// Lists recent research sessions for a conversation, capped at 500 rows.
    pub fn list_research_sessions(
        &self,
        conversation_id: Id,
        limit: usize,
    ) -> Result<Vec<ResearchSession>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let raw = {
            let mut stmt = conn
                .prepare(
                    "SELECT id,conversation_id,title,sources,hypotheses,decisions,experiments \
                     FROM research_sessions WHERE conversation_id=?1 ORDER BY rowid DESC LIMIT ?2",
                )
                .map_err(|_| Error::Storage)?;
            let rows = stmt
                .query_map(
                    params![conversation_id.to_string(), read_limit(limit)],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                            row.get(6)?,
                        ))
                    },
                )
                .map_err(|_| Error::Storage)?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|_| Error::Storage)?
        };
        raw.into_iter()
            .map(|row| research_from(&conn, row))
            .collect()
    }

    /// Gets a research session and up to its latest 500 note revisions.
    pub fn get_research_session(&self, id: Id) -> Result<Option<ResearchSession>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let row = conn
            .query_row(
                "SELECT id,conversation_id,title,sources,hypotheses,decisions,experiments FROM research_sessions WHERE id=?1",
                [id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
            )
            .optional()
            .map_err(|_| Error::Storage)?;
        row.map(|row| research_from(&conn, row)).transpose()
    }

    /// Deletes a research session and its note revisions.
    pub fn delete_research_session(&self, id: Id) -> Result<bool> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        Ok(conn
            .execute(
                "DELETE FROM research_sessions WHERE id=?1",
                [id.to_string()],
            )
            .map_err(|_| Error::Storage)?
            != 0)
    }

    /// Atomically appends the next numbered note revision for a research session.
    pub fn append_research_note(
        &self,
        session_id: Id,
        text: &str,
        created_at: u64,
    ) -> Result<NoteRevision> {
        let mut conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let tx = conn.transaction().map_err(|_| Error::Storage)?;
        let revision: Option<u32> = tx
            .query_row(
                "SELECT MAX(revision) FROM research_notes WHERE session_id=?1",
                [session_id.to_string()],
                |row| row.get(0),
            )
            .map_err(|_| Error::Storage)?;
        let note = NoteRevision {
            revision: revision.unwrap_or(0).checked_add(1).ok_or(Error::Storage)?,
            text: text.to_owned(),
            created_at,
        };
        let inserted = tx
            .execute(
                "INSERT INTO research_notes(session_id,revision,text,created_at) \
                 SELECT ?1,?2,?3,?4 WHERE EXISTS(SELECT 1 FROM research_sessions WHERE id=?1)",
                params![
                    session_id.to_string(),
                    note.revision,
                    note.text,
                    timestamp(created_at)?
                ],
            )
            .map_err(|_| Error::Storage)?;
        if inserted == 0 {
            return Err(Error::InvalidInput);
        }
        tx.commit().map_err(|_| Error::Storage)?;
        Ok(note)
    }
}

type ResearchRow = (String, String, String, String, String, String, String);

fn insert_research_session(
    conn: &Connection,
    session: &ResearchSession,
    upsert: bool,
) -> Result<bool> {
    let conflict = if upsert {
        " ON CONFLICT(id) DO UPDATE SET conversation_id=excluded.conversation_id,title=excluded.title,sources=excluded.sources,hypotheses=excluded.hypotheses,decisions=excluded.decisions,experiments=excluded.experiments"
    } else {
        ""
    };
    let sql = format!(
        "INSERT INTO research_sessions(id,conversation_id,title,sources,hypotheses,decisions,experiments) \
         SELECT ?1,?2,?3,?4,?5,?6,?7 WHERE EXISTS(SELECT 1 FROM conversations WHERE id=?2){conflict}"
    );
    conn.execute(
        &sql,
        params![
            session.id.to_string(),
            session.conversation_id.to_string(),
            session.title,
            encode(&session.sources)?,
            encode(&session.hypotheses)?,
            encode(&session.decisions)?,
            encode(&session.experiments)?
        ],
    )
    .map(|count| count != 0)
    .map_err(|_| {
        if upsert {
            Error::Storage
        } else {
            Error::Conflict
        }
    })
}

fn insert_notes(conn: &Connection, session_id: Id, notes: &[NoteRevision]) -> Result<()> {
    for note in notes {
        conn.execute(
            "INSERT OR IGNORE INTO research_notes(session_id,revision,text,created_at) VALUES(?1,?2,?3,?4)",
            params![session_id.to_string(), note.revision, note.text, timestamp(note.created_at)?],
        )
        .map_err(|_| Error::Storage)?;
    }
    Ok(())
}

fn research_from(conn: &Connection, row: ResearchRow) -> Result<ResearchSession> {
    let (id, conversation_id, title, sources, hypotheses, decisions, experiments) = row;
    let parsed_id = parse_id(&id)?;
    let mut stmt = conn
        .prepare(
            "SELECT revision,text,created_at FROM (\
                 SELECT revision,text,created_at FROM research_notes \
                 WHERE session_id=?1 ORDER BY revision DESC LIMIT ?2\
             ) ORDER BY revision",
        )
        .map_err(|_| Error::Storage)?;
    let notes = stmt
        .query_map(params![id, read_limit(MAX_LIST_ITEMS)], |row| {
            Ok(NoteRevision {
                revision: row.get(0)?,
                text: row.get(1)?,
                created_at: row.get(2)?,
            })
        })
        .map_err(|_| Error::Storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| Error::Storage)?;
    Ok(ResearchSession {
        id: parsed_id,
        conversation_id: parse_id(&conversation_id)?,
        title,
        sources: serde_json::from_str::<Vec<SourceReference>>(&sources)
            .map_err(|_| Error::Storage)?,
        hypotheses: serde_json::from_str(&hypotheses).map_err(|_| Error::Storage)?,
        decisions: serde_json::from_str(&decisions).map_err(|_| Error::Storage)?,
        experiments: serde_json::from_str(&experiments).map_err(|_| Error::Storage)?,
        notes,
    })
}
