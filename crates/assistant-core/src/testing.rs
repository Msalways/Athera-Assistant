//! Deterministic fixtures for testing the real core. Never presented as model inference.
use assistant_contracts::*;
use async_trait::async_trait;
use std::{collections::VecDeque, sync::Mutex};

pub struct ScriptedProvider {
    pub actions: Mutex<VecDeque<Result<AgentAction>>>,
    pub events: Mutex<VecDeque<Vec<ProviderEvent>>>,
    pub local: bool,
}
impl ScriptedProvider {
    pub fn new(local: bool, actions: Vec<AgentAction>) -> Self {
        Self {
            local,
            actions: Mutex::new(actions.into_iter().map(Ok).collect()),
            events: Mutex::new(VecDeque::new()),
        }
    }

    pub fn with_events(
        local: bool,
        actions: Vec<AgentAction>,
        events: Vec<Vec<ProviderEvent>>,
    ) -> Self {
        Self {
            local,
            actions: Mutex::new(actions.into_iter().map(Ok).collect()),
            events: Mutex::new(events.into()),
        }
    }

    fn next_action(&self) -> Result<AgentAction> {
        self.actions
            .lock()
            .map_err(|_| Error::Unavailable)?
            .pop_front()
            .unwrap_or(Err(Error::Unavailable))
    }
}
#[async_trait]
impl ModelProvider for ScriptedProvider {
    fn id(&self) -> &str {
        "scripted-test-provider"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            tool_calls: true,
            planning: true,
            local: self.local,
        }
    }
    async fn infer(&self, _: ContextBundle) -> Result<AgentAction> {
        self.next_action()
    }
    async fn infer_stream(&self, _: ContextBundle, sink: ProviderEventSink) -> Result<AgentAction> {
        for event in self
            .events
            .lock()
            .map_err(|_| Error::Unavailable)?
            .pop_front()
            .unwrap_or_default()
        {
            sink(event)?;
        }
        self.next_action()
    }
}

pub struct EchoExecutor;
#[async_trait]
impl ToolExecutor for EchoExecutor {
    async fn execute(&self, _: &ToolSpec, call: &ToolCall) -> Result<serde_json::Value> {
        Ok(call.arguments.clone())
    }
}

pub struct ScriptedExecutor {
    results: Mutex<VecDeque<Result<serde_json::Value>>>,
}

impl ScriptedExecutor {
    pub fn new(results: Vec<serde_json::Value>) -> Self {
        Self {
            results: Mutex::new(results.into_iter().map(Ok).collect()),
        }
    }
}

#[async_trait]
impl ToolExecutor for ScriptedExecutor {
    async fn execute(&self, _: &ToolSpec, _: &ToolCall) -> Result<serde_json::Value> {
        self.results
            .lock()
            .map_err(|_| Error::Unavailable)?
            .pop_front()
            .unwrap_or(Err(Error::Unavailable))
    }
}

pub fn example_tool(risk: Risk) -> ToolSpec {
    ToolSpec {
        id: "fixture.echo".into(),
        version: "1".into(),
        name: "Echo".into(),
        description: "echo a message".into(),
        input_schema: serde_json::json!({"type":"object","properties":{"text":{"type":"string"}},"required":["text"],"additionalProperties":false}),
        output_schema: None,
        connection_id: "fixture".into(),
        source_tool: "echo".into(),
        risk,
        enabled: true,
        requires_auth: false,
        requires_network: false,
    }
}
