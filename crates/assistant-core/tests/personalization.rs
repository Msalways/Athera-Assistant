use assistant_contracts::*;
use assistant_core::{
    personalization::{now, PersonalizationService},
    registry,
    testing::{example_tool, EchoExecutor, ScriptedProvider},
    Assistant,
};
use std::sync::Arc;
use storage_sqlite::SqliteStore;

fn evidence(store: &SqliteStore) -> Observation {
    let mut task = Task::new(UserInput {
        conversation_id: Id::new_v4(),
        text: "echo hello".into(),
        source: InputSource::Text,
    });
    task.status = TaskStatus::Completed;
    store.save_task(&task).unwrap();
    store
        .record_observation(&Observation {
            id: Id::new_v4(),
            conversation_id: task.input.conversation_id,
            source_id: task.id,
            kind: ObservationKind::Correction,
            text: "Use concise answers".into(),
            created_at: now(),
        })
        .unwrap()
}

fn proposal(store: &SqliteStore, evidence: &Observation) -> RuleProposal {
    PersonalizationService::new(store).proposal(
        "Use concise answers".into(),
        "User correction".into(),
        RuleScope::Conversation(evidence.conversation_id),
        RuleSource::Model,
        vec![evidence.id],
    )
}

#[test]
fn concurrent_reviews_have_one_winner() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let evidence = evidence(&store);
    let proposal = proposal(&store, &evidence);
    store.propose_personal_rule(&proposal, None, None).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let handles = [RuleDecision::Activate, RuleDecision::Reject]
        .into_iter()
        .map(|decision| {
            let store = store.clone();
            let barrier = barrier.clone();
            let id = proposal.rule.id;
            std::thread::spawn(move || {
                barrier.wait();
                store.decide_personal_rule(id, 1, decision, now())
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(Error::Conflict)))
            .count(),
        1
    );
    assert_eq!(store.rule_history(proposal.rule.id).unwrap().len(), 2);
}

#[test]
fn atomic_proposal_review_replacement_and_rollback() {
    let store = SqliteStore::memory().unwrap();
    let evidence = evidence(&store);
    let first = proposal(&store, &evidence);
    store
        .propose_personal_rule(&first, Some(evidence.id), None)
        .unwrap();
    // Retry uses the original identity, even if the model drafted a new ID.
    assert_eq!(
        store
            .propose_personal_rule(&proposal(&store, &evidence), Some(evidence.id), None)
            .unwrap()
            .id,
        first.id
    );
    let active = store
        .decide_personal_rule(first.rule.id, 1, RuleDecision::Activate, now())
        .unwrap();
    assert_eq!(
        store
            .decide_personal_rule(first.rule.id, 1, RuleDecision::Reject, now())
            .unwrap_err(),
        Error::Conflict
    );
    let mut replacement = proposal(&store, &evidence);
    replacement.rule.instruction = "Use detailed answers".into();
    replacement.rule.supersedes = Some(active.id);
    store
        .propose_personal_rule(&replacement, None, None)
        .unwrap();
    store
        .decide_personal_rule(replacement.rule.id, 1, RuleDecision::Activate, now())
        .unwrap();
    assert_eq!(
        store.rule_proposal(first.id).unwrap().rule.status,
        AdaptiveRuleStatus::Disabled
    );
    let old = store.adaptive_rule(first.rule.id).unwrap();
    store
        .decide_personal_rule(
            old.id,
            old.version,
            RuleDecision::Rollback { version: 2 },
            now(),
        )
        .unwrap();
    assert_eq!(
        store.adaptive_rule(replacement.rule.id).unwrap().status,
        AdaptiveRuleStatus::Disabled
    );
    assert_eq!(store.rule_history(first.rule.id).unwrap().len(), 4);
    assert_eq!(
        store
            .personal_rules(evidence.conversation_id, None, now())
            .unwrap()
            .len(),
        1
    );
    // Fail after saving a revision but before committing the duplicate proposal.
    let mut duplicate = proposal(&store, &evidence);
    duplicate.id = first.id;
    duplicate.rule.instruction = "Different rule to exercise transaction failure".into();
    assert!(store.propose_personal_rule(&duplicate, None, None).is_err());
    assert!(store.adaptive_rule(duplicate.rule.id).is_err());
    assert!(store.rule_history(duplicate.rule.id).unwrap().is_empty());
}

