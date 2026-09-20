use crate::{parse_id, read_limit, timestamp, SqliteStore};
use assistant_contracts::{
    conversation::{Conversation, Message, MessageRole, MessageStatus},
    Error, Id, Result,
};
use rusqlite::{params, OptionalExtension};

impl SqliteStore {
    /// Inserts a persistent conversation. Temporary conversations are rejected.
    pub fn create_conversation(&self, conversation: &Conversation) -> Result<()> {
        persistable(conversation)?;
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.execute(
            "INSERT INTO conversations(id,title,summary,updated_at) VALUES(?1,?2,?3,?4)",
            params![
                conversation.id.to_string(),
                conversation.title,
                conversation.summary,
                timestamp(conversation.updated_at)?
            ],
        )
        .map_err(|_| Error::Conflict)?;
        Ok(())
    }

    /// Inserts or replaces the mutable fields of a persistent conversation.
    pub fn upsert_conversation(&self, conversation: &Conversation) -> Result<()> {
        persistable(conversation)?;
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        conn.execute(
            "INSERT INTO conversations(id,title,summary,updated_at) VALUES(?1,?2,?3,?4) \
             ON CONFLICT(id) DO UPDATE SET title=excluded.title,summary=excluded.summary,updated_at=excluded.updated_at",
            params![
                conversation.id.to_string(),
                conversation.title,
                conversation.summary,
                timestamp(conversation.updated_at)?
            ],
        )
        .map_err(|_| Error::Storage)?;
        Ok(())
    }

    /// Lists the most recently updated conversations, capped at 500 rows.
    pub fn list_conversations(&self, limit: usize) -> Result<Vec<Conversation>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt = conn
            .prepare("SELECT id,title,summary,updated_at FROM conversations ORDER BY updated_at DESC,rowid DESC LIMIT ?1")
            .map_err(|_| Error::Storage)?;
        let rows = stmt
            .query_map([read_limit(limit)], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .map_err(|_| Error::Storage)?;
        rows.map(|row| conversation_from(row.map_err(|_| Error::Storage)?))
            .collect()
    }

    /// Gets a persistent conversation by ID.
    pub fn get_conversation(&self, id: Id) -> Result<Option<Conversation>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let row = conn
            .query_row(
                "SELECT id,title,summary,updated_at FROM conversations WHERE id=?1",
                [id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(|_| Error::Storage)?;
        row.map(conversation_from).transpose()
    }

    /// Deletes a conversation and its messages and research sessions.
    pub fn delete_conversation(&self, id: Id) -> Result<bool> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        Ok(conn
            .execute("DELETE FROM conversations WHERE id=?1", [id.to_string()])
            .map_err(|_| Error::Storage)?
            != 0)
    }

    /// Appends a message if its conversation exists.
    pub fn append_message(&self, message: &Message) -> Result<()> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let inserted = conn
            .execute(
                "INSERT INTO messages(id,conversation_id,role,content,status,created_at) \
                 SELECT ?1,?2,?3,?4,?5,?6 WHERE EXISTS(SELECT 1 FROM conversations WHERE id=?2)",
                params![
                    message.id.to_string(),
                    message.conversation_id.to_string(),
                    role_name(&message.role),
                    message.content,
                    status_name(&message.status),
                    timestamp(message.created_at)?
                ],
            )
            .map_err(|_| Error::Conflict)?;
        if inserted == 0 {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }

    /// Updates a message only when both its ID and conversation ID match.
    pub fn update_message(&self, message: &Message) -> Result<bool> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        Ok(conn
            .execute(
                "UPDATE messages SET role=?3,content=?4,status=?5,created_at=?6 WHERE id=?1 AND conversation_id=?2",
                params![
                    message.id.to_string(),
                    message.conversation_id.to_string(),
                    role_name(&message.role),
                    message.content,
                    status_name(&message.status),
                    timestamp(message.created_at)?
                ],
            )
            .map_err(|_| Error::Storage)?
            != 0)
    }

    /// Lists the latest messages in insertion order, capped at 500 rows.
    pub fn list_messages(&self, conversation_id: Id, limit: usize) -> Result<Vec<Message>> {
        let conn = self.connection.lock().map_err(|_| Error::Storage)?;
        let mut stmt = conn
            .prepare(
                "SELECT id,conversation_id,role,content,status,created_at FROM (\
                     SELECT id,conversation_id,role,content,status,created_at,sequence FROM messages \
                     WHERE conversation_id=?1 ORDER BY sequence DESC LIMIT ?2\
                 ) ORDER BY sequence",
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
                    ))
                },
            )
            .map_err(|_| Error::Storage)?;
        rows.map(|row| message_from(row.map_err(|_| Error::Storage)?))
            .collect()
    }
}

type ConversationRow = (String, String, String, u64);
type MessageRow = (String, String, String, String, String, u64);

fn persistable(conversation: &Conversation) -> Result<()> {
    if conversation.temporary {
        Err(Error::InvalidInput)
    } else {
        Ok(())
    }
}

fn conversation_from((id, title, summary, updated_at): ConversationRow) -> Result<Conversation> {
    Ok(Conversation {
        id: parse_id(&id)?,
        title,
        temporary: false,
        summary,
        updated_at,
    })
}

fn role_name(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
    }
}

fn status_name(status: &MessageStatus) -> &'static str {
    match status {
        MessageStatus::Generating => "generating",
        MessageStatus::Complete => "complete",
        MessageStatus::Cancelled => "cancelled",
        MessageStatus::Failed => "failed",
        MessageStatus::Interrupted => "interrupted",
    }
}

fn message_from(
    (id, conversation_id, role, content, status, created_at): MessageRow,
) -> Result<Message> {
    let role = match role.as_str() {
        "user" => MessageRole::User,
        "assistant" => MessageRole::Assistant,
        _ => return Err(Error::Storage),
    };
    let status = match status.as_str() {
        "generating" => MessageStatus::Generating,
        "complete" => MessageStatus::Complete,
        "cancelled" => MessageStatus::Cancelled,
        "failed" => MessageStatus::Failed,
        "interrupted" => MessageStatus::Interrupted,
        _ => return Err(Error::Storage),
    };
    Ok(Message {
        id: parse_id(&id)?,
        conversation_id: parse_id(&conversation_id)?,
        role,
        content,
        status,
        created_at,
    })
}
