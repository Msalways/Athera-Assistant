//! Session-only host credentials. Never serialized or included in debug output.
use adapter_mcp::{ConnectionConfig, CredentialPurpose, CredentialRequest, CredentialResolver};
use assistant_contracts::{Error, Result};
use provider_cloud::{valid_secret_reference, CloudConfig, EnvironmentSecrets, SecretStore};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

struct CloudCredential {
    reference: String,
    endpoint: String,
    value: String,
}

struct McpCredential {
    binding: CredentialRequest,
    value: String,
}

#[derive(Default)]
pub(crate) struct SessionSecrets {
    cloud: Mutex<Option<CloudCredential>>,
    mcp: Mutex<BTreeMap<String, McpCredential>>,
}

impl SessionSecrets {
    pub fn validate(value: &str) -> Result<()> {
        if value.is_empty() || value.len() > 8192 || !value.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }

    pub fn set(&self, config: &CloudConfig, value: String) -> Result<()> {
        Self::validate(&value)?;
        if !valid_secret_reference(&config.secret_ref) {
            return Err(Error::InvalidInput);
        }
        *self.cloud.lock().map_err(|_| Error::Unavailable)? = Some(CloudCredential {
            reference: config.secret_ref.clone(),
            endpoint: config.endpoint.clone(),
            value,
        });
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        *self.cloud.lock().map_err(|_| Error::Unavailable)? = None;
        Ok(())
    }

    pub fn contains(&self, config: &CloudConfig) -> bool {
        self.cloud.lock().is_ok_and(|slot| {
            slot.as_ref().is_some_and(|key| {
                key.reference == config.secret_ref && key.endpoint == config.endpoint
            })
        })
    }

    pub fn provider(self: &Arc<Self>, config: &CloudConfig) -> Arc<dyn SecretStore> {
        Arc::new(ScopedSecrets {
            store: self.clone(),
            endpoint: config.endpoint.clone(),
        })
    }

    pub fn set_mcp(&self, config: &ConnectionConfig, value: String) -> Result<()> {
        Self::validate(&value)?;
        let binding = config.credential_request()?.ok_or(Error::InvalidInput)?;
        if binding.purpose == CredentialPurpose::OauthAccessToken {
            return Err(Error::Denied);
        }
        self.mcp
            .lock()
            .map_err(|_| Error::Unavailable)?
            .insert(config.id.clone(), McpCredential { binding, value });
        Ok(())
    }

    pub fn clear_mcp(&self, connection_id: &str) -> Result<()> {
        self.mcp
            .lock()
            .map_err(|_| Error::Unavailable)?
            .remove(connection_id);
        Ok(())
    }

    pub fn contains_mcp(&self, config: &ConnectionConfig) -> bool {
        let Ok(Some(binding)) = config.credential_request() else {
            return false;
        };
        self.mcp.lock().is_ok_and(|credentials| {
            credentials
                .get(&config.id)
                .is_some_and(|credential| credential.binding == binding)
        })
    }

    pub fn payload_contains_secret(&self, payload: &serde_json::Value) -> bool {
        self.mcp.lock().is_ok_and(|credentials| {
            credentials
                .values()
                .any(|credential| contains_secret(payload, &credential.value))
        })
    }
}

struct ScopedSecrets {
    store: Arc<SessionSecrets>,
    endpoint: String,
}

impl SecretStore for ScopedSecrets {
    fn get(&self, reference: &str) -> Result<String> {
        let slot = self.store.cloud.lock().map_err(|_| Error::Unavailable)?;
        if let Some(key) = slot.as_ref().filter(|key| key.reference == reference) {
            // Editing the endpoint must never forward an existing session key to another server.
            return if key.endpoint == self.endpoint {
                Ok(key.value.clone())
            } else {
                Err(Error::AuthRequired)
            };
        }
        EnvironmentSecrets.get(reference)
    }
}

#[async_trait::async_trait]
impl CredentialResolver for SessionSecrets {
    fn resolve(&self, request: &CredentialRequest) -> Result<String> {
        if let Some(credential) = self
            .mcp
            .lock()
            .map_err(|_| Error::Unavailable)?
            .get(&request.connection_id)
        {
            return if credential.binding == *request {
                Ok(credential.value.clone())
            } else {
                Err(Error::AuthRequired)
            };
        }
        std::env::var(&request.secret_ref).map_err(|_| Error::AuthRequired)
    }
}

fn contains_secret(value: &serde_json::Value, secret: &str) -> bool {
    match value {
        serde_json::Value::String(value) => value.contains(secret),
        serde_json::Value::Array(values) => {
            values.iter().any(|value| contains_secret(value, secret))
        }
        serde_json::Value::Object(values) => {
            values.values().any(|value| contains_secret(value, secret))
        }
        _ => false,
    }
}
