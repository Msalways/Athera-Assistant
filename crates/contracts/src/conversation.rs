//! Conversation contracts are separate from action tasks and raw tool results.
use crate::{Id, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::{atomic::AtomicBool, Arc};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    Generating,
    Complete,
    Cancelled,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Id,
    pub title: String,
    pub temporary: bool,
    pub summary: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Id,
    pub conversation_id: Id,
    pub role: MessageRole,
    pub content: String,
    pub status: MessageStatus,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalMemory {
    pub id: Id,
    pub text: String,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceReference {
    pub id: Id,
    pub title: String,
    pub url: Option<String>,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteRevision {
    pub revision: u32,
    pub text: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchSession {
    pub id: Id,
    pub conversation_id: Id,
    pub title: String,
    pub sources: Vec<SourceReference>,
    pub hypotheses: Vec<String>,
    pub decisions: Vec<String>,
    pub experiments: Vec<String>,
    pub notes: Vec<NoteRevision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAvailability {
    MissingModel,
    Ready,
    Busy,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallationStatus {
    Missing,
    Downloading,
    Verifying,
    Installed,
    Failed,
    Cancelled,
}

/// A release must pin both artifact and runtime provenance before distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelManifest {
    pub id: String,
    pub url: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub revision: String,
    pub license: String,
    pub runtime_revision: String,
    pub context_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInstallation {
    pub manifest: ModelManifest,
    pub status: InstallationStatus,
    pub downloaded_bytes: u64,
    pub error: Option<String>,
}

/// Prompt content is data: providers must supply their own fixed system policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRequest {
    pub messages: Vec<Message>,
    pub summary: String,
    pub memories: Vec<PersonalMemory>,
    #[serde(default)]
    pub sources: Vec<SourceReference>,
    #[serde(default)]
    pub research_notes: Vec<NoteRevision>,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConversationEvent {
    Delta { text: String },
    Finished,
}

/// Cancellation is checked during prompt evaluation and token generation.
pub type Cancellation = Arc<AtomicBool>;
pub type ConversationSink = Arc<dyn Fn(ConversationEvent) + Send + Sync>;

#[async_trait]
pub trait ConversationProvider: Send + Sync {
    fn availability(&self) -> ProviderAvailability;
    async fn generate(
        &self,
        request: ConversationRequest,
        cancel: Cancellation,
        sink: ConversationSink,
    ) -> Result<()>;
    async fn unload(&self) -> Result<()>;
}
