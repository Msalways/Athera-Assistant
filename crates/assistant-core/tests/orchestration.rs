use assistant_contracts::*;
use assistant_core::{
    registry,
    testing::{example_tool, EchoExecutor, ScriptedProvider},
    Assistant,
};
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use storage_sqlite::SqliteStore;

fn input(source: InputSource) -> UserInput {
    UserInput {
        conversation_id: Id::new_v4(),
        text: "echo message".into(),
        source,
    }
}
fn call() -> AgentAction {
    AgentAction::CallTool {
        call: ToolCall {
            tool_id: "fixture.echo".into(),
            version: "1".into(),
            arguments: json!({"text":"hello"}),
        },
    }
}
fn respond() -> AgentAction {
    AgentAction::Respond {
        text: "Done".into(),
    }
}

fn structured_response() -> AgentAction {
    AgentAction::RespondStructured {
        output: AssistantOutput {
            schema: OUTPUT_SCHEMA_V1.into(),
            blocks: vec![OutputBlock::Table {
                id: "prices".into(),
                columns: vec!["Company".into(), "Price".into()],
                rows: vec![vec!["Example".into(), "10".into()]],
            }],
        },
    }
}

struct StreamingProvider;

#[async_trait::async_trait]
impl ModelProvider for StreamingProvider {
    fn id(&self) -> &str {
        "streaming-fixture"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            tool_calls: true,
            planning: true,
            local: false,
        }
    }

    async fn infer(&self, _: ContextBundle) -> Result<AgentAction> {
        panic!("the engine must use the streaming provider entry point")
    }

    async fn infer_stream(&self, _: ContextBundle, sink: ProviderEventSink) -> Result<AgentAction> {
        sink(ProviderEvent::TextDelta {
            text: "Hello ".into(),
        })?;
        sink(ProviderEvent::TextDelta {
            text: "world".into(),
        })?;
        Ok(AgentAction::Respond {
            text: "Hello world".into(),
        })
    }
}

struct LateDeltaProvider;

#[async_trait::async_trait]
impl ModelProvider for LateDeltaProvider {
    fn id(&self) -> &str {
        "late-delta-fixture"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            tool_calls: true,
            planning: true,
            local: false,
        }
    }

    async fn infer(&self, _: ContextBundle) -> Result<AgentAction> {
        unreachable!()
    }

    async fn infer_stream(&self, _: ContextBundle, sink: ProviderEventSink) -> Result<AgentAction> {
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            sink(ProviderEvent::TextDelta {
                text: "late".into(),
            })
            .unwrap();
        });
        Ok(respond())
    }
}

#[tokio::test]
async fn provider_deltas_and_worker_lifecycle_are_ordered_before_the_terminal_event() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let provider = Arc::new(StreamingProvider);
    let assistant = Assistant::new(
        store.clone(),
        provider.clone(),
        provider,
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    );
    let task = assistant.submit(input(InputSource::Text)).unwrap();
    let completed = assistant.run(task.id).await.unwrap();
    assert_eq!(completed.status, TaskStatus::Completed);

    let events = store.events(0).unwrap();
    let kinds: Vec<_> = events.iter().map(|event| event.kind).collect();
    assert_eq!(
        kinds,
        [
            RunEventKind::RunStarted,
            RunEventKind::TaskState,
            RunEventKind::WorkerStarted,
            RunEventKind::TextDelta,
            RunEventKind::TextDelta,
            RunEventKind::WorkerTerminal,
            RunEventKind::OutputUpsert,
            RunEventKind::RunTerminal,
        ]
    );
    assert_eq!(
        events
            .iter()
            .filter_map(|event| event.text_delta.as_deref())
            .collect::<String>(),
        "Hello world"
    );
    let worker = events[2].worker_id;
    assert!(worker.is_some());
    assert!(events[2..=5].iter().all(|event| event.worker_id == worker));
}

/// Announces which provider is answering, then streams text. This is what a
/// failover chain does, so the turn can be attributed to the vendor that
/// actually produced it.
struct AttributingProvider;

#[async_trait::async_trait]
impl ModelProvider for AttributingProvider {
    fn id(&self) -> &str {
        "attributing-fixture"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            tool_calls: true,
            planning: true,
            local: false,
        }
    }

    async fn infer(&self, _: ContextBundle) -> Result<AgentAction> {
        unreachable!()
    }

    async fn infer_stream(&self, _: ContextBundle, sink: ProviderEventSink) -> Result<AgentAction> {
        sink(ProviderEvent::ProviderSelected {
            provider_id: "fallback-vendor".into(),
        })?;
        sink(ProviderEvent::TextDelta {
            text: "Answered".into(),
        })?;
        Ok(respond())
    }
}

#[tokio::test]
async fn a_turn_records_the_provider_that_answered_it() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let provider = Arc::new(AttributingProvider);
    let assistant = Assistant::new(
        store.clone(),
        provider.clone(),
        provider,
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    );
    let task = assistant.submit(input(InputSource::Text)).unwrap();
    let completed = assistant.run(task.id).await.unwrap();
    assert_eq!(completed.status, TaskStatus::Completed);
    // Attribution lives on the task, not in a process-wide "last answered"
    // value, so concurrent turns cannot misreport each other.
    assert_eq!(
        completed.answered_by.as_deref(),
        Some("fallback-vendor"),
        "the answering provider must be recorded on the turn"
    );
    assert_eq!(
        store.task(task.id).unwrap().answered_by.as_deref(),
        Some("fallback-vendor"),
        "attribution must persist"
    );
}

#[tokio::test]
async fn a_turn_without_a_provider_selection_is_left_unattributed() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let provider = Arc::new(StreamingProvider);
    let assistant = Assistant::new(
        store.clone(),
        provider.clone(),
        provider,
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    );
    let task = assistant.submit(input(InputSource::Text)).unwrap();
    let completed = assistant.run(task.id).await.unwrap();
    // A single provider that never announces itself must not be given a label.
    assert_eq!(completed.answered_by, None);
}

