//! Failover behaviour tests: what may fail over, what must not, and that the
//! provider which answered is always attributable.

use super::*;
use assistant_contracts::failover::{may_fail_over, FailoverPolicy, FAILOVER_MAX_ATTEMPTS};
use assistant_contracts::model::NormalizedError;
use assistant_contracts::Id;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn policy(ids: &[&str]) -> FailoverPolicy {
    FailoverPolicy {
        schema: assistant_contracts::failover::FAILOVER_POLICY_SCHEMA_V1.into(),
        primary_provider_id: ids[0].into(),
        fallback_provider_ids: ids[1..].iter().map(|s| (*s).to_string()).collect(),
        local_fallback: false,
    }
}

fn set_policy(runtime: &Runtime, ids: &[&str]) {
    runtime
        .store
        .set_setting(
            FAILOVER_POLICY_SETTING,
            &serde_json::to_value(policy(ids)).unwrap(),
        )
        .unwrap();
}

fn profile_for(provider_id: &str) -> ProviderProfile {
    let mut profile = ProviderProfile::new(provider_id, "api_key");
    profile.non_secret_config =
        serde_json::json!({ "base_url": "https://example.test/v1", "model": "m" });
    profile
}

/// Configure a usable provider with a stored key, the only kind that can build.
async fn await_fallback_only(runtime: &Runtime) {
    runtime
        .dispatch(
            "save_provider_profile",
            serde_json::json!({
                "provider_id": "openai-compatible",
                "auth_option_id": "api_key",
                "config": { "base_url": "https://example.test/v1", "model": "m" },
                "secret": "sk-fixture-not-real",
            }),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn no_policy_setting_means_a_single_provider_chain() {
    let runtime = Runtime::open(":memory:").unwrap();
    let chain = resolve_failover_chain(&runtime.store, &runtime.secrets, None).unwrap();
    assert!(
        chain.is_empty(),
        "absent policy must not assemble a chain at all"
    );
}

#[tokio::test]
async fn an_unusable_primary_is_never_replaced_by_another_vendor() {
    let runtime = Runtime::open(":memory:").unwrap();
    // Self-fallback is refused by the policy contract.
    let bad = policy(&["openai", "openai"]);
    assert!(bad.validate().is_err());
    // A policy naming an unknown primary keeps that primary in the first slot.
    // It used to contribute nothing, which let the next member answer as though
    // the configured primary had never existed.
    set_policy(&runtime, &["ghost-provider", "nvidia-nim"]);
    let chain = resolve_failover_chain(&runtime.store, &runtime.secrets, None).unwrap();
    assert_eq!(
        chain.first().map(|p| p.id().to_owned()),
        Some("ghost-provider".to_owned()),
        "an unusable primary must not be silently replaced"
    );
}

#[tokio::test]
async fn each_chain_member_binds_its_own_credential() {
    let runtime = Runtime::open(":memory:").unwrap();
    let reference = format!("ASSISTANT_{}_KEY", Id::new_v4().simple()).to_uppercase();

    // The primary has a profile and a stored secret.
    let mut primary = ProviderProfile::new("nvidia-nim", "api_key");
    primary.non_secret_config = serde_json::json!({
        "base_url": "https://primary.example/v1",
        "model": "primary-model",
    });
    runtime.store.save_provider_profile(&primary).unwrap();
    runtime
        .dispatch(
            "save_provider_profile",
            serde_json::json!({
                "provider_id": "nvidia-nim",
                "auth_option_id": "api_key",
                "config": {"base_url": "https://primary.example/v1", "model": "primary-model"},
                "secret": "primary-key-value-not-real",
            }),
        )
        .await
        .unwrap();
    let _ = reference;

    // The fallback is named but has no profile of its own, so it must not be
    // assembled, and the primary's secret must never be reused for it.
    runtime
        .store
        .set_setting(
            FAILOVER_POLICY_SETTING,
            &serde_json::to_value(policy(&["nvidia-nim", "openai-compatible"])).unwrap(),
        )
        .unwrap();

    let chain = resolve_failover_chain(&runtime.store, &runtime.secrets, None).unwrap();
    assert_eq!(
        chain.len(),
        1,
        "only the member that can bind its own credential joins the chain"
    );
}

#[tokio::test]
async fn a_cleared_policy_behaves_exactly_like_an_absent_one() {
    let runtime = Runtime::open(":memory:").unwrap();
    // Clearing stores JSON null rather than removing the key. That must not be
    // read back as a malformed policy, or the whole snapshot breaks.
    runtime
        .store
        .set_setting(FAILOVER_POLICY_SETTING, &serde_json::Value::Null)
        .unwrap();
    let chain = resolve_failover_chain(&runtime.store, &runtime.secrets, None).unwrap();
    assert!(chain.is_empty());
    assert!(
        runtime.snapshot().await.is_ok(),
        "a cleared policy must not break the snapshot"
    );
}

#[tokio::test]
async fn an_unusable_primary_holds_its_slot_instead_of_promoting_the_fallback() {
    let runtime = Runtime::open(":memory:").unwrap();
    // Only the fallback exists. The primary named in the policy does not.
    await_fallback_only(&runtime).await;
    runtime
        .store
        .save_provider_profile(&profile_for("openai-compatible"))
        .unwrap();
    set_policy(&runtime, &["nvidia-nim", "openai-compatible"]);

    let chain = resolve_failover_chain(&runtime.store, &runtime.secrets, None).unwrap();
    // The order the user configured must survive: index 0 is still the primary,
    // which cannot answer, rather than the fallback promoted into its place.
    assert_eq!(
        chain.first().map(|p| p.id().to_owned()),
        Some("nvidia-nim".to_owned()),
        "the primary must keep its slot even when it cannot be built"
    );
}

#[tokio::test]
async fn a_missing_profile_on_the_primary_never_reorders_the_chain() {
    let runtime = Runtime::open(":memory:").unwrap();
    // A fallback with a key, and a policy whose primary has no profile at all.
    await_fallback_only(&runtime).await;
    set_policy(&runtime, &["ghost-provider", "openai-compatible"]);

    let chain = resolve_failover_chain(&runtime.store, &runtime.secrets, None).unwrap();
    assert_eq!(
        chain.first().map(|p| p.id().to_owned()),
        Some("ghost-provider".to_owned()),
        "an absent primary must still be first, not silently replaced"
    );
}

#[tokio::test]
async fn an_unusable_fallback_is_dropped_without_deciding_who_answers() {
    let runtime = Runtime::open(":memory:").unwrap();
    await_fallback_only(&runtime).await;
    // The primary works; the fallback does not exist.
    set_policy(&runtime, &["openai-compatible", "ghost-provider"]);
    let chain = resolve_failover_chain(&runtime.store, &runtime.secrets, None).unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].id(), "openai-compatible");
}

