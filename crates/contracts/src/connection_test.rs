use crate::provider_health::NormalizedTestFailure;
use serde::{Deserialize, Serialize};

/// Schema version for connection test results.
pub const CONNECTION_TEST_SCHEMA_V1: &str = "aethra.connection-test.v1";

/// Result of a provider connection test.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectionTestResult {
    pub schema: String,
    pub success: bool,
    pub failure_kind: Option<NormalizedTestFailure>,
    pub model_id: Option<String>,
    pub latency_ms: Option<u64>,
    pub message: String,
}

impl ConnectionTestResult {
    pub fn success(model_id: &str, latency_ms: u64) -> Self {
        Self {
            schema: CONNECTION_TEST_SCHEMA_V1.into(),
            success: true,
            failure_kind: None,
            model_id: Some(model_id.into()),
            latency_ms: Some(latency_ms),
            message: format!("Connected to {model_id}"),
        }
    }

    pub fn failure(kind: NormalizedTestFailure, message: &str) -> Self {
        Self {
            schema: CONNECTION_TEST_SCHEMA_V1.into(),
            success: false,
            failure_kind: Some(kind),
            model_id: None,
            latency_ms: None,
            message: message.into(),
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != CONNECTION_TEST_SCHEMA_V1 {
            return Err("unsupported connection test schema");
        }
        if self.message.is_empty() {
            return Err("message is required");
        }
        Ok(())
    }
}

/// A provider connection test service.
/// Tests whether a resolved model client can reach the provider.
pub struct ConnectionTestService;

impl ConnectionTestService {
    /// Classify an HTTP status code into a normalized failure kind.
    pub fn classify_http_status(status: u16) -> NormalizedTestFailure {
        match status {
            401 | 403 => NormalizedTestFailure::Credential,
            404 => NormalizedTestFailure::ModelNotFound,
            429 => NormalizedTestFailure::Quota,
            500..=599 => NormalizedTestFailure::ProviderError,
            _ => NormalizedTestFailure::Unknown,
        }
    }

    /// Classify a connection error string into a normalized failure kind.
    pub fn classify_connection_error(error: &str) -> NormalizedTestFailure {
        let lower = error.to_lowercase();
        if lower.contains("connection refused") || lower.contains("dns") {
            NormalizedTestFailure::Endpoint
        } else if lower.contains("timeout")
            || lower.contains("timed out")
            || lower.contains("tls")
            || lower.contains("ssl")
        {
            NormalizedTestFailure::Network
        } else {
            NormalizedTestFailure::Unknown
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_result() {
        let r = ConnectionTestResult::success("gpt-4", 150);
        assert!(r.success);
        assert_eq!(r.model_id.as_deref(), Some("gpt-4"));
        assert_eq!(r.latency_ms, Some(150));
    }

    #[test]
    fn failure_result() {
        let r = ConnectionTestResult::failure(NormalizedTestFailure::Credential, "bad key");
        assert!(!r.success);
        assert_eq!(r.failure_kind, Some(NormalizedTestFailure::Credential));
    }

    #[test]
    fn classify_401_as_credential() {
        assert_eq!(
            ConnectionTestService::classify_http_status(401),
            NormalizedTestFailure::Credential
        );
    }

    #[test]
    fn classify_429_as_quota() {
        assert_eq!(
            ConnectionTestService::classify_http_status(429),
            NormalizedTestFailure::Quota
        );
    }

    #[test]
    fn classify_404_as_model_not_found() {
        assert_eq!(
            ConnectionTestService::classify_http_status(404),
            NormalizedTestFailure::ModelNotFound
        );
    }

    #[test]
    fn classify_connection_refused() {
        assert_eq!(
            ConnectionTestService::classify_connection_error("connection refused"),
            NormalizedTestFailure::Endpoint
        );
    }

    #[test]
    fn classify_timeout() {
        assert_eq!(
            ConnectionTestService::classify_connection_error("request timed out"),
            NormalizedTestFailure::Network
        );
    }

    #[test]
    fn validate_empty_message_rejected() {
        let r = ConnectionTestResult {
            schema: CONNECTION_TEST_SCHEMA_V1.into(),
            success: true,
            failure_kind: None,
            model_id: None,
            latency_ms: None,
            message: String::new(),
        };
        assert_eq!(r.validate(), Err("message is required"));
    }
}