#[tokio::test]
async fn deltas_arriving_after_a_worker_finishes_are_discarded() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let provider = Arc::new(LateDeltaProvider);
    let assistant = Assistant::new(
        store.clone(),
        provider.clone(),
        provider,
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    );
    let task = assistant.submit(input(InputSource::Text)).unwrap();
    assistant.run(task.id).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    let events = store.events(0).unwrap();
    assert!(!events
        .iter()
        .any(|event| event.kind == RunEventKind::TextDelta));
    assert_eq!(events.last().unwrap().kind, RunEventKind::RunTerminal);
}

#[tokio::test]
async fn structured_output_is_validated_and_persisted_in_the_run_stream() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let assistant = app(store.clone(), vec![structured_response()], vec![]);
    let task = assistant.submit(input(InputSource::Text)).unwrap();
    let completed = assistant.run(task.id).await.unwrap();
    assert_eq!(completed.status, TaskStatus::Completed);
    assert!(matches!(
        completed.output.as_ref().unwrap().blocks[0],
        OutputBlock::Table { .. }
    ));
    let events = store.events(0).unwrap();
    assert_eq!(events[events.len() - 2].kind, RunEventKind::OutputUpsert);
    assert_eq!(events.last().unwrap().kind, RunEventKind::RunTerminal);
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == RunEventKind::RunTerminal)
            .count(),
        1
    );
}

#[tokio::test]
async fn malformed_structured_output_never_reaches_the_persisted_stream() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let invalid = AgentAction::RespondStructured {
        output: AssistantOutput {
            schema: "unknown.output".into(),
            blocks: vec![OutputBlock::Markdown {
                id: "answer".into(),
                markdown: "must not persist".into(),
            }],
        },
    };
    let assistant = app(store.clone(), vec![invalid], vec![respond()]);
    let task = assistant.submit(input(InputSource::Text)).unwrap();
    let completed = assistant.run(task.id).await.unwrap();
    assert_eq!(completed.status, TaskStatus::Completed);
    assert_eq!(completed.output.unwrap().schema, OUTPUT_SCHEMA_V1);
    assert!(!serde_json::to_string(&store.events(0).unwrap())
        .unwrap()
        .contains("unknown.output"));
}
fn app(store: Arc<SqliteStore>, fast: Vec<AgentAction>, cloud: Vec<AgentAction>) -> Assistant {
    Assistant::new(
        store,
        Arc::new(ScriptedProvider::new(true, fast)),
        Arc::new(ScriptedProvider::new(false, cloud)),
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    )
}

struct ToolOnlyProvider;
#[async_trait::async_trait]
impl ModelProvider for ToolOnlyProvider {
    fn id(&self) -> &str {
        "tool-only-fixture"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            tool_calls: true,
            planning: false,
            local: true,
        }
    }
    async fn infer(&self, _: ContextBundle) -> Result<AgentAction> {
        panic!("A tool-only model must not run when no tools were retrieved")
    }
}

struct RateLimitedProvider {
    calls: AtomicUsize,
}

#[async_trait::async_trait]
impl ModelProvider for RateLimitedProvider {
    fn id(&self) -> &str {
        "rate-limited-fixture"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            tool_calls: true,
            planning: true,
            local: false,
        }
    }
    async fn infer(&self, _: ContextBundle) -> Result<AgentAction> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(Error::RateLimited)
    }
}

#[tokio::test]
async fn cloud_quota_fails_once_with_actionable_message_and_leaves_local_usable() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let local = Arc::new(ScriptedProvider::new(true, vec![respond()]));
    let cloud = Arc::new(RateLimitedProvider {
        calls: AtomicUsize::new(0),
    });
    let assistant = Assistant::new(
        store.clone(),
        local,
        cloud.clone(),
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    );
    let mut cloud_task = assistant.submit(input(InputSource::Text)).unwrap();
    cloud_task.role = Role::Reasoner;
    store.save_task(&cloud_task).unwrap();

    let failed = assistant.run(cloud_task.id).await.unwrap();
    assert_eq!(failed.status, TaskStatus::Failed);
    assert_eq!(failed.failures, 1);
    assert_eq!(cloud.calls.load(Ordering::SeqCst), 1);
    assert!(failed.message.contains("retry window"));
    assert!(failed.message.contains("local chat"));
    assert!(failed.pending.is_none());
    assert!(failed.result_refs.is_empty());

    let local_task = assistant.submit(input(InputSource::Text)).unwrap();
    let completed = assistant.run(local_task.id).await.unwrap();
    assert_eq!(completed.status, TaskStatus::Completed);
    assert_eq!(completed.message, "Done");
    assert_eq!(cloud.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn greeting_without_tools_goes_directly_to_cloud() {
    let assistant = Assistant::new(
        Arc::new(SqliteStore::memory().unwrap()),
        Arc::new(ToolOnlyProvider),
        Arc::new(ScriptedProvider::new(false, vec![respond()])),
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    );
    let mut greeting = input(InputSource::Text);
    greeting.text = "hi".into();
    let task = assistant.submit(greeting).unwrap();
    let done = assistant.run(task.id).await.unwrap();
    assert_eq!(done.status, TaskStatus::Completed);
    assert_eq!(done.role, Role::Reasoner);
    assert_eq!(done.failures, 0);
    assert!(done.step <= 2);
}

#[tokio::test]
async fn repeated_search_falls_back_instead_of_spinning() {
    let assistant = app(
        Arc::new(SqliteStore::memory().unwrap()),
        vec![AgentAction::Search {
            query: "  ECHO MESSAGE  ".into(),
        }],
        vec![respond()],
    );
    let task = assistant.submit(input(InputSource::Text)).unwrap();
    let done = assistant.run(task.id).await.unwrap();
    assert_eq!(done.status, TaskStatus::Completed);
    assert_eq!(done.role, Role::Reasoner);
    assert_eq!(done.failures, 1);
}

#[tokio::test]
async fn cloud_auth_pauses_without_consuming_failure_budget_and_resumes() {
    for role in [Role::Reasoner, Role::Planner, Role::Responder] {
        let store = Arc::new(SqliteStore::memory().unwrap());
        let cloud = Arc::new(ScriptedProvider::new(false, vec![respond()]));
        cloud
            .actions
            .lock()
            .unwrap()
            .push_front(Err(Error::AuthRequired));
        let assistant = Assistant::new(
            store.clone(),
            Arc::new(ToolOnlyProvider),
            cloud.clone(),
            Arc::new(EchoExecutor),
            EngineConfig {
                max_failures: 1,
                ..EngineConfig::default()
            },
        );
        let mut task = assistant.submit(input(InputSource::Text)).unwrap();
        task.role = role;
        store.save_task(&task).unwrap();
        let paused = assistant.run(task.id).await.unwrap();
        assert_eq!(paused.status, TaskStatus::WaitingForAuth);
        assert_eq!(paused.failures, 0);
        assert_eq!(paused.step, 1);
        assert!(paused.message.contains("Cloud authentication"));
        assistant.run(task.id).await.unwrap();
        assert_eq!(cloud.actions.lock().unwrap().len(), 1);
        assistant.resume_auth(task.id).unwrap();
        assert_eq!(
            assistant.run(task.id).await.unwrap().status,
            TaskStatus::Completed
        );
    }
}

#[tokio::test]
async fn exhausted_failure_budget_preserves_the_error_reason() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let cloud = Arc::new(ScriptedProvider::new(false, vec![]));
    cloud
        .actions
        .lock()
        .unwrap()
        .extend(vec![Err(Error::InvalidResponse); 3]);
    let assistant = Assistant::new(
        store.clone(),
        Arc::new(ToolOnlyProvider),
        cloud,
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    );
    let mut task = assistant.submit(input(InputSource::Text)).unwrap();
    task.role = Role::Reasoner;
    store.save_task(&task).unwrap();
    let failed = assistant.run(task.id).await.unwrap();
    assert_eq!(failed.status, TaskStatus::Failed);
    assert!(failed.message.contains("Provider response is invalid"));
}

