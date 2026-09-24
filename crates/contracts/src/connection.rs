//! Public connection state contains status only; credentials and authorization URLs stay native.
use crate::{Error, Id, Result};
use serde::{Deserialize, Serialize};

pub const CONNECTION_STATE_SCHEMA_V1: &str = "aethra.connection-state.v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Connected,
    Required,
    Connecting,
    Refreshing,
    Expired,
    StepUpRequired,
    Denied,
    Revoked,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConnectionState {
    pub schema: String,
    pub connection_id: String,
    pub service_name: String,
    pub state: ConnectionStatus,
    #[serde(default)]
    pub requested_scopes: Vec<String>,
    #[serde(default)]
    pub granted_scopes: Vec<String>,
    pub expires_at: Option<u64>,
    pub resume_task_id: Option<Id>,
    pub message: String,
}

impl ConnectionState {
    pub fn validate(&self) -> Result<()> {
        if self.schema != CONNECTION_STATE_SCHEMA_V1
            || self.connection_id.trim().is_empty()
            || self.connection_id.len() > 200
            || self.service_name.trim().is_empty()
            || self.service_name.len() > 200
            || self.requested_scopes.len() > 64
            || self.granted_scopes.len() > 64
            || self
                .requested_scopes
                .iter()
                .chain(&self.granted_scopes)
                .any(|scope| scope.trim().is_empty() || scope.len() > 500)
            || self.message.len() > 1_000
        {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_safe_public_connection_state() {
        let state = ConnectionState {
            schema: CONNECTION_STATE_SCHEMA_V1.into(),
            connection_id: "parallel".into(),
            service_name: "Parallel".into(),
            state: ConnectionStatus::Required,
            requested_scopes: vec!["tasks:read".into()],
            granted_scopes: vec![],
            expires_at: None,
            resume_task_id: None,
            message: "Connect Parallel to continue.".into(),
        };
        assert_eq!(state.validate(), Ok(()));
        let mut invalid = state;
        invalid.requested_scopes = vec![String::new()];
        assert_eq!(invalid.validate(), Err(Error::InvalidInput));
    }
}
