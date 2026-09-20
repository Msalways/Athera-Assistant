use super::*;
use crate::testing::{EchoExecutor, ScriptedProvider};

fn action(name: &str, arguments: Value) -> AgentAction {
    AgentAction::CallTool {
        call: ToolCall {
            tool_id: format!("sms.{name}"),
            version: "1".into(),
            arguments,
        },
    }
}
fn experiment(actions: Vec<AgentAction>) -> Experiment {
    Experiment::new(
        Arc::new(ScriptedProvider::new(true, actions)),
        Arc::new(EchoExecutor),
        Arc::new(storage_sqlite::SqliteStore::open(":memory:").unwrap()),
    )
}
#[test]
fn draft_prompt_directs_needle_to_the_single_tool() {
    assert_eq!(
        draft_prompt("Hey leave tomorrow take care of tasks"),
        "Use the only available tool. Set message to a clear SMS based on this wording: Hey leave tomorrow take care of tasks"
    );
}
#[test]
fn malformed_drafts_and_ambiguous_recipients_are_rejected() {
    for message in [
        json!({}),
        json!({"message":""}),
        json!({"message":"   "}),
        json!({"message":42}),
        json!({"message":"ok","send":true}),
    ] {
        assert!(decode_draft(action("propose_draft", message), &draft_tool()).is_err());
    }
    for recipient in [
        "Alice",
        "+15551234567,+15557654321",
        "123",
        "+1;5551234567",
        "１２３４５６７",
    ] {
        assert!(validate_recipient(recipient).is_err());
    }
    assert!(validate_recipient("+15551234567").is_ok());
}
#[tokio::test]
async fn approval_binds_exact_recipient_message_and_session() {
    let experiment = experiment(vec![action(
        "propose_draft",
        json!({"message":"See you soon."}),
    )]);
    experiment
        .draft("+15551234567".into(), "Be friendly".into())
        .await
        .unwrap();
    let state = experiment.snapshot();
    assert!(PolicyEngine::approve(&state, state.id, &state.recipient, &state.message).is_ok());
    assert!(PolicyEngine::approve(&state, Id::new_v4(), &state.recipient, &state.message).is_err());
    assert!(PolicyEngine::approve(&state, state.id, "+15557654321", &state.message).is_err());
    assert!(PolicyEngine::approve(&state, state.id, &state.recipient, "changed").is_err());
    experiment.stop();
    assert!(experiment.current(state.id).is_err());
    assert!(PolicyEngine::approve(
        &experiment.snapshot(),
        state.id,
        &state.recipient,
        &state.message
    )
    .is_err());
}
#[tokio::test]
async fn unchanged_wording_and_handoff_are_visible_without_fallback() {
    let experiment = experiment(vec![
        action("propose_draft", json!({"message":"Hi"})),
        AgentAction::Handoff {
            role: Role::Reasoner,
            objective: "help".into(),
            reason: "ambiguous".into(),
        },
    ]);
    experiment
        .draft("+15551234567".into(), "Hi".into())
        .await
        .unwrap();
    assert!(experiment.snapshot().detail.contains("unchanged"));
    assert!(experiment
        .draft("+15551234567".into(), "improve".into())
        .await
        .is_err());
    assert_eq!(experiment.snapshot().status, "failed");
    assert!(experiment.snapshot().message.is_empty());
}
#[test]
fn stale_targets_unapproved_text_and_arbitrary_clicks_are_denied() {
    let screen = json!({"revision":4,"elements":[{"id":1,"kind":"message"},{"id":2,"kind":"send"},{"id":3,"kind":"other"}]});
    let spec = operation("select_element");
    for arguments in [
        json!({"revision":3,"target":2}),
        json!({"revision":4,"target":9}),
        json!({"revision":4,"target":3}),
    ] {
        let AgentAction::CallTool { call } = action("select_element", arguments) else {
            unreachable!()
        };
        assert!(PolicyEngine::action(&spec, &call, &screen, "Approved").is_err());
    }
    let AgentAction::CallTool { call } = action(
        "enter_text",
        json!({"revision":4,"target":1,"text":"Changed"}),
    ) else {
        unreachable!()
    };
    assert!(PolicyEngine::action(&operation("enter_text"), &call, &screen, "Approved").is_err());
}

struct SendExecutor {
    sends: std::sync::atomic::AtomicUsize,
    denied: bool,
}
#[async_trait::async_trait]
impl ToolExecutor for SendExecutor {
    async fn execute(&self, spec: &ToolSpec, _: &ToolCall) -> Result<Value> {
        if self.denied {
            return Err(Error::Denied);
        }
        if spec.source_tool == "select_element" {
            self.sends.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            return Err(Error::OutcomeUnknown);
        }
        Ok(json!({"revision":4,"elements":[{"id":2,"kind":"send"}]}))
    }
}
#[tokio::test]
async fn permission_denial_and_uncertain_send_never_retry() {
    for denied in [false, true] {
        let executor = Arc::new(SendExecutor {
            sends: 0.into(),
            denied,
        });
        let store = Arc::new(storage_sqlite::SqliteStore::open(":memory:").unwrap());
        let experiment = Experiment::new(
            Arc::new(ScriptedProvider::new(
                true,
                vec![
                    action("propose_draft", json!({"message":"Hi"})),
                    action("select_element", json!({"revision":4,"target":2})),
                ],
            )),
            executor.clone(),
            store.clone(),
        );
        experiment
            .draft("+15551234567".into(), "Hi".into())
            .await
            .unwrap();
        let state = experiment.snapshot();
        assert!(experiment
            .approve(state.id, state.recipient.clone(), state.message.clone())
            .await
            .is_err());
        assert!(experiment
            .approve(state.id, state.recipient, state.message)
            .await
            .is_err());
        assert_eq!(
            executor.sends.load(std::sync::atomic::Ordering::SeqCst),
            usize::from(!denied)
        );
        assert_eq!(
            experiment.snapshot().status,
            if denied { "failed" } else { "outcome_unknown" }
        );
        if !denied {
            assert!(store.setting("sms_send_attempt").unwrap().is_some());
            let restarted = Experiment::new(
                Arc::new(ScriptedProvider::new(true, vec![])),
                executor,
                store,
            );
            assert_eq!(restarted.snapshot().status, "outcome_unknown");
        }
    }
}

struct SlowDraft;
#[async_trait::async_trait]
impl ModelProvider for SlowDraft {
    fn id(&self) -> &str {
        "slow-fixture"
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            local: true,
            tool_calls: true,
            planning: false,
        }
    }
    async fn infer(&self, _: ContextBundle) -> Result<AgentAction> {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok(action("propose_draft", json!({"message":"Late result"})))
    }
}
#[tokio::test]
async fn stop_discards_inflight_draft() {
    let experiment = Experiment::new(
        Arc::new(SlowDraft),
        Arc::new(EchoExecutor),
        Arc::new(storage_sqlite::SqliteStore::memory().unwrap()),
    );
    let draft = experiment.draft("+15551234567".into(), "Draft hello".into());
    let stop = async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        experiment.stop();
    };
    let (result, ()) = tokio::join!(draft, stop);
    assert!(result.is_err());
    assert_eq!(experiment.snapshot().status, "stopped");
    assert!(experiment.snapshot().message.is_empty());
}