#[tokio::test]
async fn hybrid_handoff_preserves_task_and_results() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    registry::register(
        store.as_ref(),
        &Capability::Tool(example_tool(Risk::ReadOnly)),
    )
    .unwrap();
    let assistant = app(
        store.clone(),
        vec![
            call(),
            AgentAction::Handoff {
                role: Role::Reasoner,
                objective: "Interpret result".into(),
                reason: "Needs reasoning".into(),
            },
        ],
        vec![respond()],
    );
    let task = assistant.submit(input(InputSource::Voice)).unwrap();
    let result = assistant.run(task.id).await.unwrap();
    assert_eq!(result.id, task.id);
    assert_eq!(result.status, TaskStatus::Completed);
    assert_eq!(result.result_refs.len(), 1);
    assert_eq!(
        store.result(result.result_refs[0]).unwrap(),
        json!({"text":"hello"})
    );
    let events = store.events(0).unwrap();
    assert!(events.windows(2).all(|v| v[0].sequence < v[1].sequence));
}

struct WebFixtureExecutor;
#[async_trait::async_trait]
impl ToolExecutor for WebFixtureExecutor {
    async fn execute(&self, spec: &ToolSpec, _: &ToolCall) -> Result<serde_json::Value> {
        Ok(json!({
            "schema":TOOL_RESULT_SCHEMA_V1,
            "source":spec.id,
            "model_context":{
                "kind":"search",
                "query":"current fact",
                "retrieved_at":1_700_000_000,
                "partial":false,
                "sources":[{
                    "id":"source-1",
                    "title":"Primary source",
                    "url":"https://example.com/fact",
                    "untrusted_excerpt":"The supported value is 42."
                }]
            },
            "raw":{"provider_debug":"persisted but hidden from the model"}
        }))
    }
}

#[tokio::test]
async fn web_results_are_persisted_and_rendered_as_source_cards() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let tool = ToolSpec {
        id: "web.search".into(),
        version: "1".into(),
        name: "Search the web".into(),
        description: "Find current facts on the web".into(),
        input_schema: json!({
            "type":"object",
            "properties":{"query":{"type":"string"}},
            "required":["query"],
            "additionalProperties":false
        }),
        output_schema: None,
        connection_id: "parallel-search".into(),
        source_tool: "web_search".into(),
        risk: Risk::ReadOnly,
        enabled: true,
        requires_auth: false,
        requires_network: true,
    };
    registry::register(store.as_ref(), &Capability::Tool(tool.clone())).unwrap();
    let provider = Arc::new(ScriptedProvider::new(
        true,
        vec![
            AgentAction::CallTool {
                call: ToolCall {
                    tool_id: tool.id.clone(),
                    version: tool.version.clone(),
                    arguments: json!({"query":"current fact"}),
                },
            },
            AgentAction::Respond {
                text: "The value is 42 [source:source-1].".into(),
            },
        ],
    ));
    let assistant = Assistant::new(
        store.clone(),
        provider.clone(),
        provider,
        Arc::new(WebFixtureExecutor),
        EngineConfig::default(),
    );
    let mut request = input(InputSource::Text);
    request.text = "Find the current fact".into();
    let task = assistant.submit(request).unwrap();
    let done = assistant.run(task.id).await.unwrap();
    assert_eq!(done.status, TaskStatus::Completed);
    assert!(store
        .result(done.result_refs[0])
        .unwrap()
        .to_string()
        .contains("provider_debug"));
    assert!(matches!(
        done.output.unwrap().blocks.as_slice(),
        [OutputBlock::Markdown { .. }, OutputBlock::Sources { sources, .. }]
            if sources[0].url == "https://example.com/fact"
    ));
}