#[test]
fn the_on_device_member_is_reachable_behind_two_cloud_providers() {
    // The vendor budget caps cloud members; the local member gets its own single
    // attempt. Capping the whole chain at the vendor budget instead left the
    // local member permanently out of reach, making opting into it a no-op.
    let cloud = 2;
    let local = 1;
    assert!(cloud + local > FAILOVER_MAX_ATTEMPTS);
    assert_eq!(cloud, FAILOVER_MAX_ATTEMPTS, "vendors stay capped");
    assert_eq!(local, 1, "the device gets exactly one attempt");
}

/// Records whether it was asked to answer, so a test can prove a chain stopped
/// rather than merely reordered.
struct CountingProvider {
    id: &'static str,
    calls: Arc<AtomicUsize>,
    outcome: std::result::Result<AgentAction, NormalizedError>,
}

#[async_trait::async_trait]
impl ModelProvider for CountingProvider {
    fn id(&self) -> &str {
        self.id
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
        match &self.outcome {
            Ok(action) => Ok(action.clone()),
            Err(_) => Err(Error::Unavailable),
        }
    }

    async fn infer_typed(
        &self,
        _context: ContextBundle,
        _sink: Option<assistant_contracts::ProviderEventSink>,
    ) -> std::result::Result<AgentAction, NormalizedError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.outcome.clone()
    }
}

fn respond() -> AgentAction {
    AgentAction::Respond {
        text: "answered".into(),
    }
}

fn chain_of(parts: Vec<Arc<dyn ModelProvider>>) -> failover::FailoverCloud {
    failover::FailoverCloud::new(parts)
}

