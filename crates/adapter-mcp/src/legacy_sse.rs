//! Pins legacy SSE's server-advertised message endpoint to the configured HTTPS origin.
use http::Uri;
use reqwest::Url;
use rmcp::{
    model::ClientJsonRpcMessage,
    transport::{
        common::client_side_sse::BoxedSseResponse,
        sse_client::{SseClient, SseTransportError},
    },
};
use std::fmt;

#[derive(Clone)]
pub(crate) struct OriginPinnedSseClient {
    client: reqwest::Client,
    origin: String,
    bearer: Option<String>,
}

impl OriginPinnedSseClient {
    pub(crate) fn new(client: reqwest::Client, endpoint: Url, bearer: Option<String>) -> Self {
        Self {
            client,
            origin: endpoint.origin().ascii_serialization(),
            bearer,
        }
    }

    fn check_uri(&self, uri: &Uri) -> Result<(), SseTransportError<reqwest::Error>> {
        let allowed = Url::parse(&uri.to_string()).is_ok_and(|url| {
            url.scheme() == "https"
                && url.origin().ascii_serialization() == self.origin
                && url.username().is_empty()
                && url.password().is_none()
        });
        if allowed {
            Ok(())
        } else {
            Err(SseTransportError::Io(std::io::Error::other(
                "SSE endpoint changed origin",
            )))
        }
    }
}

impl fmt::Debug for OriginPinnedSseClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OriginPinnedSseClient")
            .field("origin", &self.origin)
            .field("bearer", &self.bearer.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

impl SseClient for OriginPinnedSseClient {
    type Error = reqwest::Error;

    async fn post_message(
        &self,
        uri: Uri,
        message: ClientJsonRpcMessage,
        _auth_token: Option<String>,
    ) -> Result<(), SseTransportError<Self::Error>> {
        self.check_uri(&uri)?;
        self.client
            .post_message(uri, message, self.bearer.clone())
            .await
    }

    async fn get_stream(
        &self,
        uri: Uri,
        last_event_id: Option<String>,
        _auth_token: Option<String>,
    ) -> Result<BoxedSseResponse, SseTransportError<Self::Error>> {
        self.check_uri(&uri)?;
        self.client
            .get_stream(uri, last_event_id, self.bearer.clone())
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_chosen_sse_endpoints_cannot_redirect_credentials() {
        let client = OriginPinnedSseClient::new(
            reqwest::Client::new(),
            Url::parse("https://mcp.example/sse").unwrap(),
            Some("fixture-secret".into()),
        );
        assert!(client
            .check_uri(&"https://mcp.example/messages?session=1".parse().unwrap())
            .is_ok());
        assert!(client
            .check_uri(&"https://evil.example/messages".parse().unwrap())
            .is_err());
        assert!(client
            .check_uri(&"http://mcp.example/messages".parse().unwrap())
            .is_err());
        assert!(!format!("{client:?}").contains("fixture-secret"));
    }
}
