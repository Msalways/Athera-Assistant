use crate::provider::AuthKind;
use crate::vault::{SecretVault, VaultError};

/// Schema version for auth resolver.
pub const AUTH_RESOLVER_SCHEMA_V1: &str = "aethra.auth-resolver.v1";

/// Errors from the auth resolver.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum AuthError {
    #[error("Credential not found")]
    NotFound,
    #[error("Vault locked or unavailable")]
    VaultLocked,
    #[error("Unsupported auth kind: {0}")]
    UnsupportedKind(String),
    #[error("Header name is required")]
    MissingHeaderName,
    #[error("Composite secret is not a JSON object")]
    MalformedComposite,
}

/// A resolved credential ready for injection into an HTTP request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCredential {
    pub header_name: String,
    pub header_value: String,
}

/// Resolves a provider's auth configuration into HTTP headers.
/// Reads from the vault; never stores or caches secrets.
pub struct AuthResolver {
    vault: Box<dyn SecretVault>,
}

impl AuthResolver {
    pub fn new(vault: Box<dyn SecretVault>) -> Self {
        Self { vault }
    }

    /// Resolve a credential into an HTTP header pair.
    pub fn resolve(
        &self,
        owner_id: &str,
        purpose: crate::credential::CredentialPurpose,
        auth_kind: AuthKind,
        header_name: Option<&str>,
    ) -> Result<ResolvedCredential, AuthError> {
        let secret = self
            .vault
            .get(owner_id, purpose)
            .map_err(|e| match e {
                VaultError::NotFound => AuthError::NotFound,
                VaultError::Locked => AuthError::VaultLocked,
                _ => AuthError::NotFound,
            })?
            .ok_or(AuthError::NotFound)?;

        match auth_kind {
            AuthKind::ApiKey => {
                let name = header_name.unwrap_or("x-api-key").to_owned();
                Ok(ResolvedCredential {
                    header_name: name,
                    header_value: secret,
                })
            }
            AuthKind::BearerToken => {
                let name = header_name.unwrap_or("authorization").to_owned();
                let value = format!("Bearer {secret}");
                Ok(ResolvedCredential {
                    header_name: name,
                    header_value: value,
                })
            }
            AuthKind::CustomCompatible => {
                let name = header_name.ok_or(AuthError::MissingHeaderName)?.to_owned();
                Ok(ResolvedCredential {
                    header_name: name,
                    header_value: secret,
                })
            }
            other => Err(AuthError::UnsupportedKind(format!("{other:?}"))),
        }
    }

    /// Whether the vault has a credential for this owner/purpose.
    pub fn has_credential(
        &self,
        owner_id: &str,
        purpose: crate::credential::CredentialPurpose,
    ) -> bool {
        self.vault.has(owner_id, purpose)
    }

