use crate::credential::CredentialPurpose;
use crate::vault::SecretVault;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleStatus {
    Active,
    ExpiringSoon,
    Expired,
    Missing,
    Revoked,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum LifecycleError {
    #[error("Credential not found")]
    NotFound,
    #[error("Vault unavailable")]
    VaultUnavailable,
}

pub struct LifecycleService {
    vault: Box<dyn SecretVault>,
}

impl LifecycleService {
    pub fn new(vault: Box<dyn SecretVault>) -> Self {
        Self { vault }
    }

    pub fn status(
        &self,
        owner_id: &str,
        purpose: CredentialPurpose,
        expires_at: Option<u64>,
        now_millis: u64,
    ) -> LifecycleStatus {
        if !self.vault.has(owner_id, purpose) {
            return LifecycleStatus::Missing;
        }
        match expires_at {
            None => LifecycleStatus::Active,
            Some(expires) if expires <= now_millis => LifecycleStatus::Expired,
            Some(expires) if expires - now_millis < 86_400_000 => LifecycleStatus::ExpiringSoon,
            Some(_) => LifecycleStatus::Active,
        }
    }

    pub fn revoke(&self, owner_id: &str, purpose: CredentialPurpose) -> Result<(), LifecycleError> {
        if !self.vault.has(owner_id, purpose) {
            return Err(LifecycleError::NotFound);
        }
        self.vault
            .delete(owner_id, purpose)
            .map_err(|_| LifecycleError::VaultUnavailable)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::TestVault;

    fn service() -> LifecycleService {
        let vault = TestVault::new();
        vault
            .put("openai", CredentialPurpose::ProviderAuth, "sk-test")
            .unwrap();
        LifecycleService::new(Box::new(vault))
    }

    #[test]
    fn active_without_expiry() {
        assert_eq!(
            service().status("openai", CredentialPurpose::ProviderAuth, None, 1000),
            LifecycleStatus::Active
        );
    }

    #[test]
    fn expired_detected() {
        assert_eq!(
            service().status("openai", CredentialPurpose::ProviderAuth, Some(500), 1000),
            LifecycleStatus::Expired
        );
    }

    #[test]
    fn expiring_soon_within_a_day() {
        assert_eq!(
            service().status(
                "openai",
                CredentialPurpose::ProviderAuth,
                Some(1000 + 3_600_000),
                1000
            ),
            LifecycleStatus::ExpiringSoon
        );
    }

    #[test]
    fn missing_when_no_secret() {
        assert_eq!(
            service().status("ghost", CredentialPurpose::ProviderAuth, None, 1000),
            LifecycleStatus::Missing
        );
    }

    #[test]
    fn revoke_removes_secret() {
        let service = service();
        service
            .revoke("openai", CredentialPurpose::ProviderAuth)
            .unwrap();
        assert_eq!(
            service.status("openai", CredentialPurpose::ProviderAuth, None, 1000),
            LifecycleStatus::Missing
        );
    }

    #[test]
    fn revoke_missing_returns_not_found() {
        assert_eq!(
            service().revoke("ghost", CredentialPurpose::ProviderAuth),
            Err(LifecycleError::NotFound)
        );
    }
}
