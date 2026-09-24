use crate::catalog::ProviderCatalog;
use crate::provider::{AuthKind, ProviderAvailability, ProviderDefinition};
use crate::requirements::{PrivacyClass, TaskRequirements};
use serde::{Deserialize, Serialize};

pub const FAILOVER_POLICY_SCHEMA_V1: &str = "aethra.failover-policy.v1";

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
}
