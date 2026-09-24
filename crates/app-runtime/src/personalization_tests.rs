use super::*;
use assistant_contracts::conversation::*;
use serde_json::json;
use std::sync::Mutex;

struct Capture(Arc<Mutex<Vec<ConversationRequest>>>);
#[async_trait::async_trait]
impl ConversationProvider for Capture {
    fn availability(&self) -> ProviderAvailability {
        ProviderAvailability::Ready
    }
    async fn generate(
        &self,
        request: ConversationRequest,
        _: Cancellation,
        sink: ConversationSink,
    ) -> Result<()> {
        self.0.lock().unwrap().push(request);
        sink(ConversationEvent::Delta {
            text: "Response".into(),
        });
        Ok(())
    }
    async fn unload(&self) -> Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn remembered_preference_reaches_chat_and_undo_has_immutable_history() {
    let mut runtime = Runtime::open(":memory:").unwrap();
    let captured = Arc::new(Mutex::new(vec![]));
    runtime.conversations = conversations::Conversations::new(
        runtime.store.clone(),
        Arc::new(Capture(captured.clone())),
    );
    let saved = runtime
        .dispatch(
            "remember_preference",
            json!({"instruction":"Use concise answers","confirmed":true}),
        )
        .await
        .unwrap();
    let id = Id::new_v4();
    let message = runtime
        .dispatch("send_message", json!({"conversation_id":id,"text":"Hello"}))
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while runtime.conversations.busy() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        captured.lock().unwrap()[0].personal.rules[0].instruction,
        "Use concise answers"
    );
    let used = runtime
        .dispatch("personal_usage", json!({"run_id":message["id"]}))
        .await
        .unwrap();
    assert_eq!(used[0]["rule"]["version"], 2);
    assert_eq!(
        runtime
            .dispatch(
                "disable_adaptive_rule",
                json!({"rule_id":saved["rule"]["id"],"expected_version":1,"confirmed":true})
            )
            .await,
        Err(Error::Conflict)
    );
    runtime
        .dispatch(
            "disable_adaptive_rule",
            json!({"rule_id":saved["rule"]["id"],"expected_version":2,"confirmed":true}),
        )
        .await
        .unwrap();
    let history = runtime
        .dispatch("rule_history", json!({"rule_id":saved["rule"]["id"]}))
        .await
        .unwrap();
    assert_eq!(history.as_array().unwrap().len(), 3);
    runtime.dispatch("rollback_rule",json!({"rule_id":saved["rule"]["id"],"expected_version":3,"version":2,"confirmed":true})).await.unwrap();
    let temporary = Id::new_v4();
    let temp = runtime
        .dispatch(
            "send_message",
            json!({"conversation_id":temporary,"text":"Hello","temporary":true}),
        )
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while runtime.conversations.busy() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(captured.lock().unwrap()[1].personal.rules.is_empty());
    assert_eq!(runtime.dispatch("record_observation",json!({"conversation_id":temporary,"source_id":temp["id"],"kind":"correction","text":"Be concise"})).await,Err(Error::Denied));
}

#[tokio::test]
async fn learning_ingress_rejects_known_secrets_and_unconfirmed_remember() {
    let runtime = Runtime::open(":memory:").unwrap();
    let config:CloudConfig=serde_json::from_value(json!({"id":"test","endpoint":"https://example.com/v1","model":"test","secret_ref":"ASSISTANT_TEST_KEY","api":"responses"})).unwrap();
    runtime
        .secrets
        .set(&config, "fixture-secret-123456".into())
        .unwrap();
    assert_eq!(
        runtime
            .dispatch(
                "propose_rule",
                json!({"instruction":"Use fixture-secret-123456","rationale":"remember"})
            )
            .await,
        Err(Error::Denied)
    );
    assert!(runtime.store.rule_proposals(None, 100).unwrap().is_empty());
    let conversation = Id::new_v4();
    assert_eq!(runtime.dispatch("record_observation",json!({"conversation_id":conversation,"source_id":Id::new_v4(),"kind":"remember","text":"Save this"})).await,Err(Error::Denied));
    assert!(runtime.store.pending_observations(100).unwrap().is_empty());
}

#[test]
fn deleting_source_conversation_forgets_derived_preferences() {
    let store = SqliteStore::memory().unwrap();
    let id = Id::new_v4();
    store
        .create_conversation(&Conversation {
            id,
            title: "test".into(),
            temporary: false,
            summary: String::new(),
            updated_at: 1,
        })
        .unwrap();
    let message = Message {
        id: Id::new_v4(),
        conversation_id: id,
        role: MessageRole::User,
        content: "Remember my preference".into(),
        status: MessageStatus::Complete,
        created_at: 1,
    };
    store.append_message(&message).unwrap();
    let observation = store
        .record_observation(&Observation {
            id: Id::new_v4(),
            conversation_id: id,
            source_id: message.id,
            kind: ObservationKind::Remember,
            text: "Use concise answers".into(),
            created_at: 1,
        })
        .unwrap();
    let proposal = assistant_core::personalization::PersonalizationService::new(&store).proposal(
        observation.text.clone(),
        "Explicit preference".into(),
        RuleScope::Conversation(id),
        RuleSource::User,
        vec![observation.id],
    );
    store
        .propose_personal_rule(&proposal, Some(observation.id), None)
        .unwrap();
    store
        .decide_personal_rule(
            proposal.rule.id,
            1,
            RuleDecision::Activate,
            assistant_core::personalization::now(),
        )
        .unwrap();
    store.delete_conversation(id).unwrap();
    assert!(store.observation(observation.id).is_err());
    assert!(store.adaptive_rule(proposal.rule.id).is_err());
    assert!(store.rule_history(proposal.rule.id).unwrap().is_empty());
}
