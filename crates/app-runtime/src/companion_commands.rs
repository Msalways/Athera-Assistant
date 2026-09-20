//! Explicit user-owned memory and research commands; no model can call this surface.
use super::{conversations::now, Runtime};
use assistant_contracts::{conversation::*, Error, Id, Result};
use serde_json::{json, Value};

pub(crate) fn id(payload: &Value, key: &str) -> Result<Id> {
    payload[key]
        .as_str()
        .and_then(|s| Id::parse_str(s).ok())
        .ok_or(Error::InvalidInput)
}
fn text(payload: &Value, key: &str, limit: usize) -> Result<String> {
    let value = payload[key].as_str().ok_or(Error::InvalidInput)?;
    if value.trim().is_empty() || value.len() > limit {
        return Err(Error::InvalidInput);
    }
    Ok(value.to_owned())
}

impl Runtime {
    pub(crate) fn companion_command(&self, name: &str, payload: Value) -> Result<Value> {
        match name {
            "send_message" => self
                .conversations
                .send(serde_json::from_value(payload).map_err(|_| Error::InvalidInput)?),
            "cancel_message" => self.conversations.cancel(id(&payload, "conversation_id")?),
            "list_conversations" => self.conversations.list(),
            "get_conversation" => self.conversations.get(id(&payload, "conversation_id")?),
            "delete_conversation" => self.conversations.delete(id(&payload, "conversation_id")?),
            "list_memories" => Ok(json!(self.store.list_memories(100)?)),
            "save_memory" => {
                if payload["confirmed"] != true {
                    return Err(Error::Denied);
                }
                let memory = PersonalMemory {
                    id: if payload.get("id").is_some() {
                        id(&payload, "id")?
                    } else {
                        Id::new_v4()
                    },
                    text: text(&payload, "text", 1000)?,
                    updated_at: now(),
                };
                self.store.upsert_memory(&memory)?;
                Ok(json!(memory))
            }
            "delete_memory" => Ok(json!(self.store.delete_memory(id(&payload, "id")?)?)),
            "save_summary" => {
                let id = id(&payload, "conversation_id")?;
                if self.conversations.busy() {
                    return Err(Error::Conflict);
                }
                let mut conversation = self
                    .store
                    .get_conversation(id)?
                    .ok_or(Error::InvalidInput)?;
                conversation.summary = text(&payload, "text", 2000)?;
                conversation.updated_at = now();
                self.store.upsert_conversation(&conversation)?;
                Ok(json!(conversation))
            }
            "create_research" => {
                let conversation_id = id(&payload, "conversation_id")?;
                let conversation = self
                    .store
                    .get_conversation(conversation_id)?
                    .ok_or(Error::InvalidInput)?;
                if conversation.temporary {
                    return Err(Error::Denied);
                }
                let session = ResearchSession {
                    id: Id::new_v4(),
                    conversation_id,
                    title: text(&payload, "title", 200)?,
                    sources: vec![],
                    hypotheses: vec![],
                    decisions: vec![],
                    experiments: vec![],
                    notes: vec![],
                };
                self.store.create_research_session(&session)?;
                Ok(json!(session))
            }
            "list_research" => Ok(json!(self
                .store
                .list_research_sessions(id(&payload, "conversation_id")?, 100)?)),
            "get_research" => Ok(json!(self
                .store
                .get_research_session(id(&payload, "id")?)?
                .ok_or(Error::InvalidInput)?)),
            "delete_research" => Ok(json!(self
                .store
                .delete_research_session(id(&payload, "id")?)?)),
            "append_research_note" => Ok(json!(self.store.append_research_note(
                id(&payload, "id")?,
                &text(&payload, "text", 16_000)?,
                now()
            )?)),
            // Shared sources are user-provided evidence, not proof that a URL was retrieved.
            "add_research_source" => {
                let mut session = self
                    .store
                    .get_research_session(id(&payload, "id")?)?
                    .ok_or(Error::InvalidInput)?;
                if session.sources.len() >= 50 {
                    return Err(Error::InvalidInput);
                }
                let url = match payload["url"].as_str() {
                    Some(url)
                        if url.len() <= 2000
                            && (url.starts_with("https://") || url.starts_with("http://")) =>
                    {
                        Some(url.to_owned())
                    }
                    Some(_) => return Err(Error::InvalidInput),
                    None => None,
                };
                session.sources.push(SourceReference {
                    id: Id::new_v4(),
                    title: text(&payload, "title", 200)?,
                    url,
                    excerpt: text(&payload, "excerpt", 8000)?,
                });
                self.store.upsert_research_session(&session)?;
                Ok(json!(session))
            }
            "update_research_reasoning" => {
                let mut session = self
                    .store
                    .get_research_session(id(&payload, "id")?)?
                    .ok_or(Error::InvalidInput)?;
                for (key, field) in [
                    ("hypotheses", &mut session.hypotheses),
                    ("decisions", &mut session.decisions),
                    ("experiments", &mut session.experiments),
                ] {
                    if let Some(value) = payload.get(key) {
                        let entries: Vec<String> = serde_json::from_value(value.clone())
                            .map_err(|_| Error::InvalidInput)?;
                        if entries.len() > 50
                            || entries
                                .iter()
                                .any(|s| s.len() > 2000 || s.trim().is_empty())
                        {
                            return Err(Error::InvalidInput);
                        }
                        *field = entries;
                    }
                }
                self.store.upsert_research_session(&session)?;
                Ok(json!(session))
            }
            _ => Err(Error::InvalidInput),
        }
    }
}
