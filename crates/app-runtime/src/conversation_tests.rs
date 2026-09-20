use super::*;
use assistant_contracts::conversation::*;
use async_trait::async_trait;
use std::sync::atomic::Ordering;

struct LocalFixture;
#[async_trait]
impl ConversationProvider for LocalFixture {
    fn availability(&self) -> ProviderAvailability {
        ProviderAvailability::Ready
    }
    async fn generate(
        &self,
        request: ConversationRequest,
        cancel: Cancellation,
        sink: ConversationSink,
    ) -> Result<()> {
        assert!(!request.messages.is_empty());
        sink(ConversationEvent::Delta {
            text: "Local ".into(),
        });
        tokio::task::yield_now().await;
        if !cancel.load(Ordering::Acquire) {
            sink(ConversationEvent::Delta {
                text: "response".into(),
            });
        }
        Ok(())
    }
    async fn unload(&self) -> Result<()> {
        Ok(())
    }
}

fn fixture() -> Runtime {
    let mut runtime = Runtime::open(":memory:").unwrap();
    runtime.conversations =
        conversations::Conversations::new(runtime.store.clone(), Arc::new(LocalFixture));
    runtime
}
async fn settle(runtime: &Runtime) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while runtime.conversations.busy() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn local_chat_streams_persists_and_never_creates_an_action_task() {
    let runtime = fixture();
    let conversation_id = Id::new_v4();
    let message = runtime
        .dispatch(
            "send_message",
            serde_json::json!({"conversation_id":conversation_id,"text":"Hello"}),
        )
        .await
        .unwrap();
    assert_eq!(message["status"], "generating");
    settle(&runtime).await;
    let saved = runtime
        .dispatch(
            "get_conversation",
            serde_json::json!({"conversation_id":conversation_id}),
        )
        .await
        .unwrap();
    assert_eq!(saved["messages"][1]["content"], "Local response");
    assert_eq!(saved["messages"][1]["status"], "complete");
    assert!(runtime.store.tasks().unwrap().is_empty());
    assert!(runtime.store.events(0).unwrap().is_empty());
}

#[tokio::test]
async fn temporary_chat_stays_out_of_sqlite_and_cancellation_discards_late_tokens() {
    let runtime = fixture();
    let conversation_id = Id::new_v4();
    runtime
        .dispatch(
            "send_message",
            serde_json::json!({"conversation_id":conversation_id,"text":"Hello","temporary":true}),
        )
        .await
        .unwrap();
    runtime
        .dispatch(
            "cancel_message",
            serde_json::json!({"conversation_id":conversation_id}),
        )
        .await
        .unwrap();
    settle(&runtime).await;
    assert!(runtime.store.list_conversations(100).unwrap().is_empty());
    let saved = runtime
        .dispatch(
            "get_conversation",
            serde_json::json!({"conversation_id":conversation_id}),
        )
        .await
        .unwrap();
    assert_eq!(saved["messages"][1]["status"], "cancelled");
    assert_eq!(saved["messages"][1]["content"], "");
    runtime
        .dispatch(
            "delete_conversation",
            serde_json::json!({"conversation_id":conversation_id}),
        )
        .await
        .unwrap();
    assert!(runtime
        .dispatch("list_conversations", serde_json::json!({}))
        .await
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn missing_model_never_falls_back_to_cloud_or_persists_a_turn() {
    let runtime = Runtime::open(":memory:").unwrap();
    assert_eq!(
        runtime
            .dispatch(
                "send_message",
                serde_json::json!({"conversation_id":Id::new_v4(),"text":"Hello"})
            )
            .await,
        Err(Error::Unavailable)
    );
    assert!(runtime.store.list_conversations(100).unwrap().is_empty());
    assert!(runtime.store.tasks().unwrap().is_empty());
}

#[tokio::test]
async fn memory_requires_confirmation_and_rejects_known_credentials() {
    let runtime = fixture();
    assert_eq!(
        runtime
            .dispatch("save_memory", serde_json::json!({"text":"I like tea"}))
            .await,
        Err(Error::Denied)
    );
    let memory = runtime
        .dispatch(
            "save_memory",
            serde_json::json!({"text":"I like tea","confirmed":true}),
        )
        .await
        .unwrap();
    runtime
        .dispatch("delete_memory", serde_json::json!({"id":memory["id"]}))
        .await
        .unwrap();
    assert!(runtime.store.list_memories(100).unwrap().is_empty());
    let cloud = provider_cloud::CloudConfig {
        id: "fixture".into(),
        endpoint: "https://example.com/v1".into(),
        model: "fixture".into(),
        secret_ref: "ASSISTANT_FIXTURE_KEY".into(),
        api: provider_cloud::ApiKind::ChatCompletions,
        max_output_tokens: 1024,
    };
    let secret = "fixture\\key\"only";
    runtime
        .dispatch(
            "save_cloud_provider",
            serde_json::json!({"cloud":cloud,"api_key":secret}),
        )
        .await
        .unwrap();
    for command in ["save_memory", "send_message", "submit_input"] {
        assert_eq!(runtime.dispatch(command, serde_json::json!({"text":secret,"confirmed":true,"conversation_id":Id::new_v4(),"source":"text"})).await, Err(Error::Denied));
    }
    assert!(runtime.store.list_memories(100).unwrap().is_empty());
}

#[tokio::test]
async fn research_notes_are_versioned_and_sources_are_explicit() {
    let runtime = fixture();
    let conversation_id = Id::new_v4();
    runtime
        .dispatch(
            "send_message",
            serde_json::json!({"conversation_id":conversation_id,"text":"Research"}),
        )
        .await
        .unwrap();
    settle(&runtime).await;
    let session = runtime
        .dispatch(
            "create_research",
            serde_json::json!({"conversation_id":conversation_id,"title":"Compare hypotheses"}),
        )
        .await
        .unwrap();
    assert!(session["sources"].as_array().unwrap().is_empty());
    for (i, text) in ["Initial hypothesis", "Revised after evidence"]
        .iter()
        .enumerate()
    {
        let note = runtime
            .dispatch(
                "append_research_note",
                serde_json::json!({"id":session["id"],"text":text}),
            )
            .await
            .unwrap();
        assert_eq!(note["revision"], i + 1);
    }
    assert_eq!(runtime.dispatch("add_research_source", serde_json::json!({"id":session["id"],"title":"Unsafe link","url":"javascript:alert(1)","excerpt":"data"})).await, Err(Error::InvalidInput));
}