#[tokio::test]
async fn approval_survives_restart_and_runs_once() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    registry::register(
        store.as_ref(),
        &Capability::Tool(example_tool(Risk::ExternalWrite)),
    )
    .unwrap();
    let first = app(store.clone(), vec![call()], vec![]);
    let task = first.submit(input(InputSource::Text)).unwrap();
    let waiting = first.run(task.id).await.unwrap();
    assert_eq!(waiting.status, TaskStatus::WaitingForApproval);
    assert!(waiting.result_refs.is_empty());
    let approval = waiting.pending.unwrap().id;
    drop(first);
    let second = app(store.clone(), vec![respond()], vec![]);
    second.approve(task.id, approval, true).unwrap();
    let done = second.run(task.id).await.unwrap();
    assert_eq!(done.status, TaskStatus::Completed);
    assert_eq!(done.result_refs.len(), 1);
    assert_eq!(
        second.approve(task.id, approval, true).unwrap_err(),
        Error::Conflict
    );
}

/// Fails the way a tool whose connection is not configured does.
struct UnavailableExecutor;

#[async_trait::async_trait]
impl ToolExecutor for UnavailableExecutor {
    async fn execute(&self, _: &ToolSpec, _: &ToolCall) -> Result<serde_json::Value> {
        Err(Error::Unavailable)
    }
}

/// A cloud provider that is simply not reachable.
struct UnreachableProvider;

#[async_trait::async_trait]
impl ModelProvider for UnreachableProvider {
    fn id(&self) -> &str {
        "unreachable-fixture"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            tool_calls: true,
            planning: true,
            local: false,
        }
    }

    async fn infer(&self, _: ContextBundle) -> Result<AgentAction> {
        Err(Error::Unavailable)
    }

    async fn infer_stream(&self, _: ContextBundle, _: ProviderEventSink) -> Result<AgentAction> {
        Err(Error::Unavailable)
    }
}

#[tokio::test]
async fn an_unreachable_model_is_not_reported_as_a_settings_problem() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let assistant = Assistant::new(
        store.clone(),
        Arc::new(ScriptedProvider::new(true, vec![call()])),
        Arc::new(UnreachableProvider),
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    );
    let task = assistant.submit(input(InputSource::Text)).unwrap();
    let done = assistant.run(task.id).await.unwrap();

    assert_eq!(done.status, TaskStatus::Failed);
    let message = done.message.to_lowercase();
    assert!(
        !message.contains("provider settings"),
        "a dropped connection is not something the user can fix in settings: {}",
        done.message
    );
    assert!(
        message.contains("not reachable") || message.contains("no reasoning model"),
        "the message must name the real cause: {}",
        done.message
    );
    match store.task_blocker(task.id).unwrap() {
        Some(TaskBlocker::ReasoningUnavailable { .. }) => {}
        other => panic!("expected an unreachable-reasoning blocker, got {other:?}"),
    }
}

#[tokio::test]
async fn a_tool_that_cannot_run_is_not_blamed_on_the_cloud_model() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    registry::register(
        store.as_ref(),
        &Capability::Tool(example_tool(Risk::ExternalWrite)),
    )
    .unwrap();
    let assistant = Assistant::new(
        store.clone(),
        Arc::new(ScriptedProvider::new(true, vec![call()])),
        Arc::new(ScriptedProvider::new(false, vec![respond()])),
        Arc::new(UnavailableExecutor),
        EngineConfig::default(),
    );
    let task = assistant.submit(input(InputSource::Text)).unwrap();
    let waiting = assistant.run(task.id).await.unwrap();
    assert_eq!(waiting.status, TaskStatus::WaitingForApproval);
    let approval = waiting.pending.unwrap().id;
    assistant.approve(task.id, approval, true).unwrap();
    let done = assistant.run(task.id).await.unwrap();

    assert_eq!(done.status, TaskStatus::Failed);
    let message = done.message.to_lowercase();
    assert!(
        !message.contains("cloud model"),
        "a tool fault must not be reported as a cloud model fault: {}",
        done.message
    );
    assert!(
        message.contains("connection"),
        "the message must point at the connection the tool needs: {}",
        done.message
    );
}

#[tokio::test]
async fn disabled_or_changed_tool_invalidates_approval() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let mut tool = example_tool(Risk::ExternalWrite);
    registry::register(store.as_ref(), &Capability::Tool(tool.clone())).unwrap();
    let assistant = app(store.clone(), vec![call()], vec![]);
    let task = assistant.submit(input(InputSource::Text)).unwrap();
    let waiting = assistant.run(task.id).await.unwrap();
    tool.enabled = false;
    registry::register(store.as_ref(), &Capability::Tool(tool)).unwrap();
    assert_eq!(
        assistant
            .approve(task.id, waiting.pending.unwrap().id, true)
            .unwrap_err(),
        Error::Conflict
    );
    assert!(store.search("echo", 20).unwrap().is_empty());
}

#[tokio::test]
async fn invalid_arguments_never_execute_and_fall_back() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    registry::register(
        store.as_ref(),
        &Capability::Tool(example_tool(Risk::ReadOnly)),
    )
    .unwrap();
    let bad = AgentAction::CallTool {
        call: ToolCall {
            tool_id: "fixture.echo".into(),
            version: "1".into(),
            arguments: json!({"text":4}),
        },
    };
    let assistant = app(store, vec![bad], vec![respond()]);
    let task = assistant.submit(input(InputSource::Text)).unwrap();
    let done = assistant.run(task.id).await.unwrap();
    assert!(done.result_refs.is_empty());
    assert_eq!(done.role, Role::Reasoner);
}

