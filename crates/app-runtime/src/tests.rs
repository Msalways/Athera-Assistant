use super::*;

#[tokio::test]
async fn injected_fast_provider_reports_ready() {
    let fast: Arc<dyn ModelProvider> =
        Arc::new(assistant_core::testing::ScriptedProvider::new(true, vec![]));
    let runtime = Runtime::open_with_fast_provider(":memory:", fast).unwrap();
    assert_eq!(runtime.snapshot().await.unwrap()["needle"], "ready");
}

#[tokio::test]
async fn run_event_pages_preserve_the_reconnect_cursor() {
    let empty_runtime = Runtime::open(":memory:").unwrap();
    let empty_reset = empty_runtime
        .dispatch("run_events", serde_json::json!({"after":99}))
        .await
        .unwrap();
    assert_eq!(empty_reset["reset_required"], true);
    assert_eq!(empty_reset["next_after"], 0);

    let runtime = Runtime::open(":memory:").unwrap();
    let task = Task::new(UserInput {
        conversation_id: Id::new_v4(),
        text: "fixture".into(),
        source: InputSource::Text,
    });
    runtime.store.save_task(&task).unwrap();
    let page = runtime
        .dispatch("run_events", serde_json::json!({"after":0}))
        .await
        .unwrap();
    assert_eq!(page["schema"], RUN_EVENT_PAGE_SCHEMA_V1);
    assert_eq!(page["after"], 0);
    assert_eq!(page["next_after"], 1);
    assert_eq!(page["has_more"], false);
    assert_eq!(page["reset_required"], false);
    assert_eq!(page["events"][0]["kind"], "run_started");
    assert_eq!(page["events"][0]["run_id"], task.id.to_string());
    assert_eq!(page["events"][0]["task_id"], task.id.to_string());
    let empty = runtime
        .dispatch("run_events", serde_json::json!({"after":1}))
        .await
        .unwrap();
    assert_eq!(empty["next_after"], 1);
    assert_eq!(empty["events"], serde_json::json!([]));

    let reset = runtime
        .dispatch("run_events", serde_json::json!({"after":99}))
        .await
        .unwrap();
    assert_eq!(reset["reset_required"], true);
    assert_eq!(reset["next_after"], 1);
    assert_eq!(
        reset["events"][0]["event_id"],
        page["events"][0]["event_id"]
    );
}

#[tokio::test]
async fn run_event_pages_report_when_more_events_are_available() {
    let runtime = Runtime::open(":memory:").unwrap();
    for index in 0..201 {
        runtime
            .store
            .save_task(&Task::new(UserInput {
                conversation_id: Id::new_v4(),
                text: format!("fixture {index}"),
                source: InputSource::Text,
            }))
            .unwrap();
    }
    let first = runtime
        .dispatch("run_events", serde_json::json!({"after":0}))
        .await
        .unwrap();
    assert_eq!(first["events"].as_array().unwrap().len(), 200);
    assert_eq!(first["next_after"], 200);
    assert_eq!(first["has_more"], true);

    let second = runtime
        .dispatch("run_events", serde_json::json!({"after":200}))
        .await
        .unwrap();
    assert_eq!(second["events"].as_array().unwrap().len(), 1);
    assert_eq!(second["next_after"], 201);
    assert_eq!(second["has_more"], false);
}

