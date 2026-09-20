//! Local conversation lifecycle. It never dispatches tools or silently routes to cloud.
use assistant_contracts::{conversation::*, Error, Id, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use storage_sqlite::SqliteStore;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SendMessage {
    conversation_id: Id,
    text: String,
    #[serde(default)]
    temporary: bool,
}

#[derive(Default)]
struct State {
    temporary: BTreeMap<Id, (Conversation, Vec<Message>)>,
    running: BTreeMap<Id, (Cancellation, Message)>,
}

pub(crate) struct Conversations {
    store: Arc<SqliteStore>,
    provider: Arc<dyn ConversationProvider>,
    state: Mutex<State>,
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl Conversations {
    pub fn new(store: Arc<SqliteStore>, provider: Arc<dyn ConversationProvider>) -> Arc<Self> {
        Arc::new(Self {
            store,
            provider,
            state: Mutex::new(State::default()),
        })
    }

    pub fn busy(&self) -> bool {
        self.state
            .lock()
            .map_or(true, |state| !state.running.is_empty())
    }

    pub fn list(&self) -> Result<Value> {
        // Temporary conversations are visible only for this runtime session.
        let state = self.state.lock().map_err(|_| Error::Storage)?;
        let mut conversations = self.store.list_conversations(100)?;
        conversations.extend(state.temporary.values().map(|(c, _)| c.clone()));
        conversations.sort_by_key(|c| std::cmp::Reverse(c.updated_at));
        Ok(json!(conversations))
    }

    pub fn get(&self, id: Id) -> Result<Value> {
        let state = self.state.lock().map_err(|_| Error::Storage)?;
        let (conversation, messages) = self.read(&state, id)?;
        Ok(json!({"conversation":conversation,"messages":messages}))
    }

    fn read(&self, state: &State, id: Id) -> Result<(Conversation, Vec<Message>)> {
        if let Some(value) = state.temporary.get(&id) {
            return Ok(value.clone());
        }
        let conversation = self
            .store
            .get_conversation(id)?
            .ok_or(Error::InvalidInput)?;
        Ok((conversation, self.store.list_messages(id, 200)?))
    }

    pub fn delete(&self, id: Id) -> Result<Value> {
        let mut state = self.state.lock().map_err(|_| Error::Storage)?;
        if state.running.contains_key(&id) {
            return Err(Error::Conflict);
        }
        let removed =
            state.temporary.remove(&id).is_some() || self.store.delete_conversation(id)?;
        Ok(json!(removed))
    }

    pub fn cancel(&self, id: Id) -> Result<Value> {
        let mut state = self.state.lock().map_err(|_| Error::Storage)?;
        if let Some((cancel, message)) = state.running.get_mut(&id) {
            cancel.store(true, Ordering::Release);
            message.status = MessageStatus::Cancelled;
            let message = message.clone();
            self.write_message(&mut state, &message)?;
        }
        Ok(Value::Null)
    }

    pub fn cancel_all(&self) -> Result<()> {
        let ids: Vec<_> = self
            .state
            .lock()
            .map_err(|_| Error::Storage)?
            .running
            .keys()
            .copied()
            .collect();
        for id in ids {
            self.cancel(id)?;
        }
        Ok(())
    }

    fn write_message(&self, state: &mut State, message: &Message) -> Result<()> {
        if let Some((_, messages)) = state.temporary.get_mut(&message.conversation_id) {
            let old = messages
                .iter_mut()
                .find(|m| m.id == message.id)
                .ok_or(Error::Conflict)?;
            *old = message.clone();
            Ok(())
        } else if self.store.update_message(message)? {
            Ok(())
        } else {
            Err(Error::Conflict)
        }
    }

    pub fn send(self: &Arc<Self>, input: SendMessage) -> Result<Value> {
        if input.text.trim().is_empty() || input.text.len() > 8000 {
            return Err(Error::InvalidInput);
        }
        let mut state = self.state.lock().map_err(|_| Error::Storage)?;
        if !state.running.is_empty() {
            return Err(Error::Conflict);
        }
        if self.provider.availability() != ProviderAvailability::Ready {
            return Err(Error::Unavailable);
        }
        let id = input.conversation_id;
        let existing = if let Some((c, _)) = state.temporary.get(&id) {
            Some(c.clone())
        } else {
            self.store.get_conversation(id)?
        };
        let mut conversation = existing.unwrap_or(Conversation {
            id,
            title: input.text.chars().take(80).collect(),
            temporary: input.temporary,
            summary: String::new(),
            updated_at: now(),
        });
        if conversation.temporary != input.temporary {
            return Err(Error::Conflict);
        }
        conversation.updated_at = now();
        if input.temporary {
            let entry = state
                .temporary
                .entry(id)
                .or_insert_with(|| (conversation.clone(), vec![]));
            entry.0 = conversation;
        } else {
            self.store.upsert_conversation(&conversation)?;
        }
        let user = Message {
            id: Id::new_v4(),
            conversation_id: id,
            role: MessageRole::User,
            content: input.text,
            status: MessageStatus::Complete,
            created_at: now(),
        };
        let assistant = Message {
            id: Id::new_v4(),
            conversation_id: id,
            role: MessageRole::Assistant,
            content: String::new(),
            status: MessageStatus::Generating,
            created_at: now(),
        };
        if let Some((_, messages)) = state.temporary.get_mut(&id) {
            messages.extend([user, assistant.clone()]);
            // Keep temporary session memory bounded as well as the inference packet.
            if messages.len() > 200 {
                messages.drain(..messages.len() - 200);
            }
        } else {
            self.store.append_message(&user)?;
            self.store.append_message(&assistant)?;
        }
        let request = (|| -> Result<ConversationRequest> {
            let (conversation, messages) = self.read(&state, id)?;
            let memories = if input.temporary {
                vec![]
            } else {
                self.store.list_memories(100)?
            };
            let mut request = bounded_context(&conversation, messages, memories);
            if !input.temporary {
                add_research_context(&mut request, self.store.list_research_sessions(id, 3)?);
            }
            Ok(request)
        })();
        let request = match request {
            Ok(request) => request,
            Err(error) => {
                let mut failed = assistant;
                failed.status = MessageStatus::Failed;
                self.write_message(&mut state, &failed)?;
                return Err(error);
            }
        };
        let cancel = Arc::new(AtomicBool::new(false));
        state
            .running
            .insert(id, (cancel.clone(), assistant.clone()));
        drop(state);
        let this = self.clone();
        tokio::spawn(async move {
            let callback = this.clone();
            let sink: ConversationSink = Arc::new(move |event| {
                if let ConversationEvent::Delta { text } = event {
                    if let Ok(mut state) = callback.state.lock() {
                        if let Some((cancel, message)) = state.running.get_mut(&id) {
                            if cancel.load(Ordering::Acquire) {
                                return;
                            }
                            if message.content.len() + text.len() > 32_000 {
                                cancel.store(true, Ordering::Release);
                                return;
                            }
                            message.content.push_str(&text);
                            let message = message.clone();
                            if callback.write_message(&mut state, &message).is_err() {
                                if let Some((cancel, _)) = state.running.get(&id) {
                                    cancel.store(true, Ordering::Release);
                                }
                            }
                        }
                    }
                }
            });
            let result = this.provider.generate(request, cancel.clone(), sink).await;
            if let Ok(mut state) = this.state.lock() {
                if let Some((_, mut message)) = state.running.remove(&id) {
                    message.status = if cancel.load(Ordering::Acquire) {
                        MessageStatus::Cancelled
                    } else if result.is_ok() && !message.content.trim().is_empty() {
                        MessageStatus::Complete
                    } else {
                        MessageStatus::Failed
                    };
                    if message.status == MessageStatus::Failed && message.content.is_empty() {
                        message.content =
                            "Local response failed. Check the model setup and retry.".into();
                    }
                    let _ = this.write_message(&mut state, &message);
                }
            }
        });
        Ok(json!(assistant))
    }
}

/// Conservatively bound UTF-8 bytes; the native adapter enforces the exact token budget.
fn bounded_context(
    conversation: &Conversation,
    messages: Vec<Message>,
    memories: Vec<PersonalMemory>,
) -> ConversationRequest {
    let latest = messages
        .iter()
        .rev()
        .find(|m| m.role == MessageRole::User)
        .map(|m| m.content.to_lowercase())
        .unwrap_or_default();
    let words: Vec<_> = latest.split_whitespace().filter(|w| w.len() > 2).collect();
    let mut remaining = 10_000usize;
    let mut selected = Vec::new();
    for message in messages
        .into_iter()
        .rev()
        .filter(|m| m.status == MessageStatus::Complete)
    {
        if message.content.len() > remaining {
            break;
        }
        remaining -= message.content.len();
        selected.push(message);
        if selected.len() == 20 {
            break;
        }
    }
    selected.reverse();
    let summary: String = conversation
        .summary
        .chars()
        .take(remaining.min(500) / 4)
        .collect();
    remaining = remaining.saturating_sub(summary.len());
    let memories = memories
        .into_iter()
        .filter(|m| words.iter().any(|w| m.text.to_lowercase().contains(w)))
        .filter(|m| {
            if m.text.len() > remaining {
                return false;
            }
            remaining -= m.text.len();
            true
        })
        .take(4)
        .collect();
    ConversationRequest {
        messages: selected,
        summary,
        memories,
        sources: vec![],
        research_notes: vec![],
        max_output_tokens: 512,
    }
}

fn add_research_context(request: &mut ConversationRequest, sessions: Vec<ResearchSession>) {
    let used = request
        .messages
        .iter()
        .map(|m| m.content.len())
        .sum::<usize>()
        + request.summary.len()
        + request.memories.iter().map(|m| m.text.len()).sum::<usize>();
    let mut remaining = 10_000usize.saturating_sub(used);
    for session in sessions {
        for mut source in session.sources.into_iter().take(3) {
            let overhead = source.title.len() + source.url.as_ref().map_or(0, String::len) + 100;
            if remaining <= overhead {
                continue;
            }
            source.excerpt = clip_utf8(&source.excerpt, (remaining - overhead).min(1500));
            remaining -= overhead + source.excerpt.len();
            request.sources.push(source);
        }
        if let Some(note) = session.notes.last() {
            if remaining <= 50 {
                continue;
            }
            let mut note = note.clone();
            note.text = clip_utf8(&note.text, (remaining - 50).min(1000));
            remaining -= note.text.len() + 50;
            request.research_notes.push(note);
        }
    }
}

fn clip_utf8(text: &str, limit: usize) -> String {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn context_keeps_latest_input_before_optional_memory() {
        let id = Id::new_v4();
        let conversation = Conversation {
            id,
            title: "test".into(),
            temporary: false,
            summary: "summary".repeat(1000),
            updated_at: 0,
        };
        let message = Message {
            id: Id::new_v4(),
            conversation_id: id,
            role: MessageRole::User,
            content: "hello ".repeat(1300),
            status: MessageStatus::Complete,
            created_at: 0,
        };
        let memories = (0..10)
            .map(|_| PersonalMemory {
                id: Id::new_v4(),
                text: "hello ".repeat(166),
                updated_at: 0,
            })
            .collect();
        let packet = bounded_context(&conversation, vec![message.clone()], memories);
        assert_eq!(packet.messages[0].content, message.content);
        assert!(
            packet
                .messages
                .iter()
                .map(|m| m.content.len())
                .sum::<usize>()
                + packet.summary.len()
                + packet.memories.iter().map(|m| m.text.len()).sum::<usize>()
                <= 10_000
        );
    }
}