#[tokio::test]
async fn interrupted_write_is_not_replayed() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let tool = example_tool(Risk::ExternalWrite);
    let mut task = Task::new(input(InputSource::Text));
    task.status = TaskStatus::Running;
    task.pending = Some(PendingAction {
        id: Id::new_v4(),
        call: ToolCall {
            tool_id: tool.id.clone(),
            version: tool.version.clone(),
            arguments: json!({"text":"hello"}),
        },
        spec: tool,
        approved: true,
        started: true,
    });
    store.save_task(&task).unwrap();
    let assistant = app(store, vec![respond()], vec![]);
    let result = assistant.run(task.id).await.unwrap();
    assert_eq!(result.status, TaskStatus::WaitingForResolution);
    assert!(result.result_refs.is_empty());
}

#[test]
fn unified_search_scopes_skills_and_rejects_remote_schema_refs() {
    let store = SqliteStore::memory().unwrap();
    let mut tool = example_tool(Risk::ReadOnly);
    registry::register(&store, &Capability::Tool(tool.clone())).unwrap();
    let skill = SkillSpec {
        id: "skill.echo".into(),
        version: "1".into(),
        name: "Echo helper".into(),
        description: "echo message".into(),
        instructions: "Use echo".into(),
        tool_requirements: vec![tool.id.clone()],
        enabled: true,
    };
    registry::register(&store, &Capability::Skill(skill)).unwrap();
    assert_eq!(store.search("echo", 20).unwrap().len(), 2);
    assert!(registry::activate(&store, "skill.echo").is_ok());
    tool.enabled = false;
    registry::register(&store, &Capability::Tool(tool.clone())).unwrap();
    assert!(registry::activate(&store, "skill.echo").is_err());
    tool.input_schema = json!({"$ref":"https://example.com/schema"});
    assert_eq!(
        registry::register(&store, &Capability::Tool(tool.clone())).unwrap_err(),
        Error::InvalidInput
    );
    tool.input_schema = json!({"type":"object"});
    tool.output_schema = Some(json!({"$ref":"https://example.com/result-schema"}));
    assert_eq!(
        registry::register(&store, &Capability::Tool(tool)).unwrap_err(),
        Error::InvalidInput
    );
    assert!(store.search("\" OR * --", 20).is_ok());
}

struct InspectProvider {
    count: std::sync::Mutex<Vec<(usize, usize)>>,
}
#[async_trait::async_trait]
impl ModelProvider for InspectProvider {
    fn id(&self) -> &str {
        "inspect"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            tool_calls: true,
            planning: true,
            local: true,
        }
    }
    async fn infer(&self, context: ContextBundle) -> Result<AgentAction> {
        self.count.lock().unwrap().push((
            context.tools.len(),
            serde_json::to_vec(&context).unwrap().len(),
        ));
        Ok(respond())
    }
}
#[tokio::test]
async fn catalogue_growth_does_not_expand_model_context() {
    for size in [10, 100, 1000, 5000] {
        let store = Arc::new(SqliteStore::memory().unwrap());
        for index in 0..size {
            let mut tool = example_tool(Risk::ReadOnly);
            tool.id = format!("fixture.echo_{index}");
            registry::register(store.as_ref(), &Capability::Tool(tool)).unwrap();
        }
        let provider = Arc::new(InspectProvider {
            count: std::sync::Mutex::new(vec![]),
        });
        let assistant = Assistant::new(
            store,
            provider.clone(),
            provider.clone(),
            Arc::new(EchoExecutor),
            EngineConfig::default(),
        );
        let task = assistant.submit(input(InputSource::Text)).unwrap();
        assistant.run(task.id).await.unwrap();
        let measurements = provider.count.lock().unwrap();
        assert!(measurements[0].0 <= 8);
        assert!(measurements[0].1 <= EngineConfig::default().fast_context_bytes);
    }
}
#[tokio::test]
async fn unavailable_needle_can_delegate_planning_and_continue_locally() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let assistant = app(
        store,
        vec![
            AgentAction::Handoff {
                role: Role::Planner,
                objective: "Plan".into(),
                reason: "Complex".into(),
            },
            respond(),
        ],
        vec![AgentAction::Plan {
            steps: vec!["Answer".into()],
        }],
    );
    let task = assistant.submit(input(InputSource::Text)).unwrap();
    let done = assistant.run(task.id).await.unwrap();
    assert_eq!(done.plan_revision, 1);
    assert_eq!(done.status, TaskStatus::Completed);
    assert_eq!(done.role, Role::Fast);
}
#[tokio::test]
async fn asks_user_and_resumes_same_task() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let assistant = app(
        store,
        vec![
            AgentAction::AskUser {
                question: "Which account?".into(),
            },
            respond(),
        ],
        vec![],
    );
    let task = assistant.submit(input(InputSource::Text)).unwrap();
    assert_eq!(
        assistant.run(task.id).await.unwrap().status,
        TaskStatus::WaitingForUser
    );
    assistant.answer(task.id, "Personal").unwrap();
    assert_eq!(
        assistant.run(task.id).await.unwrap().status,
        TaskStatus::Completed
    );
}
struct AuthExecutor {
    calls: std::sync::atomic::AtomicUsize,
}
#[async_trait::async_trait]
impl ToolExecutor for AuthExecutor {
    async fn execute(&self, _: &ToolSpec, _: &ToolCall) -> Result<serde_json::Value> {
        if self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            Err(Error::AuthRequired)
        } else {
            Ok(json!({"connected":true}))
        }
    }
}
#[tokio::test]
async fn auth_pause_resumes_the_pending_step() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    registry::register(
        store.as_ref(),
        &Capability::Tool(example_tool(Risk::ReadOnly)),
    )
    .unwrap();
    let provider = Arc::new(ScriptedProvider::new(true, vec![call(), respond()]));
    let assistant = Assistant::new(
        store,
        provider.clone(),
        provider,
        Arc::new(AuthExecutor {
            calls: std::sync::atomic::AtomicUsize::new(0),
        }),
        EngineConfig::default(),
    );
    let task = assistant.submit(input(InputSource::Text)).unwrap();
    let paused = assistant.run(task.id).await.unwrap();
    assert_eq!(paused.status, TaskStatus::WaitingForAuth);
    let pending = paused.pending.unwrap().id;
    assistant.resume_auth(task.id).unwrap();
    let done = assistant.run(task.id).await.unwrap();
    assert_eq!(done.result_refs, vec![pending]);
    assert_eq!(done.status, TaskStatus::Completed);
}