#[tokio::test]
async fn web_key_is_session_only_endpoint_bound_and_replaceable() {
    let path = std::env::temp_dir().join(format!("assistant-session-test-{}.db", Id::new_v4()));
    let reference = format!("ASSISTANT_{}_KEY", Id::new_v4().simple()).to_uppercase();
    let runtime = Runtime::open(&path).unwrap();
    let cloud = settings(&reference).cloud.unwrap();
    let key = "fixture-session-key-not-a-real-credential";
    let response = runtime
        .dispatch(
            "save_cloud_provider",
            serde_json::json!({"cloud":cloud,"api_key":key}),
        )
        .await
        .unwrap();
    assert!(response.is_null());
    assert_eq!(
        runtime.secrets.provider(&cloud).get(&reference).unwrap(),
        key
    );
    assert_eq!(runtime.snapshot().await.unwrap()["cloud_session_key"], true);
    assert_eq!(
        runtime.snapshot().await.unwrap()["cloud_credential"],
        "configured"
    );
    for output in [
        runtime.snapshot().await.unwrap(),
        runtime.store.setting("settings").unwrap().unwrap(),
        serde_json::json!(runtime.store.events(0).unwrap()),
    ] {
        assert!(!output.to_string().contains(key));
    }
    let mut other = cloud.clone();
    other.endpoint = "https://another.example/v1".into();
    assert_eq!(
        runtime.secrets.provider(&other).get(&reference),
        Err(Error::AuthRequired)
    );
    runtime
        .dispatch(
            "save_cloud_provider",
            serde_json::json!({"cloud":cloud,"api_key":null}),
        )
        .await
        .unwrap();
    assert!(runtime.secrets.contains(&cloud));
    let mut invalid = cloud.clone();
    invalid.endpoint = "http://not-https.example".into();
    assert_eq!(
        runtime
            .dispatch(
                "save_cloud_provider",
                serde_json::json!({"cloud":invalid,"api_key":"replacement"})
            )
            .await,
        Err(Error::InvalidInput)
    );
    assert_eq!(
        runtime.secrets.provider(&cloud).get(&reference).unwrap(),
        key
    );
    runtime
        .dispatch(
            "save_cloud_provider",
            serde_json::json!({"cloud":cloud,"api_key":"replacement-fixture-key"}),
        )
        .await
        .unwrap();
    assert_eq!(
        runtime.secrets.provider(&cloud).get(&reference).unwrap(),
        "replacement-fixture-key"
    );
    runtime
        .dispatch("clear_cloud_key", serde_json::json!({}))
        .await
        .unwrap();
    assert_eq!(
        runtime.snapshot().await.unwrap()["cloud_credential"],
        "missing"
    );
    runtime
        .dispatch(
            "save_cloud_provider",
            serde_json::json!({"cloud":cloud,"api_key":key}),
        )
        .await
        .unwrap();
    drop(runtime);
    let reopened = Runtime::open(&path).unwrap();
    assert_eq!(
        reopened.snapshot().await.unwrap()["cloud_credential"],
        "missing"
    );
    assert_eq!(
        reopened.snapshot().await.unwrap()["cloud_session_key"],
        false
    );
    drop(reopened);
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn malformed_keys_never_change_configuration() {
    let runtime = Runtime::open(":memory:").unwrap();
    let cloud = settings("ASSISTANT_FIXTURE_KEY").cloud.unwrap();
    for key in [
        String::new(),
        "spaces in key".into(),
        "key\nline".into(),
        "x".repeat(8193),
    ] {
        assert_eq!(
            runtime
                .dispatch(
                    "save_cloud_provider",
                    serde_json::json!({"cloud":cloud,"api_key":key})
                )
                .await,
            Err(Error::InvalidInput)
        );
        assert!(runtime.store.setting("settings").unwrap().is_none());
    }
}

fn settings(reference: &str) -> Settings {
    Settings {
        cloud: Some(CloudConfig {
            id: "fixture".into(),
            endpoint: "https://example.com/v1".into(),
            model: "fixture-model".into(),
            secret_ref: reference.into(),
            api: provider_cloud::ApiKind::ChatCompletions,
            max_output_tokens: 1024,
        }),
        ..Settings::default()
    }
}

#[test]
fn repairs_legacy_invalid_reference_without_changing_valid_references() {
    let mut legacy = settings("nvapi-fixture-not-a-real-key");
    assert!(repair_credential_reference(&mut legacy));
    assert_eq!(
        legacy.cloud.as_ref().unwrap().secret_ref,
        "ASSISTANT_CLOUD_KEY"
    );
    assert!(!repair_credential_reference(&mut legacy));
    let mut custom = settings("ASSISTANT_NVIDIA_KEY");
    assert!(!repair_credential_reference(&mut custom));
    assert_eq!(custom.cloud.unwrap().secret_ref, "ASSISTANT_NVIDIA_KEY");
}

#[tokio::test]
async fn rejects_key_values_before_persistence_and_reports_missing_credentials() {
    let runtime = Runtime::open(":memory:").unwrap();
    assert_eq!(
        runtime
            .configure(settings("nvapi-fixture-not-a-real-key"))
            .await,
        Err(Error::InvalidInput)
    );
    assert!(runtime.store.setting("settings").unwrap().is_none());
    assert_eq!(
        runtime.snapshot().await.unwrap()["cloud_credential"],
        "not_configured"
    );
    let reference = format!("ASSISTANT_{}_KEY", Id::new_v4().simple()).to_uppercase();
    runtime.configure(settings(&reference)).await.unwrap();
    assert_eq!(
        runtime.snapshot().await.unwrap()["cloud_credential"],
        "missing"
    );
}

#[tokio::test]
async fn mcp_credentials_are_session_only_and_bound_before_network_io() {
    let path = std::env::temp_dir().join(format!("assistant-mcp-auth-{}.db", Id::new_v4()));
    let runtime = Runtime::open(&path).unwrap();
    let mut connection = ConnectionConfig {
        schema: adapter_mcp::MCP_CONNECTION_SCHEMA_V1.into(),
        id: "protected-fixture".into(),
        name: "Protected fixture".into(),
        url: "https://does-not-resolve.invalid/mcp".into(),
        transport: adapter_mcp::McpTransport::StreamableHttp,
        authentication: adapter_mcp::McpAuthentication::BearerToken {
            secret_ref: "keystore:protected-fixture".into(),
        },
        preset: None,
    };
    assert_eq!(
        runtime
            .dispatch("connect_mcp", serde_json::json!(connection))
            .await,
        Err(Error::AuthRequired)
    );
    assert_eq!(
        runtime.snapshot().await.unwrap()["settings"]["connections"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let secret = "fixture-mcp-secret-not-a-real-credential";
    runtime
        .dispatch(
            "save_mcp_credential",
            serde_json::json!({"connection_id":connection.id,"secret":secret}),
        )
        .await
        .unwrap();
    let status = runtime
        .dispatch(
            "mcp_connection_status",
            serde_json::json!({"connection_id":connection.id}),
        )
        .await
        .unwrap();
    assert_eq!(status["state"], "connected");
    assert_eq!(status["schema"], CONNECTION_STATE_SCHEMA_V1);
    for output in [
        runtime.snapshot().await.unwrap(),
        runtime.store.setting("settings").unwrap().unwrap(),
        serde_json::json!(runtime.store.events(0).unwrap()),
        status,
    ] {
        assert!(!output.to_string().contains(secret));
    }
    assert_eq!(
        runtime
            .dispatch(
                "submit_input",
                serde_json::json!({
                    "conversation_id":Id::new_v4(),
                    "text":format!("repeat {secret}"),
                    "source":"text"
                })
            )
            .await,
        Err(Error::Denied)
    );

    connection.url = "https://changed.invalid/mcp".into();
    assert_eq!(
        runtime
            .dispatch("connect_mcp", serde_json::json!(connection))
            .await,
        Err(Error::AuthRequired)
    );
    assert_eq!(
        runtime
            .dispatch(
                "mcp_connection_status",
                serde_json::json!({"connection_id":"protected-fixture"}),
            )
            .await
            .unwrap()["state"],
        "required"
    );
    drop(runtime);

    let reopened = Runtime::open(&path).unwrap();
    assert_eq!(
        reopened
            .dispatch(
                "mcp_connection_status",
                serde_json::json!({"connection_id":"protected-fixture"}),
            )
            .await
            .unwrap()["state"],
        "required"
    );
    drop(reopened);
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn oauth_manifest_is_persisted_but_access_tokens_require_the_oauth_flow() {
    let runtime = Runtime::open(":memory:").unwrap();
    let connection = ConnectionConfig {
        schema: adapter_mcp::MCP_CONNECTION_SCHEMA_V1.into(),
        id: "oauth-fixture".into(),
        name: "OAuth fixture".into(),
        url: "https://mcp.example.com/api".into(),
        transport: adapter_mcp::McpTransport::StreamableHttp,
        authentication: adapter_mcp::McpAuthentication::OauthAuthorizationCode {
            token_ref: "keystore:oauth-fixture".into(),
            authorization_server: "https://auth.example.com".into(),
            resource: "https://mcp.example.com/api".into(),
            requested_scopes: vec!["read".into()],
        },
        preset: None,
    };
    runtime
        .dispatch("save_mcp_connection", serde_json::json!(connection))
        .await
        .unwrap();
    let status = runtime
        .dispatch(
            "mcp_connection_status",
            serde_json::json!({"connection_id":connection.id}),
        )
        .await
        .unwrap();
    assert_eq!(status["state"], "required");
    assert_eq!(status["requested_scopes"], serde_json::json!(["read"]));
    assert_eq!(
        runtime
            .dispatch(
                "save_mcp_credential",
                serde_json::json!({
                    "connection_id":connection.id,
                    "secret":"not-an-oauth-token"
                }),
            )
            .await,
        Err(Error::Denied)
    );
    assert_eq!(
        runtime
            .dispatch("connect_mcp", serde_json::json!(connection))
            .await,
        Err(Error::AuthRequired)
    );
}

#[tokio::test]
async fn opening_runtime_finalizes_expired_interrupted_graphs() {
    let path = std::env::temp_dir().join(format!("assistant-recovery-test-{}.db", Id::new_v4()));
    let store = SqliteStore::open(&path).unwrap();
    let mut task = Task::new(UserInput {
        conversation_id: Id::new_v4(),
        text: "recover after restart".into(),
        source: InputSource::Text,
    });
    task.status = TaskStatus::Running;
    let node_id = Id::new_v4();
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    task.work_graph = Some(WorkGraph {
        schema: WORK_GRAPH_SCHEMA_V1.into(),
        task_id: task.id,
        created_at: at.saturating_sub(2),
        deadline_at: at.saturating_sub(1),
        max_parallel_workers: 1,
        nodes: vec![WorkNode {
            id: node_id,
            task_id: task.id,
            idempotency_key: format!("{}:0:expired", task.id),
            deadline_at: at.saturating_sub(1),
            retry_budget: 1,
            attempts: 1,
            state: WorkNodeState::Running,
            dependency_policy: DependencyPolicy::AllSucceeded,
            request: WorkerRequest {
                worker_id: Id::new_v4(),
                node_id,
                task_id: task.id,
                objective: "expired work".into(),
                operation: WorkerOperation::Infer,
                context: ContextBundle {
                    task_id: task.id,
                    role: Role::Reasoner,
                    goal: "expired work".into(),
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
    store.save_task(&task).unwrap();
    drop(store);

    let runtime = Runtime::open(&path).unwrap();
    let recovered = runtime.store.task(task.id).unwrap();
    assert_eq!(recovered.status, TaskStatus::Failed);
    assert_eq!(
        recovered.work_graph.unwrap().nodes[0].state,
        WorkNodeState::Failed
    );
    drop(runtime);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn startup_recovery_runs_without_an_entered_tokio_handle() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let provider = Arc::new(assistant_core::testing::ScriptedProvider::new(
        false,
        vec![AgentAction::Respond {
            text: "recovered".into(),
        }],
    ));
    let assistant = Arc::new(Assistant::new(
        store.clone(),
        provider.clone(),
        provider,
        Arc::new(assistant_core::testing::EchoExecutor),
        EngineConfig::default(),
    ));
    let task = assistant
        .submit(UserInput {
            conversation_id: Id::new_v4(),
            text: "resume".into(),
            source: InputSource::Text,
        })
        .unwrap();

    launch_recovery(assistant, vec![task.id]).unwrap();

    for _ in 0..100 {
        let current = store.task(task.id).unwrap();
        if current.status.terminal() {
            assert_eq!(current.status, TaskStatus::Completed);
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("startup recovery did not finish");
}
