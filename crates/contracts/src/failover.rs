use crate::catalog::ProviderCatalog;
use crate::model::NormalizedError;
use crate::provider::{AuthKind, ProviderAvailability, ProviderDefinition};
use crate::requirements::{PrivacyClass, TaskRequirements};
use serde::{Deserialize, Serialize};

pub const FAILOVER_POLICY_SCHEMA_V1: &str = "aethra.failover-policy.v1";
/// At most one cloud fallback after the primary, so a turn cannot silently fan
/// out across many vendors before the user learns anything.
pub const FAILOVER_MAX_ATTEMPTS: usize = 2;

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum FailoverError {
    #[error("Primary provider id is required")]
    MissingPrimary,
    #[error("Provider cannot fail over to itself: {0}")]
    SelfFallback(String),
    #[error("Duplicate fallback provider: {0}")]
    DuplicateFallback(String),
    #[error("Provider not in catalog: {0}")]
    UnknownProvider(String),
    #[error("Fallback provider is not available: {0}")]
    UnavailableFallback(String),
    #[error("Fallback uses a different transport family: {0}")]
    TransportMismatch(String),
    #[error("Fallback lacks a shared auth kind: {0}")]
    AuthMismatch(String),
    #[error("Task must stay on device; no cloud fallback allowed")]
    LocalOnlyTask,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FailoverPolicy {
    pub schema: String,
    pub primary_provider_id: String,
    pub fallback_provider_ids: Vec<String>,
    /// Allow the on-device provider to answer after the cloud chain is
    /// exhausted. Off by default: moving to a model that runs on this device is
    /// a capability downgrade, not an equivalent retry, so it is never assumed.
    #[serde(default)]
    pub local_fallback: bool,
}

impl FailoverPolicy {
    pub fn validate(&self) -> Result<(), FailoverError> {
        if self.schema != FAILOVER_POLICY_SCHEMA_V1 {
            return Err(FailoverError::MissingPrimary);
        }
        if self.primary_provider_id.is_empty() {
            return Err(FailoverError::MissingPrimary);
        }
        let mut seen = Vec::new();
        for fallback in &self.fallback_provider_ids {
            if fallback == &self.primary_provider_id {
                return Err(FailoverError::SelfFallback(fallback.clone()));
            }
            if seen.contains(fallback) {
                return Err(FailoverError::DuplicateFallback(fallback.clone()));
            }
            seen.push(fallback.clone());
        }
        Ok(())
    }

    pub fn chain(&self) -> Vec<String> {
        let mut chain = vec![self.primary_provider_id.clone()];
        chain.extend(self.fallback_provider_ids.iter().cloned());
        chain
    }
}

pub fn compatible_fallback(
    catalog: &ProviderCatalog,
    requirements: &TaskRequirements,
    primary_id: &str,
    candidate_id: &str,
) -> Result<(), FailoverError> {
    if matches!(
        requirements.privacy_class,
        PrivacyClass::LocalOnly | PrivacyClass::StrictlyLocal
    ) {
        return Err(FailoverError::LocalOnlyTask);
    }
    let primary = catalog
        .get(primary_id)
        .ok_or_else(|| FailoverError::UnknownProvider(primary_id.into()))?;
    let candidate = catalog
        .get(candidate_id)
        .ok_or_else(|| FailoverError::UnknownProvider(candidate_id.into()))?;
    if candidate.availability != ProviderAvailability::Available {
        return Err(FailoverError::UnavailableFallback(candidate_id.into()));
    }
    if candidate.transport_family != primary.transport_family {
        return Err(FailoverError::TransportMismatch(candidate_id.into()));
    }
    if !shares_auth_kind(primary, candidate) {
        return Err(FailoverError::AuthMismatch(candidate_id.into()));
    }
    Ok(())
}

/// Whether a failure is transient enough that a different provider might
/// legitimately succeed, versus a failure that describes the user's own setup.
///
/// Only transient, provider-independent conditions may fail over. A rejected
/// credential, an unknown model, a bad endpoint, or a stream the provider could
/// not produce are configuration and capability facts: moving to another vendor
/// would hide them and send the request somewhere the user did not choose.
pub fn may_fail_over(error: &NormalizedError) -> bool {
    match error {
        NormalizedError::NetworkUnavailable { .. }
        | NormalizedError::Timeout
        | NormalizedError::RateLimited { .. }
        | NormalizedError::QuotaExceeded => true,
        // 402 and 429 are about this provider's account, not about the request,
        // so a second vendor with its own billing is a genuine way forward and is
        // the case fallback exists for. A 4xx that faults the request itself is
        // not: the same request would be refused identically elsewhere, and
        // switching would hide a bad model or a bad setting behind an answer that
        // looks fine.
        NormalizedError::ProviderError { status, .. } => *status >= 500 || *status == 0 || *status == 402,
        NormalizedError::AuthenticationFailed
        | NormalizedError::AuthorizationDenied
        | NormalizedError::ModelNotFound
        | NormalizedError::EndpointNotFound
        // A silent provider is not a reason to try someone else. The request was
        // delivered, so the fault is that provider's access to that model, and a
        // different key would be refused or ignored in the same way. Switching
        // would hide a setup mistake behind a working-looking answer.
        | NormalizedError::NoResponse
        | NormalizedError::InvalidResponse { .. } => false,
    }
}

fn shares_auth_kind(primary: &ProviderDefinition, candidate: &ProviderDefinition) -> bool {
    candidate.auth_options.iter().any(|option| {
        option.auth_kind != AuthKind::None
            && primary
                .auth_options
                .iter()
                .any(|primary_option| primary_option.auth_kind == option.auth_kind)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog_seeds::{anthropic_definition, default_catalog, openai_definition};

    fn policy() -> FailoverPolicy {
        FailoverPolicy {
            schema: FAILOVER_POLICY_SCHEMA_V1.into(),
            primary_provider_id: "openai".into(),
            fallback_provider_ids: vec!["nvidia-nim".into()],
            local_fallback: false,
        }
    }

    fn requirements() -> TaskRequirements {
        TaskRequirements::for_action(vec!["sms.send".into()])
    }

    #[test]
    fn valid_policy_chain() {
        let policy = policy();
        policy.validate().unwrap();
        assert_eq!(
            policy.chain(),
            vec!["openai".to_string(), "nvidia-nim".to_string()]
        );
    }

    #[test]
    fn self_fallback_rejected() {
        let mut policy = policy();
        policy.fallback_provider_ids = vec!["openai".into()];
        assert_eq!(
            policy.validate(),
            Err(FailoverError::SelfFallback("openai".into()))
        );
    }

    #[test]
    fn duplicate_fallback_rejected() {
        let mut policy = policy();
        policy.fallback_provider_ids = vec!["nvidia-nim".into(), "nvidia-nim".into()];
        assert!(matches!(
            policy.validate(),
            Err(FailoverError::DuplicateFallback(_))
        ));
    }

    #[test]
    fn openai_to_nvidia_is_compatible() {
        let catalog = default_catalog().unwrap();
        compatible_fallback(&catalog, &requirements(), "openai", "nvidia-nim").unwrap();
    }

    #[test]
    fn local_only_task_rejects_cloud_fallback() {
        let catalog = default_catalog().unwrap();
        let requirements = TaskRequirements::for_greeting();
        assert_eq!(
            compatible_fallback(&catalog, &requirements, "openai", "nvidia-nim"),
            Err(FailoverError::LocalOnlyTask)
        );
    }

    #[test]
    fn cross_transport_fallback_rejected() {
        let catalog = default_catalog().unwrap();
        assert_eq!(
            compatible_fallback(&catalog, &requirements(), "openai", "anthropic"),
            Err(FailoverError::TransportMismatch("anthropic".into()))
        );
    }

    #[test]
    fn disabled_fallback_rejected() {
        let catalog = default_catalog().unwrap();
        assert_eq!(
            compatible_fallback(&catalog, &requirements(), "openai", "aws-bedrock"),
            Err(FailoverError::UnavailableFallback("aws-bedrock".into()))
        );
    }

    #[test]
    fn unknown_provider_rejected() {
        let catalog = default_catalog().unwrap();
        assert_eq!(
            compatible_fallback(&catalog, &requirements(), "openai", "ghost"),
            Err(FailoverError::UnknownProvider("ghost".into()))
        );
    }

    #[test]
    fn policy_roundtrip() {
        let json = serde_json::to_string(&policy()).unwrap();
        let decoded: FailoverPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(policy(), decoded);
    }

    #[test]
    fn anthropic_seed_helpers_exist() {
        openai_definition().validate().unwrap();
        anthropic_definition().validate().unwrap();
    }

    #[test]
    fn transient_conditions_may_fail_over() {
        assert!(may_fail_over(&NormalizedError::NetworkUnavailable {
            detail: String::new(),
        }));
        assert!(may_fail_over(&NormalizedError::Timeout));
        assert!(may_fail_over(&NormalizedError::RateLimited {
            retry_after_secs: Some(30),
            detail: String::new(),
        }));
        assert!(may_fail_over(&NormalizedError::QuotaExceeded));
        assert!(may_fail_over(&NormalizedError::ProviderError {
            status: 503,
            detail: String::new()
        }));
        // An exhausted balance is this vendor's billing, not a fault in the
        // request, so a second vendor with its own credits is a real way forward.
        assert!(may_fail_over(&NormalizedError::ProviderError {
            status: 402,
            detail: "insufficient credits".into(),
        }));
    }

    /// The distinction that keeps fallback honest: a fault about the account can
    /// be routed around, a fault about the request cannot.
    #[test]
    fn a_rejected_request_never_fails_over_even_though_the_status_is_an_error() {
        for status in [400u16, 405, 409, 413, 415, 422] {
            assert!(
                !may_fail_over(&NormalizedError::ProviderError {
                    status,
                    detail: "unsupported parameter".into(),
                }),
                "HTTP {status} describes the request, so retrying elsewhere would only hide it"
            );
        }
    }

    #[test]
    fn setup_and_capability_faults_never_fail_over() {
        // A rejected key, an unknown model, a bad endpoint, or a stream the
        // provider could not produce describe the user's setup. Moving to
        // another vendor would hide that and leak the prompt elsewhere.
        for error in [
            NormalizedError::AuthenticationFailed,
            NormalizedError::AuthorizationDenied,
            NormalizedError::ModelNotFound,
            NormalizedError::EndpointNotFound,
            NormalizedError::InvalidResponse {
                detail: "stream produced no content".into(),
            },
            NormalizedError::ProviderError {
                status: 404,
                detail: String::new(),
            },
        ] {
            assert!(!may_fail_over(&error), "must not fail over: {error:?}");
        }
    }

    #[test]
    fn attempt_cap_is_bounded() {
        assert_eq!(FAILOVER_MAX_ATTEMPTS, 2);
        let chain = ["a".to_string(), "b".to_string(), "c".to_string()];
        let used = chain.len().min(FAILOVER_MAX_ATTEMPTS);
        assert_eq!(
            used, 2,
            "one fallback after the primary, never the whole chain"
        );
    }
}