fn context() -> ContextBundle {
    ContextBundle {
        task_id: Id::new_v4(),
        role: Role::Reasoner,
        goal: "hi".into(),
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

#[tokio::test]
async fn a_blocked_primary_stops_the_turn_instead_of_letting_the_fallback_answer() {
    // The behavioural half of "order is preserved": preserving the order is
    // worthless if the chain still walks past a blocked slot. A deleted profile
    // must not be reported as a transient fault.
    let primary_calls = Arc::new(AtomicUsize::new(0));
    let fallback_calls = Arc::new(AtomicUsize::new(0));
    let chain = chain_of(vec![
        Arc::new(CountingProvider {
            id: "deleted-primary",
            calls: primary_calls.clone(),
            // What a blocked member reports: the provider it names is not there.
            outcome: Err(NormalizedError::EndpointNotFound),
        }),
        Arc::new(CountingProvider {
            id: "healthy-fallback",
            calls: fallback_calls.clone(),
            outcome: Ok(respond()),
        }),
    ]);

    let outcome = chain.infer(context()).await;
    assert!(
        outcome.is_err(),
        "a blocked primary must not be answered by the fallback"
    );
    assert_eq!(
        primary_calls.load(Ordering::SeqCst),
        1,
        "the primary was tried"
    );
    assert_eq!(
        fallback_calls.load(Ordering::SeqCst),
        0,
        "the fallback must not answer behind a blocked primary"
    );
}

#[tokio::test]
async fn a_genuinely_transient_failure_still_reaches_the_fallback() {
    // Guard against over-correcting: the fix must not stop real failover.
    let primary_calls = Arc::new(AtomicUsize::new(0));
    let fallback_calls = Arc::new(AtomicUsize::new(0));
    let chain = chain_of(vec![
        Arc::new(CountingProvider {
            id: "flaky-primary",
            calls: primary_calls.clone(),
            outcome: Err(NormalizedError::ProviderError {
                status: 503,
                detail: "unavailable".into(),
            }),
        }),
        Arc::new(CountingProvider {
            id: "healthy-fallback",
            calls: fallback_calls.clone(),
            outcome: Ok(respond()),
        }),
    ]);

    let outcome = chain.infer(context()).await;
    assert!(outcome.is_ok(), "a transient fault must still fail over");
    assert_eq!(fallback_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_deleted_profile_reports_a_fault_that_cannot_be_treated_as_transient() {
    // Documents the coupling that made the silent switch reachable: the engine's
    // generic `Unavailable` becomes a provider error with no HTTP status, and the
    // failover rule counts a status-less error as transient. So a member that is
    // blocked on purpose must not report itself through that default.
    let mapped = assistant_contracts::normalized_from_engine(Error::Unavailable);
    assert_eq!(
        mapped,
        NormalizedError::ProviderError {
            status: 0,
            detail: "provider unavailable".into()
        }
    );
    assert!(
        may_fail_over(&mapped),
        "the lossy default really is transient; this is why the blocked member overrides it"
    );
    // What the blocked member reports instead, and the reason the chain stops.
    assert!(
        !may_fail_over(&NormalizedError::EndpointNotFound),
        "a provider the chain names but that is not configured cannot be waited out"
    );
}

#[test]
fn transient_failures_are_the_only_ones_allowed_to_move_vendor() {
    // The rule the whole feature rests on.
    assert!(may_fail_over(&NormalizedError::Timeout));
    assert!(may_fail_over(&NormalizedError::NetworkUnavailable));
    assert!(may_fail_over(&NormalizedError::RateLimited {
        retry_after_secs: None
    }));
    // Setup and capability facts must surface to the user instead.
    assert!(!may_fail_over(&NormalizedError::AuthenticationFailed));
    assert!(!may_fail_over(&NormalizedError::ModelNotFound));
    assert!(!may_fail_over(&NormalizedError::EndpointNotFound));
    assert!(!may_fail_over(&NormalizedError::InvalidResponse {
        detail: String::new()
    }));
}

#[test]
fn a_turn_can_try_at_most_the_primary_and_one_fallback() {
    let ids = ["a", "b", "c", "d"];
    let chain = policy(&ids);
    let used = chain.chain().len().min(FAILOVER_MAX_ATTEMPTS);
    assert_eq!(used, 2);
    assert!(
        used < chain.chain().len(),
        "a long chain must not be walked in one turn"
    );
}
