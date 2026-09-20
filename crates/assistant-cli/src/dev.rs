//! Loopback-only development bridge. Production uses Tauri IPC.
use app_runtime::Runtime;
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{uri::Authority, HeaderMap, StatusCode, Uri},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database =
        std::env::var_os("ASSISTANT_DEV_DATABASE").unwrap_or_else(|| "assistant.db".into());
    let runtime = Arc::new(Runtime::open(database)?);
    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/command", post(command))
        .route("/api/evaluate-fixture", post(evaluate_fixture))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .with_state(runtime);
    let port: u16 = std::env::var("ASSISTANT_DEV_PORT")
        .unwrap_or_else(|_| "8787".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    println!("Rust development bridge: http://{}", listener.local_addr()?);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
async fn health() -> Json<Value> {
    Json(json!({"service": "assistant-dev", "status": "ready"}))
}

async fn evaluate_fixture(
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    if std::env::var("ASSISTANT_DEV_FIXTURE_MODE").as_deref() != Ok("1") || !local_client(&headers)
    {
        return (StatusCode::FORBIDDEN, Json(json!({"error":"denied"})));
    }
    (
        StatusCode::OK,
        Json(assistant_cli::evaluate_fixture_report(body).await),
    )
}

async fn command(
    State(runtime): State<Arc<Runtime>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    if !local_client(&headers) {
        return (StatusCode::FORBIDDEN, Json(json!({"error":"denied"})));
    }
    let Some(name) = body["name"].as_str() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_input"})),
        );
    };
    match runtime.dispatch(name, body["payload"].clone()).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        ),
    }
}

fn loopback_host(host: &str) -> bool {
    host == "127.0.0.1" || host.eq_ignore_ascii_case("localhost")
}

fn local_client(headers: &HeaderMap) -> bool {
    // Custom header/no CORS blocks cross-site writes; Host validation blocks DNS rebinding.
    if headers
        .get("x-assistant-client")
        .and_then(|s| s.to_str().ok())
        != Some("local-ui")
    {
        return false;
    }
    if !headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.parse::<Authority>().ok())
        .is_some_and(|a| loopback_host(a.host()))
    {
        return false;
    }
    match headers.get("origin") {
        None => true,
        Some(origin) => origin
            .to_str()
            .ok()
            .and_then(|o| o.parse::<Uri>().ok())
            .is_some_and(|uri| {
                uri.scheme_str() == Some("http") && uri.host().is_some_and(loopback_host)
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bridge_rejects_cross_origin_and_rebound_hosts() {
        let mut headers = HeaderMap::new();
        headers.insert("x-assistant-client", "local-ui".parse().unwrap());
        headers.insert("host", "127.0.0.1:1421".parse().unwrap());
        headers.insert("origin", "http://127.0.0.1:1421".parse().unwrap());
        assert!(local_client(&headers));
        for origin in [
            "https://untrusted.example",
            "null",
            "http://127.0.0.1.untrusted.example",
        ] {
            headers.insert("origin", origin.parse().unwrap());
            assert!(!local_client(&headers));
        }
        headers.remove("origin");
        assert!(local_client(&headers));
        headers.insert("host", "rebound.example:8787".parse().unwrap());
        assert!(!local_client(&headers));
        headers.insert("host", "127.0.0.1:8787".parse().unwrap());
        headers.remove("x-assistant-client");
        assert!(!local_client(&headers));
    }
}