#[test]
fn scope_before_limit_expiration_precedence_and_temporary_privacy() {
    let store = SqliteStore::memory().unwrap();
    let evidence = evidence(&store);
    for _ in 0..40 {
        let mut other = proposal(&store, &evidence);
        other.rule.source = RuleSource::User;
        other.rule.evidence_ids.clear();
        other.rule.scope = RuleScope::Conversation(Id::new_v4());
        other.rule.priority = 255;
        store.propose_personal_rule(&other, None, None).unwrap();
        store
            .decide_personal_rule(other.rule.id, 1, RuleDecision::Activate, now())
            .unwrap();
    }
    let mut global = proposal(&store, &evidence);
    global.rule.scope = RuleScope::Global;
    global.rule.preference_key = Some("response.length".into());
    store.propose_personal_rule(&global, None, None).unwrap();
    store
        .decide_personal_rule(global.rule.id, 1, RuleDecision::Activate, now())
        .unwrap();
    let mut scoped = proposal(&store, &evidence);
    scoped.rule.preference_key = global.rule.preference_key.clone();
    store.propose_personal_rule(&scoped, None, None).unwrap();
    store
        .decide_personal_rule(scoped.rule.id, 1, RuleDecision::Activate, now())
        .unwrap();
    let personal = PersonalizationService::new(&store);
    let selected = personal
        .context(evidence.conversation_id, None, "hello", false)
        .unwrap();
    assert_eq!(selected.rules.len(), 1);
    assert_eq!(selected.rules[0].id, scoped.rule.id);
    assert!(personal
        .context(evidence.conversation_id, None, "hello", true)
        .unwrap()
        .rules
        .is_empty());
    let mut unavailable = evidence.clone();
    unavailable.id = Id::new_v4();
    unavailable.source_id = Id::new_v4();
    assert_eq!(
        store.record_observation(&unavailable).unwrap_err(),
        Error::Denied
    );
    let mut conflict = proposal(&store, &evidence);
    conflict.rule.preference_key = scoped.rule.preference_key.clone();
    conflict.rule.instruction = "Use detailed answers".into();
    store.propose_personal_rule(&conflict, None, None).unwrap();
    assert_eq!(
        store
            .decide_personal_rule(conflict.rule.id, 1, RuleDecision::Activate, now())
            .unwrap_err(),
        Error::Conflict
    );
    let mut replacement = proposal(&store, &evidence);
    replacement.rule.preference_key = scoped.rule.preference_key.clone();
    replacement.rule.instruction = "Use brief answers".into();
    replacement.rule.supersedes = Some(scoped.rule.id);
    store
        .propose_personal_rule(&replacement, None, None)
        .unwrap();
    store
        .decide_personal_rule(replacement.rule.id, 1, RuleDecision::Activate, now())
        .unwrap();
    assert_eq!(
        store.adaptive_rule(scoped.rule.id).unwrap().status,
        AdaptiveRuleStatus::Disabled
    );
    let mut expired = proposal(&store, &evidence);
    expired.rule.expires_at = Some(expired.rule.created_at + 1);
    store.propose_personal_rule(&expired, None, None).unwrap();
    assert_eq!(
        store
            .decide_personal_rule(
                expired.rule.id,
                1,
                RuleDecision::Activate,
                expired.rule.created_at + 2
            )
            .unwrap_err(),
        Error::InvalidInput
    );
}

#[tokio::test]
async fn proposal_never_finishes_foreground_task_and_usage_is_traceable() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let evidence = evidence(&store);
    let draft = proposal(&store, &evidence);
    let provider = Arc::new(ScriptedProvider::new(
        true,
        vec![
            AgentAction::ProposeRule { proposal: draft },
            AgentAction::Respond {
                text: "Original task finished".into(),
            },
        ],
    ));
    let assistant = Assistant::new(
        store.clone(),
        provider.clone(),
        provider,
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    );
    let task = assistant
        .submit(UserInput {
            conversation_id: evidence.conversation_id,
            text: "Continue the original request".into(),
            source: InputSource::Text,
        })
        .unwrap();
    let finished = assistant.run(task.id).await.unwrap();
    assert_eq!(finished.status, TaskStatus::Completed);
    assert_eq!(finished.message, "Original task finished");
    assert_eq!(finished.step, 2);
    let saved = store.rule_proposals(None, 10).unwrap().remove(0);
    assert_eq!(saved.rule.status, AdaptiveRuleStatus::Proposed);
    store
        .decide_personal_rule(saved.rule.id, 1, RuleDecision::Activate, now())
        .unwrap();
    let provider = Arc::new(ScriptedProvider::new(
        true,
        vec![AgentAction::Respond {
            text: "Used preference".into(),
        }],
    ));
    let assistant = Assistant::new(
        store.clone(),
        provider.clone(),
        provider,
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    );
    let task = assistant
        .submit(UserInput {
            conversation_id: evidence.conversation_id,
            text: "Next request".into(),
            source: InputSource::Text,
        })
        .unwrap();
    assistant.run(task.id).await.unwrap();
    assert_eq!(store.personal_usage(task.id).unwrap()[0].rule.version, 2);
    store
        .decide_personal_rule(saved.rule.id, 2, RuleDecision::Disable, now())
        .unwrap();
    assert_eq!(
        store.personal_usage(task.id).unwrap()[0].rule.status,
        AdaptiveRuleStatus::Enabled
    );
}

