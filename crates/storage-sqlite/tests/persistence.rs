use assistant_contracts::conversation::{
    Conversation, Message, MessageRole, MessageStatus, NoteRevision, PersonalMemory,
    ResearchSession, SourceReference,
};
use assistant_contracts::*;
use rusqlite::Connection;
use serde_json::json;
use storage_sqlite::SqliteStore;
#[test]
fn migrates_legacy_preferences_once_and_marks_missing_history() {
    let path =
        std::env::temp_dir().join(format!("assistant-personal-migration-{}.db", Id::new_v4()));
    let id = Id::new_v4();
    let proposal_id = Id::new_v4();
    let rule = json!({"schema":ADAPTIVE_RULE_SCHEMA_V1,"id":id,"version":3,"scope":"global","status":"disabled","source":"user","priority":50,"instruction":"Be concise","evidence_ids":[],"created_at":1700000000_u64,"updated_at":1700000010_u64,"expires_at":null,"supersedes":null});
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(include_str!("../../../migrations/001_initial.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../../migrations/005_adaptive_rules.sql"))
            .unwrap();
        conn.execute("INSERT INTO adaptive_rules(id,status,scope,updated_at,data) VALUES(?1,'\"disabled\"','\"global\"',1700000010,?2)",rusqlite::params![id.to_string(),rule.to_string()]).unwrap();
        let mut stale = rule.clone();
        stale["status"] = json!("enabled");
        let proposal = json!({"schema":RULE_PROPOSAL_SCHEMA_V1,"id":proposal_id,"rule":stale,"rationale":"Legacy preference","proposed_at":1700000000_u64});
        conn.execute(
            "INSERT INTO rule_proposals VALUES(?1,'\"enabled\"',1700000000,?2)",
            rusqlite::params![proposal_id.to_string(), proposal.to_string()],
        )
        .unwrap();
    }
    for _ in 0..2 {
        let store = SqliteStore::open(&path).unwrap();
        assert_eq!(store.adaptive_rule(id).unwrap().created_at, 1700000000000);
        let revisions = store.rule_history(id).unwrap();
        assert_eq!(revisions.len(), 1);
        assert!(revisions[0].historical_baseline);
        assert_eq!(
            store.rule_proposal(proposal_id).unwrap().rule.status,
            AdaptiveRuleStatus::Disabled
        );
    }
    std::fs::remove_file(path).unwrap();
}
#[test]
fn reopens_tasks_results_and_settings_with_ordered_events() {
    let path = std::env::temp_dir().join(format!("assistant-test-{}.db", Id::new_v4()));
    let task = Task::new(UserInput {
        conversation_id: Id::new_v4(),
        text: "Test".into(),
        source: InputSource::Text,
    });
    let result = Id::new_v4();
    {
        let store = SqliteStore::open(&path).unwrap();
        store.save_task(&task).unwrap();
        store.save_result(result, &json!({"value":42})).unwrap();
        store.set_setting("theme", &json!("system")).unwrap();
    }
    {
        let store = SqliteStore::open(&path).unwrap();
        assert_eq!(store.task(task.id).unwrap().input.text, "Test");
        assert_eq!(store.result(result).unwrap(), json!({"value":42}));
        assert_eq!(store.setting("theme").unwrap(), Some(json!("system")));
        let events = store.events(0).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, RunEventKind::RunStarted);
        assert_eq!(events[0].schema, RUN_EVENT_SCHEMA_V1);
        assert_eq!(events[0].event_id, format!("{}:1", task.id));
        assert!(store.events(1).unwrap().is_empty());
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn unfinished_task_enumeration_is_not_limited_to_the_recent_snapshot() {
    let store = SqliteStore::memory().unwrap();
    let unfinished = Task::new(UserInput {
        conversation_id: Id::new_v4(),
        text: "recover me".into(),
        source: InputSource::Text,
    });
    store.save_task(&unfinished).unwrap();
    for index in 0..201 {
        let mut completed = Task::new(UserInput {
            conversation_id: Id::new_v4(),
            text: format!("completed {index}"),
            source: InputSource::Text,
        });
        completed.status = TaskStatus::Completed;
        completed.message = "done".into();
        completed.output = Some(AssistantOutput::markdown("done"));
        store.save_task(&completed).unwrap();
    }

    assert!(!store
        .tasks()
        .unwrap()
        .iter()
        .any(|task| task.id == unfinished.id));
    assert_eq!(
        store
            .unfinished_tasks()
            .unwrap()
            .into_iter()
            .map(|task| task.id)
            .collect::<Vec<_>>(),
        vec![unfinished.id]
    );
}

#[test]
fn migrates_an_existing_database_without_losing_tasks_or_settings() {
    let path = std::env::temp_dir().join(format!("assistant-migration-{}.db", Id::new_v4()));
    let task = Task::new(UserInput {
        conversation_id: Id::new_v4(),
        text: "Preserve me".into(),
        source: InputSource::Text,
    });
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(include_str!("../../../migrations/001_initial.sql"))
            .unwrap();
        conn.execute(
            "INSERT INTO tasks(id,conversation_id,data) VALUES(?1,?2,?3)",
            rusqlite::params![
                task.id.to_string(),
                task.input.conversation_id.to_string(),
                serde_json::to_string(&task).unwrap()
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO settings(key,data) VALUES('theme','\"dark\"')",
            [],
        )
        .unwrap();
    }
    let store = SqliteStore::open(&path).unwrap();
    assert_eq!(store.task(task.id).unwrap().input.text, "Preserve me");
    assert_eq!(store.setting("theme").unwrap(), Some(json!("dark")));
    drop(store);
    let conn = Connection::open(&path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version IN (1,2,3,4)",
            [],
            |row| row.get::<_, u32>(0)
        )
        .unwrap(),
        4
    );
    drop(conn);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn completed_output_has_one_upsert_and_one_terminal_event() {
    let store = SqliteStore::memory().unwrap();
    let mut task = Task::new(UserInput {
        conversation_id: Id::new_v4(),
        text: "Show a table".into(),
        source: InputSource::Text,
    });
    store.save_task(&task).unwrap();
    task.status = TaskStatus::Completed;
    task.message = "Name | Value\nA | 1".into();
    task.output = Some(AssistantOutput {
        schema: OUTPUT_SCHEMA_V1.into(),
        blocks: vec![OutputBlock::Table {
            id: "result".into(),
            columns: vec!["Name".into(), "Value".into()],
            rows: vec![vec!["A".into(), "1".into()]],
        }],
    });
    store.save_task(&task).unwrap();
    store.save_task(&task).unwrap();

    let first = store.events(0).unwrap();
    assert_eq!(first.len(), 3);
    assert_eq!(
        first
            .iter()
            .filter(|event| event.kind == RunEventKind::RunTerminal)
            .count(),
        1
    );
    assert_eq!(first[1].kind, RunEventKind::OutputUpsert);
    assert_eq!(first[1].output, task.output);
    assert_eq!(store.events(first[1].sequence).unwrap()[0], first[2]);
    assert_eq!(store.events(0).unwrap(), first);
}

fn conversation(id: Id, updated_at: u64) -> Conversation {
    Conversation {
        id,
        title: "Conversation".into(),
        temporary: false,
        summary: "Summary".into(),
        updated_at,
    }
}

fn message(id: Id, conversation_id: Id, content: &str, status: MessageStatus) -> Message {
    Message {
        id,
        conversation_id,
        role: MessageRole::Assistant,
        content: content.into(),
        status,
        created_at: 10,
    }
}

#[test]
fn persists_conversations_and_messages_in_insertion_order_and_recovers_generation() {
    let path = std::env::temp_dir().join(format!("assistant-conversations-{}.db", Id::new_v4()));
    let conversation_id = Id::new_v4();
    let first = message(
        Id::new_v4(),
        conversation_id,
        "first",
        MessageStatus::Complete,
    );
    let second = message(
        Id::new_v4(),
        conversation_id,
        "second",
        MessageStatus::Generating,
    );
    {
        let store = SqliteStore::open(&path).unwrap();
        store
            .create_conversation(&conversation(conversation_id, 1))
            .unwrap();
        store.append_message(&first).unwrap();
        store.append_message(&second).unwrap();
        assert_eq!(
            store.list_messages(conversation_id, 1).unwrap()[0].content,
            "second"
        );
        let mut changed = first.clone();
        changed.content = "updated".into();
        assert!(store.update_message(&changed).unwrap());
        let wrong = message(
            Id::new_v4(),
            Id::new_v4(),
            "orphan",
            MessageStatus::Complete,
        );
        assert_eq!(store.append_message(&wrong), Err(Error::InvalidInput));
    }
    {
        let store = SqliteStore::open(&path).unwrap();
        let messages = store.list_messages(conversation_id, 10).unwrap();
        assert_eq!(
            messages
                .iter()
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>(),
            vec!["updated", "second"]
        );
        assert_eq!(messages[1].status, MessageStatus::Interrupted);
        assert!(store.delete_conversation(conversation_id).unwrap());
        assert!(store.list_messages(conversation_id, 10).unwrap().is_empty());
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn temporary_conversations_never_enter_sqlite() {
    let store = SqliteStore::memory().unwrap();
    let id = Id::new_v4();
    let mut temporary = conversation(id, 1);
    temporary.temporary = true;
    assert_eq!(
        store.create_conversation(&temporary),
        Err(Error::InvalidInput)
    );
    assert_eq!(
        store.upsert_conversation(&temporary),
        Err(Error::InvalidInput)
    );
    assert!(store.get_conversation(id).unwrap().is_none());
}

#[test]
fn memory_crud_is_explicit_and_bounded() {
    let store = SqliteStore::memory().unwrap();
    let id = Id::new_v4();
    let mut memory = PersonalMemory {
        id,
        text: "tea".into(),
        updated_at: 1,
    };
    store.create_memory(&memory).unwrap();
    memory.text = "coffee".into();
    memory.updated_at = 2;
    store.upsert_memory(&memory).unwrap();
    assert_eq!(store.get_memory(id).unwrap().unwrap().text, "coffee");
    assert_eq!(store.list_memories(1).unwrap().len(), 1);
    assert!(store.delete_memory(id).unwrap());
    assert!(store.get_memory(id).unwrap().is_none());
}

#[test]
fn research_sessions_keep_immutable_ordered_note_revisions() {
    let store = SqliteStore::memory().unwrap();
    let conversation_id = Id::new_v4();
    store
        .create_conversation(&conversation(conversation_id, 1))
        .unwrap();
    let session_id = Id::new_v4();
    let mut session = ResearchSession {
        id: session_id,
        conversation_id,
        title: "Research".into(),
        sources: vec![SourceReference {
            id: Id::new_v4(),
            title: "Source".into(),
            url: Some("https://example.com".into()),
            excerpt: "Excerpt".into(),
        }],
        hypotheses: vec!["Hypothesis".into()],
        decisions: vec![],
        experiments: vec![],
        notes: vec![NoteRevision {
            revision: 1,
            text: "one".into(),
            created_at: 1,
        }],
    };
    let mut orphan = session.clone();
    orphan.id = Id::new_v4();
    orphan.conversation_id = Id::new_v4();
    assert_eq!(
        store.create_research_session(&orphan),
        Err(Error::InvalidInput)
    );
    store.create_research_session(&session).unwrap();
    let note = store.append_research_note(session_id, "two", 2).unwrap();
    assert_eq!(note.revision, 2);
    session.title = "Updated".into();
    session.notes[0].text = "attempted overwrite".into();
    store.upsert_research_session(&session).unwrap();
    let loaded = store.get_research_session(session_id).unwrap().unwrap();
    assert_eq!(loaded.title, "Updated");
    assert_eq!(
        loaded
            .notes
            .iter()
            .map(|n| n.text.as_str())
            .collect::<Vec<_>>(),
        vec!["one", "two"]
    );
    assert_eq!(
        store
            .list_research_sessions(conversation_id, 1)
            .unwrap()
            .len(),
        1
    );
    assert!(store.delete_research_session(session_id).unwrap());
}

#[test]
fn adaptive_rules_are_versioned_bounded_and_user_transitioned() {
    let store = SqliteStore::memory().unwrap();
    let id = Id::new_v4();
    let mut rule = AdaptiveRule {
        schema: ADAPTIVE_RULE_SCHEMA_V1.into(),
        id,
        version: 1,
        scope: RuleScope::Global,
        status: AdaptiveRuleStatus::Proposed,
        source: RuleSource::Model,
        priority: 20,
        instruction: "Prefer concise answers".into(),
        evidence_ids: vec![Id::new_v4()],
        created_at: 1,
        updated_at: 1,
        expires_at: None,
        supersedes: None,
        preference_key: None,
    };
    let proposal = RuleProposal {
        schema: RULE_PROPOSAL_SCHEMA_V1.into(),
        id: Id::new_v4(),
        rule: rule.clone(),
        rationale: "Repeated user corrections".into(),
        proposed_at: 2,
    };
    store.put_rule_proposal(&proposal).unwrap();
    assert_eq!(
        store
            .rule_proposals(Some(AdaptiveRuleStatus::Proposed), 5)
            .unwrap()
            .len(),
        1
    );
    store.put_adaptive_rule(&rule).unwrap();
    store
        .set_adaptive_rule_status(id, AdaptiveRuleStatus::Enabled, 3)
        .unwrap();
    rule.status = AdaptiveRuleStatus::Enabled;
    rule.version = 2;
    rule.updated_at = 3;
    assert_eq!(store.adaptive_rule(id).unwrap(), rule);
    assert_eq!(
        store
            .adaptive_rules(Some(&RuleScope::Global), 5)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn jobs_persist_cancel_and_report_due() {
    let store = SqliteStore::memory().unwrap();
    let mut job = DurableJob::schedule("job-1", "remind me", 2000, 1000).unwrap();
    store.save_job(&job).unwrap();
    assert_eq!(store.job("job-1").unwrap().unwrap(), job);
    assert!(store.due_jobs(1500).unwrap().is_empty());
    assert_eq!(store.due_jobs(2000).unwrap().len(), 1);
    job.cancel(1500).unwrap();
    store.save_job(&job).unwrap();
    assert!(store.due_jobs(9999).unwrap().is_empty());
    assert_eq!(store.job("missing").unwrap(), None);
}

#[test]
fn job_save_rejects_invalid_records() {
    let store = SqliteStore::memory().unwrap();
    let mut job = DurableJob::schedule("job-1", "remind me", 2000, 1000).unwrap();
    job.objective.clear();
    assert!(store.save_job(&job).is_err());
}
