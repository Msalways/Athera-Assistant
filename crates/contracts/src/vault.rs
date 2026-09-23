//! Vendor-neutral secret vault interface.
//! Implementations exist in platform adapters (Android Keystore, host test vault).
//! The trait is defined here so orchestration code depends only on the contract.
use crate::credential::CredentialPurpose;

/// Errors specific to vault operations.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum VaultError {
    #[error("Secret not found")]
    NotFound,
    #[error("Wrong purpose or binding")]
    PurposeMismatch,
    #[error("Vault is locked or unavailable")]
    Locked,
    #[error("Storage failure")]
    Storage,
}

/// A vendor-neutral vault for storing and retrieving secrets.
/// Each secret is bound to a (owner_id, purpose) pair.
/// Wrong purpose or binding never resolves a secret.
pub trait SecretVault: Send + Sync {
    fn put(
        &self,
        owner_id: &str,
        purpose: CredentialPurpose,
        value: &str,
    ) -> Result<(), VaultError>;

    fn get(&self, owner_id: &str, purpose: CredentialPurpose)
        -> Result<Option<String>, VaultError>;

    fn delete(&self, owner_id: &str, purpose: CredentialPurpose) -> Result<(), VaultError>;

    fn has(&self, owner_id: &str, purpose: CredentialPurpose) -> bool {
        self.get(owner_id, purpose)
            .map(|v| v.is_some())
            .unwrap_or(false)
    }

    fn delete_all(&self, owner_id: &str) -> Result<(), VaultError>;
}

/// A host-only test vault for deterministic unit tests.
pub struct TestVault {
    inner: std::sync::Mutex<std::collections::HashMap<(String, CredentialPurpose), String>>,
}

impl TestVault {
    pub fn new() -> Self {
        Self {
            inner: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for TestVault {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretVault for TestVault {
    fn put(
        &self,
        owner_id: &str,
        purpose: CredentialPurpose,
        value: &str,
    ) -> Result<(), VaultError> {
        let mut map = self.inner.lock().map_err(|_| VaultError::Locked)?;
        map.insert((owner_id.to_owned(), purpose), value.to_owned());
        Ok(())
    }

    fn get(
        &self,
        owner_id: &str,
        purpose: CredentialPurpose,
    ) -> Result<Option<String>, VaultError> {
        let map = self.inner.lock().map_err(|_| VaultError::Locked)?;
        Ok(map.get(&(owner_id.to_owned(), purpose)).cloned())
    }

    fn delete(&self, owner_id: &str, purpose: CredentialPurpose) -> Result<(), VaultError> {
        let mut map = self.inner.lock().map_err(|_| VaultError::Locked)?;
        map.remove(&(owner_id.to_owned(), purpose));
        Ok(())
    }

    fn has(&self, owner_id: &str, purpose: CredentialPurpose) -> bool {
        self.get(owner_id, purpose)
            .map(|v| v.is_some())
            .unwrap_or(false)
    }

    fn delete_all(&self, owner_id: &str) -> Result<(), VaultError> {
        let mut map = self.inner.lock().map_err(|_| VaultError::Locked)?;
        map.retain(|(id, _), _| id != owner_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_put_get_delete() {
        let vault = TestVault::new();
        vault
            .put("nvidia", CredentialPurpose::ProviderAuth, "secret-key")
            .unwrap();
        let value = vault
            .get("nvidia", CredentialPurpose::ProviderAuth)
            .unwrap();
        assert_eq!(value.as_deref(), Some("secret-key"));
        vault
            .delete("nvidia", CredentialPurpose::ProviderAuth)
            .unwrap();
        assert!(!vault.has("nvidia", CredentialPurpose::ProviderAuth));
    }

    #[test]
    fn purpose_mismatch_returns_none() {
        let vault = TestVault::new();
        vault
            .put("mcp-1", CredentialPurpose::ConnectorAuth, "token")
            .unwrap();
        let value = vault.get("mcp-1", CredentialPurpose::ProviderAuth).unwrap();
        assert_eq!(value, None);
    }

    #[test]
    fn wrong_owner_returns_none() {
        let vault = TestVault::new();
        vault
            .put("owner-a", CredentialPurpose::ProviderAuth, "key")
            .unwrap();
        let value = vault
            .get("owner-b", CredentialPurpose::ProviderAuth)
            .unwrap();
        assert_eq!(value, None);
    }

    #[test]
    fn delete_all_removes_owner_secrets() {
        let vault = TestVault::new();
        vault
            .put("x", CredentialPurpose::ProviderAuth, "k1")
            .unwrap();
        vault
            .put("x", CredentialPurpose::ConnectorAuth, "k2")
            .unwrap();
        vault.delete_all("x").unwrap();
        assert!(!vault.has("x", CredentialPurpose::ProviderAuth));
        assert!(!vault.has("x", CredentialPurpose::ConnectorAuth));
    }
}