#[tokio::test]
async fn learning_is_durable_deduplicated_and_guarded() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let evidence = evidence(&store);
    let provider = Arc::new(ScriptedProvider::new(
        false,
        vec![AgentAction::ProposeRule {
            proposal: proposal(&store, &evidence),
        }],
    ));
    let assistant = Assistant::new(
        store.clone(),
        provider.clone(),
        provider,
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    );
    assert_eq!(assistant.learn_pending().await.unwrap().len(), 1);
    assert!(assistant.learn_pending().await.unwrap().is_empty());
    assert!(store
        .personal_rules(evidence.conversation_id, None, now())
        .unwrap()
        .is_empty());
    let mut secret = proposal(&store, &evidence);
    secret.rule.instruction = "known-secret".into();
    let guard: assistant_core::personalization::PersonalDataGuard = Arc::new(|value| {
        if value.to_string().contains("known-secret") {
            Err(Error::Denied)
        } else {
            Ok(())
        }
    });
    assert_eq!(
        PersonalizationService::new(store.as_ref())
            .propose_model(secret, None, &guard)
            .unwrap_err(),
        Error::Denied
    );
    assert_eq!(store.rule_proposals(None, 10).unwrap().len(), 1);
}

#[tokio::test]
async fn skill_replay_gates_activation_and_dependency_changes_disable_it() {
    let store = Arc::new(SqliteStore::memory().unwrap());
    let evidence = evidence(&store);
    let tool = example_tool(Risk::ExternalWrite);
    registry::register(store.as_ref(), &Capability::Tool(tool.clone())).unwrap();
    let spec = SkillSpec {
        id: String::new(),
        version: "1".into(),
        name: "Echo workflow".into(),
        description: "Echo hello".into(),
        instructions: "Echo hello with the supplied tool".into(),
        tool_requirements: vec![tool.id.clone()],
        enabled: false,
    };
    let guard: assistant_core::personalization::PersonalDataGuard = Arc::new(|_| Ok(()));
    let draft = PersonalizationService::new(store.as_ref())
        .propose_skill(spec, "Recorded user correction".into(), evidence.id, &guard)
        .unwrap();
    assert_eq!(
        store
            .decide_personal_rule(draft.rule.id, 1, RuleDecision::Activate, now())
            .unwrap_err(),
        Error::Denied
    );
    let call = ToolCall {
        tool_id: tool.id.clone(),
        version: tool.version.clone(),
        arguments: serde_json::json!({"text":"hello"}),
    };
    let actions = vec![
        AgentAction::CallTool { call: call.clone() },
        AgentAction::Respond {
            text: "Done".into(),
        },
        AgentAction::CallTool { call: call.clone() },
        AgentAction::Respond {
            text: "Done".into(),
        },
    ];
    let provider = Arc::new(ScriptedProvider::new(false, actions));
    let assistant = Assistant::new(
        store.clone(),
        provider.clone(),
        provider,
        Arc::new(EchoExecutor),
        EngineConfig::default(),
    );
    let report = assistant
        .evaluate_generated_skill(
            draft.rule.id,
            1,
            &[SkillCase {
                observation_id: evidence.id,
                expected: vec![call],
                simulated_results: vec![serde_json::json!({"ok":true})],
            }],
        )
        .await
        .unwrap();
    assert!(report.passed);
    assert_eq!(report.baseline_matches, 1);
    store
        .decide_personal_rule(draft.rule.id, 1, RuleDecision::Activate, now())
        .unwrap();
    let skill_id = format!("personal:{}", draft.rule.id);
    assert!(registry::activate(store.as_ref(), &skill_id).is_ok());
    let mut changed = tool;
    changed.enabled = false;
    registry::register(store.as_ref(), &Capability::Tool(changed)).unwrap();
    assert_eq!(
        registry::activate(store.as_ref(), &skill_id).unwrap_err(),
        Error::Conflict
    );
    assert_eq!(
        store.adaptive_rule(draft.rule.id).unwrap().status,
        AdaptiveRuleStatus::Disabled
    );
    assert!(store
        .tasks()
        .unwrap()
        .iter()
        .all(|t| t.status == TaskStatus::Completed));
}