#[tokio::test]
async fn auth_failure_after_write_dispatch_is_never_replayed() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    registry::register(
        store.as_ref(),
        &Capability::Tool(example_tool(Risk::ExternalWrite)),
    )
    .unwrap();
    let provider = Arc::new(ScriptedProvider::new(true, vec![call()]));
    let executor = Arc::new(AuthExecutor {
        calls: AtomicUsize::new(0),
    });
    let assistant = Assistant::new(
        store,
        provider.clone(),
        provider,
        executor.clone(),
        EngineConfig::default(),
    );
    let task = assistant.submit(input(InputSource::Text)).unwrap();
    let waiting = assistant.run(task.id).await.unwrap();
    let approval = waiting.pending.unwrap().id;
    assistant.approve(task.id, approval, true).unwrap();

    let uncertain = assistant.run(task.id).await.unwrap();

    assert_eq!(uncertain.status, TaskStatus::WaitingForResolution);
    assert!(uncertain.pending.unwrap().started);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(assistant.resume_auth(task.id).unwrap_err(), Error::Conflict);
    assert_eq!(
        assistant.run(task.id).await.unwrap().status,
        TaskStatus::WaitingForResolution
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}
struct SlowProvider {
    entered: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}
#[async_trait::async_trait]
impl ModelProvider for SlowProvider {
    fn id(&self) -> &str {
        "slow"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            tool_calls: true,
            planning: false,
            local: true,
        }
    }
    async fn infer(&self, _: ContextBundle) -> Result<AgentAction> {
        self.entered.notify_one();
        self.release.notified().await;
        Ok(call())
    }
}
#[tokio::test]
async fn cancellation_discards_late_model_tool_call() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    registry::register(
        store.as_ref(),
        &Capability::Tool(example_tool(Risk::ReadOnly)),
    )
    .unwrap();
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let provider = Arc::new(SlowProvider {
        entered: entered.clone(),
        release: release.clone(),
    });
    let assistant = Arc::new(Assistant::new(
        store.clone(),
        provider.clone(),
        provider,
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    ));
    let task = assistant.submit(input(InputSource::Text)).unwrap();
    let running = assistant.clone();
    let handle = tokio::spawn(async move { running.run(task.id).await.unwrap() });
    entered.notified().await;
    assistant.cancel(task.id).unwrap();
    release.notify_one();
    let done = handle.await.unwrap();
    assert_eq!(done.status, TaskStatus::Cancelled);
    assert!(done.result_refs.is_empty());
    let events = store.events(0).unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == RunEventKind::RunTerminal)
            .count(),
        1
    );
    assert!(!events
        .iter()
        .any(|event| event.kind == RunEventKind::OutputUpsert));
}

fn graph_proposal(nodes: Vec<ProposedWorkNode>) -> AgentAction {
    AgentAction::PlanGraph {
        proposal: WorkGraphProposal {
            schema: WORK_GRAPH_PROPOSAL_SCHEMA_V1.into(),
            nodes,
        },
    }
}

fn infer_node(key: &str, dependencies: &[&str]) -> ProposedWorkNode {
    ProposedWorkNode {
        key: key.into(),
        objective: key.into(),
        operation: WorkerOperation::Infer,
        dependencies: dependencies.iter().map(|value| (*value).into()).collect(),
        dependency_policy: DependencyPolicy::AllSucceeded,
    }
}

struct ParallelGraphProvider {
    active: AtomicUsize,
    max_active: AtomicUsize,
}

#[async_trait::async_trait]
impl ModelProvider for ParallelGraphProvider {
    fn id(&self) -> &str {
        "parallel-graph-fixture"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            tool_calls: true,
            planning: true,
            local: false,
        }
    }

    async fn infer(&self, context: ContextBundle) -> Result<AgentAction> {
        match context.role {
            Role::Planner => Ok(graph_proposal(vec![
                infer_node("first", &[]),
                infer_node("second", &[]),
            ])),
            Role::Reasoner => {
                let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
                self.max_active.fetch_max(active, Ordering::SeqCst);
                let delay = if context.goal == "first" { 40 } else { 5 };
                tokio::time::sleep(Duration::from_millis(delay)).await;
                self.active.fetch_sub(1, Ordering::SeqCst);
                Ok(AgentAction::Respond { text: context.goal })
            }
            Role::Responder => Ok(respond()),
            Role::Fast => unreachable!(),
        }
    }
}

#[tokio::test]
async fn planner_graph_runs_two_workers_concurrently_and_joins_in_graph_order() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let provider = Arc::new(ParallelGraphProvider {
        active: AtomicUsize::new(0),
        max_active: AtomicUsize::new(0),
    });
    let assistant = Assistant::new(
        store.clone(),
        Arc::new(ToolOnlyProvider),
        provider.clone(),
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    );
    let mut task = assistant.submit(input(InputSource::Text)).unwrap();
    task.role = Role::Planner;
    store.save_task(&task).unwrap();

    let done = assistant.run(task.id).await.unwrap();

    assert_eq!(done.status, TaskStatus::Completed);
    assert_eq!(provider.max_active.load(Ordering::SeqCst), 2);
    assert_eq!(done.result_refs.len(), 2);
    assert_eq!(
        store.result(done.result_refs[0]).unwrap()["response"],
        "first"
    );
    assert_eq!(
        store.result(done.result_refs[1]).unwrap()["response"],
        "second"
    );
    assert!(done
        .work_graph
        .unwrap()
        .nodes
        .iter()
        .all(|node| node.state == WorkNodeState::Completed));
}

struct HangingGraphProvider {
    entered: Arc<tokio::sync::Notify>,
}

