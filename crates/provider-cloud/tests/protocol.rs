use assistant_contracts::*;
use provider_cloud::*;
use serde_json::json;
use std::sync::Arc;
fn context() -> ContextBundle {
    ContextBundle {
        task_id: Id::new_v4(),
        role: Role::Reasoner,
        goal: "Help".into(),
        plan: vec![],
        handoff: None,
        history: vec![],
        results: vec![],
        skills: vec![],
        candidates: vec![],
        tools: vec![],
        adaptive_rules: vec![],
    }
}
fn config(api: ApiKind) -> CloudConfig {
    CloudConfig {
        id: "test".into(),
        endpoint: "https://api.example.com/v1".into(),
        model: "configured-model".into(),
        secret_ref: "ASSISTANT_TEST_KEY".into(),
        api,
        max_output_tokens: 777,
    }
}
#[test]
fn responses_normalizes_control_without_persisting_remote_conversation() {
    let provider =
        CloudProvider::new(config(ApiKind::Responses), Arc::new(EnvironmentSecrets)).unwrap();
    let ctx = context();
    assert_eq!(provider.request_body(&ctx).unwrap()["store"], false);
    assert_eq!(
        provider.request_body(&ctx).unwrap()["max_output_tokens"],
        777
    );
    let action=provider.parse(&json!({"status":"completed","output":[{"type":"function_call","name":"capabilities_search","arguments":"{\"query\":\"calendar\"}"}]}),&ctx).unwrap();
    assert!(matches!(action,AgentAction::Search {query} if query=="calendar"));
}
#[test]
fn rejects_partial_and_multiple_calls() {
    let provider =
        CloudProvider::new(config(ApiKind::Responses), Arc::new(EnvironmentSecrets)).unwrap();
    let call = json!({"type":"function_call","name":"capabilities_search","arguments":"{\"query\":\"calendar\"}"});
    assert!(provider
        .parse(
            &json!({"status":"incomplete","output":[call.clone()]}),
            &context()
        )
        .is_err());
    assert!(provider
        .parse(
            &json!({"status":"completed","output":[call.clone(),call]}),
            &context()
        )
        .is_err());
}
#[test]
fn compatible_adapter_and_endpoint_validation() {
    let provider = CloudProvider::new(
        config(ApiKind::ChatCompletions),
        Arc::new(EnvironmentSecrets),
    )
    .unwrap();
    assert_eq!(
        provider.request_body(&context()).unwrap()["max_tokens"],
        777
    );
    assert!(
        matches!(provider.parse(&json!({"choices":[{"finish_reason":"stop","message":{"content":"Hello"}}]}),&context()).unwrap(),AgentAction::Respond {text} if text=="Hello")
    );
    let mut bad = config(ApiKind::Responses);
    bad.endpoint = "http://example.com".into();
    assert!(CloudProvider::new(bad, Arc::new(EnvironmentSecrets)).is_err());
    for invalid_limit in [0, 8193] {
        let mut bad = config(ApiKind::Responses);
        bad.max_output_tokens = invalid_limit;
        assert_eq!(
            CloudProvider::new(bad, Arc::new(EnvironmentSecrets))
                .err()
                .unwrap(),
            Error::InvalidInput
        );
    }
    assert_eq!(EnvironmentSecrets.get("PATH").unwrap_err(), Error::Denied);
}

#[test]
fn credential_references_are_environment_names_not_keys() {
    assert!(valid_secret_reference("ASSISTANT_CLOUD_KEY"));
    for invalid in [
        "nvapi-fixture-not-a-real-key",
        "PATH",
        "ASSISTANT_bad_KEY",
        "ASSISTANT_\n_KEY",
    ] {
        assert!(!valid_secret_reference(invalid));
        assert_eq!(EnvironmentSecrets.get(invalid), Err(Error::Denied));
    }
    let missing = format!("ASSISTANT_{}_KEY", Id::new_v4().simple()).to_uppercase();
    assert_eq!(EnvironmentSecrets.get(&missing), Err(Error::AuthRequired));
}