    /// Redact a credential value for safe logging/display.
    pub fn redact(value: &str) -> String {
        if value.len() <= 8 {
            "****".to_string()
        } else {
            let prefix = &value[..4];
            let suffix = &value[value.len() - 4..];
            format!("{prefix}****{suffix}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential::CredentialPurpose;
    use crate::vault::TestVault;

    fn setup() -> AuthResolver {
        let vault = TestVault::new();
        vault
            .put(
                "nvidia",
                CredentialPurpose::ProviderAuth,
                "sk-test-abc123def456",
            )
            .unwrap();
        AuthResolver::new(Box::new(vault))
    }

    #[test]
    fn api_key_resolution() {
        let resolver = setup();
        let cred = resolver
            .resolve(
                "nvidia",
                CredentialPurpose::ProviderAuth,
                AuthKind::ApiKey,
                None,
            )
            .unwrap();
        assert_eq!(cred.header_name, "x-api-key");
        assert_eq!(cred.header_value, "sk-test-abc123def456");
    }

    #[test]
    fn bearer_token_resolution() {
        let resolver = setup();
        let cred = resolver
            .resolve(
                "nvidia",
                CredentialPurpose::ProviderAuth,
                AuthKind::BearerToken,
                None,
            )
            .unwrap();
        assert_eq!(cred.header_name, "authorization");
        assert_eq!(cred.header_value, "Bearer sk-test-abc123def456");
    }

    #[test]
    fn custom_header_resolution() {
        let resolver = setup();
        let cred = resolver
            .resolve(
                "nvidia",
                CredentialPurpose::ProviderAuth,
                AuthKind::CustomCompatible,
                Some("x-custom-key"),
            )
            .unwrap();
        assert_eq!(cred.header_name, "x-custom-key");
        assert_eq!(cred.header_value, "sk-test-abc123def456");
    }

    #[test]
    fn missing_credential_returns_not_found() {
        let resolver = setup();
        let result = resolver.resolve(
            "nonexistent",
            CredentialPurpose::ProviderAuth,
            AuthKind::ApiKey,
            None,
        );
        assert_eq!(result, Err(AuthError::NotFound));
    }

    #[test]
    fn custom_header_requires_name() {
        let resolver = setup();
        let result = resolver.resolve(
            "nvidia",
            CredentialPurpose::ProviderAuth,
            AuthKind::CustomCompatible,
            None,
        );
        assert_eq!(result, Err(AuthError::MissingHeaderName));
    }

    #[test]
    fn unsupported_auth_kind_rejected() {
        let resolver = setup();
        let result = resolver.resolve(
            "nvidia",
            CredentialPurpose::ProviderAuth,
            AuthKind::OAuth2Pkce,
            None,
        );
        assert!(matches!(result, Err(AuthError::UnsupportedKind(_))));
    }

    #[test]
    fn redact_long_string() {
        assert_eq!(AuthResolver::redact("sk-abc123def456"), "sk-a****f456");
    }

    #[test]
    fn redact_short_string() {
        assert_eq!(AuthResolver::redact("abc"), "****");
    }
}

/// A composite static secret resolver for providers needing multiple values
/// (e.g., API key + endpoint, or API key + org ID).
pub struct CompositeSecretResolver {
    vault: Box<dyn SecretVault>,
}

impl CompositeSecretResolver {
    pub fn new(vault: Box<dyn SecretVault>) -> Self {
        Self { vault }
    }

    /// Resolve multiple secrets for a provider.
    /// The vault entry holds one JSON object mapping key names to values;
    /// every requested key must be present with a string value, otherwise
    /// the whole set is rejected.
    pub fn resolve_all(
        &self,
        owner_id: &str,
        purpose: crate::credential::CredentialPurpose,
        keys: &[&str],
    ) -> Result<Vec<ResolvedCompositeSecret>, AuthError> {
        let raw = self
            .vault
            .get(owner_id, purpose)
            .map_err(|_| AuthError::NotFound)?
            .ok_or(AuthError::NotFound)?;
        let map: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&raw).map_err(|_| AuthError::MalformedComposite)?;
        let mut results = Vec::new();
        for key in keys {
            let value = map
                .get(*key)
                .and_then(|value| value.as_str())
                .ok_or(AuthError::NotFound)?;
            results.push(ResolvedCompositeSecret {
                key: key.to_string(),
                value: value.to_owned(),
            });
        }
        Ok(results)
    }
}

/// A resolved composite secret (key + value pair).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCompositeSecret {
    pub key: String,
    pub value: String,
}

#[cfg(test)]
mod composite_tests {
    use super::*;
    use crate::credential::CredentialPurpose;
    use crate::vault::TestVault;

    fn setup() -> CompositeSecretResolver {
        let vault = TestVault::new();
        vault
            .put(
                "aws",
                CredentialPurpose::ProviderAuth,
                r#"{"access_key_id":"AKID","secret_access_key":"SECRET","session_token":"TOKEN"}"#,
            )
            .unwrap();
        CompositeSecretResolver::new(Box::new(vault))
    }

    #[test]
    fn resolves_distinct_values_per_key() {
        let resolved = setup()
            .resolve_all(
                "aws",
                CredentialPurpose::ProviderAuth,
                &["access_key_id", "secret_access_key", "session_token"],
            )
            .unwrap();
        assert_eq!(resolved.len(), 3);
        assert_eq!(resolved[0].value, "AKID");
        assert_eq!(resolved[1].value, "SECRET");
        assert_eq!(resolved[2].value, "TOKEN");
    }

    #[test]
    fn partial_set_rejected() {
        let result = setup().resolve_all(
            "aws",
            CredentialPurpose::ProviderAuth,
            &["access_key_id", "missing_key"],
        );
        assert_eq!(result, Err(AuthError::NotFound));
    }

    #[test]
    fn non_object_blob_rejected() {
        let vault = TestVault::new();
        vault
            .put("flat", CredentialPurpose::ProviderAuth, "just-a-string")
            .unwrap();
        let resolver = CompositeSecretResolver::new(Box::new(vault));
        assert_eq!(
            resolver.resolve_all("flat", CredentialPurpose::ProviderAuth, &["access_key_id"]),
            Err(AuthError::MalformedComposite)
        );
    }

    #[test]
    fn non_string_value_rejected() {
        let vault = TestVault::new();
        vault
            .put(
                "mixed",
                CredentialPurpose::ProviderAuth,
                r#"{"access_key_id":42}"#,
            )
            .unwrap();
        let resolver = CompositeSecretResolver::new(Box::new(vault));
        assert_eq!(
            resolver.resolve_all("mixed", CredentialPurpose::ProviderAuth, &["access_key_id"]),
            Err(AuthError::NotFound)
        );
    }

    #[test]
    fn missing_entry_returns_not_found() {
        assert_eq!(
            setup().resolve_all("ghost", CredentialPurpose::ProviderAuth, &["access_key_id"]),
            Err(AuthError::NotFound)
        );
    }
}