#[async_trait::async_trait]
impl ModelProvider for HangingGraphProvider {
    fn id(&self) -> &str {
        "hanging-graph-fixture"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            tool_calls: true,
            planning: true,
            local: false,
        }
    }

    async fn infer(&self, context: ContextBundle) -> Result<AgentAction> {
        match context.role {
            Role::Planner => Ok(graph_proposal(vec![
                infer_node("root", &[]),
                infer_node("descendant", &["root"]),
            ])),
            Role::Reasoner => {
                self.entered.notify_one();
                std::future::pending().await
            }
            Role::Responder => Ok(respond()),
            Role::Fast => unreachable!(),
        }
    }
}

#[tokio::test]
async fn graph_cancellation_aborts_active_work_and_cancels_descendants() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let entered = Arc::new(tokio::sync::Notify::new());
    let provider = Arc::new(HangingGraphProvider {
        entered: entered.clone(),
    });
    let assistant = Arc::new(Assistant::new(
        store.clone(),
        Arc::new(ToolOnlyProvider),
        provider,
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    ));
    let mut task = assistant.submit(input(InputSource::Text)).unwrap();
    task.role = Role::Planner;
    store.save_task(&task).unwrap();
    let running = assistant.clone();
    let handle = tokio::spawn(async move { running.run(task.id).await.unwrap() });
    entered.notified().await;

    assistant.cancel(task.id).unwrap();
    let cancelled = tokio::time::timeout(Duration::from_secs(1), handle)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(cancelled.status, TaskStatus::Cancelled);
    assert!(cancelled
        .work_graph
        .unwrap()
        .nodes
        .iter()
        .all(|node| node.state == WorkNodeState::Cancelled));
}

struct ResumeGraphProvider;

#[async_trait::async_trait]
impl ModelProvider for ResumeGraphProvider {
    fn id(&self) -> &str {
        "resume-graph-fixture"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            tool_calls: true,
            planning: true,
            local: false,
        }
    }

    async fn infer(&self, context: ContextBundle) -> Result<AgentAction> {
        match context.role {
            Role::Reasoner => Ok(AgentAction::Respond {
                text: "recovered".into(),
            }),
            Role::Responder => Ok(respond()),
            _ => unreachable!(),
        }
    }
}

fn interrupted_infer_task(status: TaskStatus) -> Task {
    let mut task = Task::new(input(InputSource::Text));
    task.status = status;
    let node_id = Id::new_v4();
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    task.work_graph = Some(WorkGraph {
        schema: WORK_GRAPH_SCHEMA_V1.into(),
        task_id: task.id,
        created_at: at.saturating_sub(1),
        deadline_at: at + 60,
        max_parallel_workers: 2,
        nodes: vec![WorkNode {
            id: node_id,
            task_id: task.id,
            idempotency_key: format!("{}:0:recover", task.id),
            deadline_at: at + 60,
            retry_budget: 1,
            attempts: 1,
            state: WorkNodeState::Running,
            dependency_policy: DependencyPolicy::AllSucceeded,
            request: WorkerRequest {
                worker_id: Id::new_v4(),
                node_id,
                task_id: task.id,
                objective: "recover safe work".into(),
                operation: WorkerOperation::Infer,
                context: ContextBundle {
                    task_id: task.id,
                    role: Role::Reasoner,
                    goal: "recover safe work".into(),
                    plan: vec![],
                    handoff: None,
                    history: vec![],
                    results: vec![],
                    skills: vec![],
                    candidates: vec![],
                    tools: vec![],
                    adaptive_rules: vec![],
                },
            },
            result_refs: vec![],
        }],
        edges: vec![],
    });
    task
}

#[tokio::test]
async fn startup_recovery_runs_only_active_retry_safe_graphs() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let safe = interrupted_infer_task(TaskStatus::Running);
    let paused = interrupted_infer_task(TaskStatus::WaitingForAuth);
    let mut changed_tool = interrupted_infer_task(TaskStatus::Running);
    let read = example_tool(Risk::ReadOnly);
    let call = ToolCall {
        tool_id: read.id.clone(),
        version: read.version.clone(),
        arguments: json!({"text":"hello"}),
    };
    let node = &mut changed_tool.work_graph.as_mut().unwrap().nodes[0];
    node.request.operation = WorkerOperation::CallTool { call: call.clone() };
    node.request.context.tools = vec![read.clone()];
    store.save_task(&safe).unwrap();
    store.save_task(&paused).unwrap();
    store.save_task(&changed_tool).unwrap();
    let mut write = read;
    write.risk = Risk::ExternalWrite;
    store.put_capability(&Capability::Tool(write)).unwrap();

    let executor = Arc::new(CountingExecutor(AtomicUsize::new(0)));
    let provider = Arc::new(ResumeGraphProvider);
    let assistant = Assistant::new(
        store.clone(),
        Arc::new(ToolOnlyProvider),
        provider,
        executor.clone(),
        EngineConfig::default(),
    );

    assert_eq!(
        assistant.recover_unfinished_graphs().unwrap(),
        vec![safe.id]
    );
    assert_eq!(
        store.task(safe.id).unwrap().work_graph.unwrap().nodes[0].state,
        WorkNodeState::Queued
    );
    assert_eq!(
        store.task(paused.id).unwrap().status,
        TaskStatus::WaitingForAuth
    );
    assert_eq!(
        store.task(changed_tool.id).unwrap().status,
        TaskStatus::Failed
    );
    assert_eq!(executor.0.load(Ordering::SeqCst), 0);

    let done = assistant.run(safe.id).await.unwrap();
    assert_eq!(done.status, TaskStatus::Completed);
}

struct RecoveringAuthProvider(AtomicUsize);

