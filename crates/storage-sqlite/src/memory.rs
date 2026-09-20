use crate::{parse_id, read_limit, timestamp, SqliteStore};
use assistant_contracts::{conversation::PersonalMemory, Error, Id, Result};
use rusqlite::{params, OptionalExtension};

impl SqliteStore {
    /// Inserts an explicit personal memory.
    pub fn create_memory(&self, memory: &PersonalMemory) -> Result<()> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.execute(
            "INSERT INTO personal_memories(id,text,updated_at) VALUES(?1,?2,?3)",
            params![
                memory.id.to_string(),
                memory.text,
                timestamp(memory.updated_at)?
            ],
        )
        .map_err(|_| Error::Conflict)?;
        Ok(())
    }

    /// Inserts or replaces an explicit personal memory.
    pub fn upsert_memory(&self, memory: &PersonalMemory) -> Result<()> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.execute(
            "INSERT INTO personal_memories(id,text,updated_at) VALUES(?1,?2,?3) \
             ON CONFLICT(id) DO UPDATE SET text=excluded.text,updated_at=excluded.updated_at",
            params![
                memory.id.to_string(),
                memory.text,
                timestamp(memory.updated_at)?
            ],
        )
        .map_err(|_| Error::Storage)?;
        Ok(())
    }

    /// Lists the most recently updated explicit memories, capped at 500 rows.
    pub fn list_memories(&self, limit: usize) -> Result<Vec<PersonalMemory>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt = conn
            .prepare("SELECT id,text,updated_at FROM personal_memories ORDER BY updated_at DESC,rowid DESC LIMIT ?1")
            .map_err(|_| Error::Storage)?;
        let rows = stmt
            .query_map([read_limit(limit)], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|_| Error::Storage)?;
        rows.map(|row| memory_from(row.map_err(|_| Error::Storage)?))
            .collect()
    }

    /// Gets an explicit personal memory by ID.
    pub fn get_memory(&self, id: Id) -> Result<Option<PersonalMemory>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let row = conn
            .query_row(
                "SELECT id,text,updated_at FROM personal_memories WHERE id=?1",
                [id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|_| Error::Storage)?;
        row.map(memory_from).transpose()
    }

    /// Deletes an explicit personal memory by ID.
    pub fn delete_memory(&self, id: Id) -> Result<bool> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        Ok(conn
            .execute(
                "DELETE FROM personal_memories WHERE id=?1",
                [id.to_string()],
            )
            .map_err(|_| Error::Storage)?
            != 0)
    }
}

fn memory_from((id, text, updated_at): (String, String, u64)) -> Result<PersonalMemory> {
    Ok(PersonalMemory {
        id: parse_id(&id)?,
        text,
        updated_at,
    })
}
