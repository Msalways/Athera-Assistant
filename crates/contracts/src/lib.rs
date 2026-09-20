//! Vendor-neutral contracts. Credentials and provider SDK objects never belong here.
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;
pub mod conversation;
pub mod events;
pub mod graph;
pub mod output;
pub mod protocol;
pub mod tool_result;
pub use events::*;
pub use graph::*;
pub use output::*;
pub use tool_result::*;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Error {
    #[error("Invalid input")]
    InvalidInput,
    #[error("Capability unavailable")]
    Unavailable,
    #[error("Authentication required")]
    AuthRequired,
    #[error("Action denied")]
    Denied,
    #[error("Operation timed out")]
    Timeout,
    #[error("Provider response is invalid")]
    InvalidResponse,
    #[error("Rate limited")]
    RateLimited,
    #[error("Storage failure")]
    Storage,
    #[error("External action outcome is unknown")]
    OutcomeUnknown,
    #[error("Task state conflict")]
    Conflict,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Fast,
    Planner,
    Reasoner,
    Responder,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InputSource {
    Text,
    Voice,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInput {
    pub conversation_id: Uuid,
    pub text: String,
    pub source: InputSource,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Created,
    Running,
    WaitingForAuth,
    WaitingForApproval,
    WaitingForUser,
    WaitingForResolution,
    Completed,
    Failed,
    Cancelled,
}
impl TaskStatus {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    ReadOnly,
    LocalSafeWrite,
    ExternalWrite,
    Sensitive,
    Destructive,
}
impl Risk {
    pub fn requires_approval(self) -> bool {
        !matches!(self, Self::ReadOnly | Self::LocalSafeWrite)
    }
    pub fn retry_safe(self) -> bool {
        matches!(self, Self::ReadOnly)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolSpec {
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(default)]
    pub output_schema: Option<Value>,
    pub connection_id: String,
    pub source_tool: String,
    pub risk: Risk,
    pub enabled: bool,
    pub requires_auth: bool,
    pub requires_network: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillSpec {
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub tool_requirements: Vec<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "spec", rename_all = "snake_case")]
pub enum Capability {
    Tool(ToolSpec),
    Skill(SkillSpec),
}
impl Capability {
    pub fn id(&self) -> &str {
        match self {
            Self::Tool(v) => &v.id,
            Self::Skill(v) => &v.id,
        }
    }
    pub fn enabled(&self) -> bool {
        match self {
            Self::Tool(v) => v.enabled,
            Self::Skill(v) => v.enabled,
        }
    }
    pub fn description(&self) -> &str {
        match self {
            Self::Tool(v) => &v.description,
            Self::Skill(v) => &v.description,
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Self::Tool(v) => &v.name,
            Self::Skill(v) => &v.name,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub kind: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub tool_id: String,
    pub version: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handoff {
    pub task_id: Uuid,
    pub plan_revision: u32,
    pub objective: String,
    pub reason: String,
    pub completed_steps: u32,
    pub result_refs: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentAction {
    CallTool {
        call: ToolCall,
    },
    Search {
        query: String,
    },
    ActivateSkill {
        skill_id: String,
    },
    Plan {
        steps: Vec<String>,
    },
    Handoff {
        role: Role,
        objective: String,
        reason: String,
    },
    AskUser {
        question: String,
    },
    Respond {
        text: String,
    },
    RespondStructured {
        output: AssistantOutput,
    },
    Fail {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingAction {
    pub id: Uuid,
    pub call: ToolCall,
    pub spec: ToolSpec,
    pub approved: bool,
    pub started: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub input: UserInput,
    pub status: TaskStatus,
    pub role: Role,
    pub plan: Vec<String>,
    pub plan_revision: u32,
    pub step: u32,
    pub search_query: String,
    pub active_skills: Vec<SkillSpec>,
    pub result_refs: Vec<Uuid>,
    pub pending: Option<PendingAction>,
    pub handoff: Option<Handoff>,
    pub message: String,
    #[serde(default)]
    pub output: Option<AssistantOutput>,
    #[serde(default)]
    pub work_graph: Option<WorkGraph>,
    pub failures: u32,
    pub call_counts: BTreeMap<String, u32>,
}
impl Task {
    pub fn new(input: UserInput) -> Self {
        Self {
            id: Uuid::new_v4(),
            search_query: input.text.clone(),
            input,
            status: TaskStatus::Created,
            role: Role::Fast,
            plan: vec![],
            plan_revision: 0,
            step: 0,
            active_skills: vec![],
            result_refs: vec![],
            pending: None,
            handoff: None,
            message: String::new(),
            output: None,
            work_graph: None,
            failures: 0,
            call_counts: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultExcerpt {
    pub id: Uuid,
    pub untrusted_data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBundle {
    pub task_id: Uuid,
    pub role: Role,
    pub goal: String,
    pub plan: Vec<String>,
    pub handoff: Option<Handoff>,
    pub history: Vec<String>,
    pub results: Vec<ResultExcerpt>,
    pub skills: Vec<SkillSpec>,
    pub candidates: Vec<Candidate>,
    pub tools: Vec<ToolSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub tool_calls: bool,
    pub planning: bool,
    pub local: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderEvent {
    TextDelta { text: String },
}

pub type ProviderEventSink = std::sync::Arc<dyn Fn(ProviderEvent) -> Result<()> + Send + Sync>;

#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn id(&self) -> &str;
    fn capabilities(&self) -> ModelCapabilities;
    async fn infer(&self, context: ContextBundle) -> Result<AgentAction>;
    async fn infer_stream(
        &self,
        context: ContextBundle,
        _sink: ProviderEventSink,
    ) -> Result<AgentAction> {
        self.infer(context).await
    }
}

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, spec: &ToolSpec, call: &ToolCall) -> Result<Value>;
}

/// One transaction persists the task snapshot and its monotonically ordered event.
pub trait Store: Send + Sync {
    fn save_task(&self, task: &Task) -> Result<()>;
    fn task(&self, id: Uuid) -> Result<Task>;
    fn tasks(&self) -> Result<Vec<Task>>;
    fn save_result(&self, id: Uuid, value: &Value) -> Result<()>;
    fn result(&self, id: Uuid) -> Result<Value>;
    fn put_capability(&self, spec: &Capability) -> Result<()>;
    fn capability(&self, id: &str) -> Result<Capability>;
    fn capabilities(&self) -> Result<Vec<Capability>>;
    fn search(&self, query: &str, limit: usize) -> Result<Vec<Candidate>>;
    fn events(&self, after: u64) -> Result<Vec<AssistantEvent>>;
    fn append_event(&self, event: &NewRunEvent) -> Result<()>;
    fn event_bounds(&self) -> Result<Option<EventBounds>>;
    fn setting(&self, key: &str) -> Result<Option<Value>>;
    fn set_setting(&self, key: &str, value: &Value) -> Result<()>;
}

pub type AssistantEvent = RunEventEnvelope;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub max_steps: u32,
    pub max_failures: u32,
    pub max_identical_calls: u32,
    pub timeout_seconds: u64,
    pub candidate_limit: usize,
    pub tool_limit: usize,
    pub fast_context_bytes: usize,
    pub cloud_context_bytes: usize,
}
impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_steps: 24,
            max_failures: 3,
            max_identical_calls: 2,
            timeout_seconds: 30,
            candidate_limit: 20,
            tool_limit: 8,
            fast_context_bytes: 16_000,
            cloud_context_bytes: 48_000,
        }
    }
}

pub use uuid::Uuid as Id;