#[async_trait::async_trait]
impl ModelProvider for RecoveringAuthProvider {
    fn id(&self) -> &str {
        "recovering-auth-fixture"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            tool_calls: true,
            planning: true,
            local: false,
        }
    }

    async fn infer(&self, context: ContextBundle) -> Result<AgentAction> {
        match context.role {
            Role::Reasoner if self.0.fetch_add(1, Ordering::SeqCst) == 0 => {
                Err(Error::AuthRequired)
            }
            Role::Reasoner => Ok(AgentAction::Respond {
                text: "recovered".into(),
            }),
            Role::Responder => Ok(respond()),
            _ => unreachable!(),
        }
    }
}

#[tokio::test]
async fn recovered_graph_auth_pause_is_retryable_without_consuming_budget() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let task = interrupted_infer_task(TaskStatus::Running);
    store.save_task(&task).unwrap();
    let provider = Arc::new(RecoveringAuthProvider(AtomicUsize::new(0)));
    let assistant = Assistant::new(
        store.clone(),
        Arc::new(ToolOnlyProvider),
        provider,
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    );
    assert_eq!(
        assistant.recover_unfinished_graphs().unwrap(),
        vec![task.id]
    );

    let waiting = assistant.run(task.id).await.unwrap();
    assert_eq!(waiting.status, TaskStatus::WaitingForAuth);
    assert_eq!(waiting.failures, 0);
    assert_eq!(
        waiting.work_graph.as_ref().unwrap().nodes[0].state,
        WorkNodeState::Queued
    );
    let attempts = waiting.work_graph.as_ref().unwrap().nodes[0].attempts;

    assistant.resume_auth(task.id).unwrap();
    let done = assistant.run(task.id).await.unwrap();
    assert_eq!(done.status, TaskStatus::Completed);
    assert_eq!(done.failures, 0);
    assert_eq!(done.work_graph.unwrap().nodes[0].attempts, attempts + 1);
}

#[tokio::test]
async fn restart_requeues_only_interrupted_safe_graph_work() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let entered = Arc::new(tokio::sync::Notify::new());
    let first_provider = Arc::new(HangingGraphProvider {
        entered: entered.clone(),
    });
    let first = Arc::new(Assistant::new(
        store.clone(),
        Arc::new(ToolOnlyProvider),
        first_provider,
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    ));
    let mut task = first.submit(input(InputSource::Text)).unwrap();
    task.role = Role::Planner;
    store.save_task(&task).unwrap();
    let running = first.clone();
    let handle = tokio::spawn(async move { running.run(task.id).await.unwrap() });
    entered.notified().await;
    handle.abort();
    assert!(handle.await.unwrap_err().is_cancelled());
    assert_eq!(
        store.task(task.id).unwrap().work_graph.unwrap().nodes[0].state,
        WorkNodeState::Running
    );

    let resumed_provider = Arc::new(ResumeGraphProvider);
    let resumed = Assistant::new(
        store.clone(),
        Arc::new(ToolOnlyProvider),
        resumed_provider,
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    );
    let done = resumed.run(task.id).await.unwrap();

    assert_eq!(done.status, TaskStatus::Completed);
    let graph = done.work_graph.unwrap();
    assert_eq!(graph.nodes[0].attempts, 2);
    assert_eq!(graph.nodes[0].state, WorkNodeState::Completed);
    assert_eq!(graph.nodes[1].state, WorkNodeState::Completed);
}

struct CountingExecutor(AtomicUsize);

#[async_trait::async_trait]
impl ToolExecutor for CountingExecutor {
    async fn execute(&self, _: &ToolSpec, _: &ToolCall) -> Result<serde_json::Value> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(json!({"unexpected": true}))
    }
}

#[tokio::test]
async fn recovered_graph_revalidates_current_tool_spec_before_execution() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let original = example_tool(Risk::ReadOnly);
    registry::register(store.as_ref(), &Capability::Tool(original.clone())).unwrap();
    let mut task = Task::new(input(InputSource::Text));
    task.status = TaskStatus::Running;
    let node_id = Id::new_v4();
    let worker_id = Id::new_v4();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    task.work_graph = Some(WorkGraph {
        schema: WORK_GRAPH_SCHEMA_V1.into(),
        task_id: task.id,
        created_at: now,
        deadline_at: now + 60,
        max_parallel_workers: 2,
        nodes: vec![WorkNode {
            id: node_id,
            task_id: task.id,
            idempotency_key: format!("{}:0:tool", task.id),
            deadline_at: now + 60,
            retry_budget: 1,
            attempts: 1,
            state: WorkNodeState::Running,
            dependency_policy: DependencyPolicy::AllSucceeded,
            request: WorkerRequest {
                worker_id,
                node_id,
                task_id: task.id,
                objective: "read".into(),
                operation: WorkerOperation::CallTool {
                    call: ToolCall {
                        tool_id: original.id.clone(),
                        version: original.version.clone(),
                        arguments: json!({"text":"hello"}),
                    },
                },
                context: ContextBundle {
                    task_id: task.id,
                    role: Role::Reasoner,
                    goal: "read".into(),
                    plan: vec![],
                    handoff: None,
                    history: vec![],
                    results: vec![],
                    skills: vec![],
                    candidates: vec![],
                    tools: vec![original.clone()],
                    adaptive_rules: vec![],
                },
            },
            result_refs: vec![],
        }],
        edges: vec![],
    });
    store.save_task(&task).unwrap();
    let mut disabled = original;
    disabled.enabled = false;
    registry::register(store.as_ref(), &Capability::Tool(disabled)).unwrap();
    let executor = Arc::new(CountingExecutor(AtomicUsize::new(0)));
    let assistant = Assistant::new(
        store,
        Arc::new(ToolOnlyProvider),
        Arc::new(ScriptedProvider::new(false, vec![])),
        executor.clone(),
        EngineConfig::default(),
    );

    let failed = assistant.run(task.id).await.unwrap();

    assert_eq!(failed.status, TaskStatus::Failed);
    assert_eq!(executor.0.load(Ordering::SeqCst), 0);
    assert_eq!(
        failed.work_graph.unwrap().nodes[0].state,
        WorkNodeState::Failed
    );
}
