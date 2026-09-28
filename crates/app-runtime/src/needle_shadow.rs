//! Shadow driver for the on-device model.
//!
//! The on-device engine ships as a prebuilt binary per platform: a static
//! library on Android, a standalone executable elsewhere. The static library is
//! the shipping integration, but an executable exposes the same engine over a
//! local HTTP port, which is what lets the shadow path be exercised on a
//! development machine without pretending to be the final binding.
//!
//! This type only ever *observes*. It proposes a capability and reports a
//! confidence; it never produces an action the engine will run. Turning a
//! proposal into a call is a separate, deliberate step that shadow mode must
//! never take on its own.
//!
//! Telemetry is off by default in the upstream binary. An on-device assistant
//! that claims to keep data on the device cannot ship a component that phones
//! home unless the user asked for that, so it is disabled unconditionally here.

use serde_json::Value;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

/// A single observation of what the on-device model proposed.
#[derive(Debug, Clone, PartialEq)]
pub struct ShadowProposal {
    pub tool: Option<String>,
    pub arguments: Value,
    /// `None` when the loaded weights carry no calibration head, which means the
    /// proposal cannot be gated on confidence at all.
    pub confidence: Option<f64>,
    pub latency_ms: u64,
}

/// The engine process and the address it was bound to. Held together so a
/// restart can never leave a stale address behind a live process.
struct Engine {
    child: Option<Child>,
    base: String,
    /// Whether the engine can actually answer yet. A live process is not a ready
    /// one: it spends seconds mapping its weights before it accepts a turn, and
    /// treating "spawned" as "ready" made concurrent observers give up.
    ready: bool,
}

pub struct NeedleShadow {
    engine: PathBuf,
    model: PathBuf,
    tools: PathBuf,
    system: PathBuf,
    state: Arc<Mutex<Engine>>,
    started: AtomicU64,
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

impl NeedleShadow {
    /// Build a driver from the environment, or `None` when the engine has not
    /// been provisioned. Absent is a normal state, not an error: the app runs
    /// without a local model and simply produces no observations.
    pub fn from_env() -> Option<Self> {
        let engine = env_path("ASSISTANT_NEEDLE_ENGINE")?;
        let model = env_path("ASSISTANT_NEEDLE_MODEL")?;
        let tools = env_path("ASSISTANT_NEEDLE_TOOLS")?;
        let system = env_path("ASSISTANT_NEEDLE_SYSTEM")?;
        Some(Self::new(engine, model, tools, system))
    }

    pub fn new(engine: PathBuf, model: PathBuf, tools: PathBuf, system: PathBuf) -> Self {
        Self {
            engine,
            model,
            tools,
            system,
            state: Arc::new(Mutex::new(Engine {
                child: None,
                base: String::new(),
                ready: false,
            })),
            started: AtomicU64::new(0),
        }
    }

    /// Ask the engine what it would do, without acting on it.
    ///
    /// Returns `None` when the engine is absent or unhealthy. Shadow mode is an
    /// observation, so every failure here is silent by design: a missing local
    /// model must never degrade a turn the cloud model is already handling.
    pub async fn observe(&self, packet: &str) -> Option<ShadowProposal> {
        self.ensure_running().await?;
        // Time only the turn itself. The first observation also pays a weight
        // load and every observation queues behind the one before it, so
        // including either would report this app's scheduling as the model's
        // speed, which is the number that decides whether it ships.
        let started = std::time::Instant::now();
        // Each observation is a fresh turn, exactly as the engine is driven in
        // production. Without this the engine keeps the previous observations in
        // context and the confidences drift away from a single-turn answer.
        self.post("reset", &serde_json::json!({})).await;
        let response = self
            .post("complete", &serde_json::json!({ "input": packet }))
            .await?;
        let latency = started.elapsed().as_millis() as u64;
        Some(parse_proposal(&response, latency))
    }

    async fn ensure_running(&self) -> Option<()> {
        // The lock is held across the whole load, not just the spawn. The engine
        // is process-global and answers one turn at a time, so observers must
        // queue here anyway; letting them past a half-loaded engine is what made
        // them fail.
        let mut guard = self.state.lock().await;
        if guard.ready
            && guard
                .child
                .as_mut()
                .is_some_and(|child| child.try_wait().ok().flatten().is_none())
        {
            return Some(());
        }
        guard.child = None;
        guard.base.clear();
        guard.ready = false;
        // A shadow driver holds one engine for the process lifetime, so a
        // restarted engine rebinds the next port rather than guessing.
        let port = 8400 + (self.started.fetch_add(1, Ordering::Relaxed) as u16 % 400);
        let child = Command::new(&self.engine)
            .arg("--model")
            .arg(&self.model)
            .arg("--tools")
            .arg(&self.tools)
            .arg("--system")
            .arg(&self.system)
            .arg("--serve")
            .arg("--port")
            .arg(port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            // Upstream ships telemetry on. Off, always: this component runs
            // beside conversations the user believes stay on their device.
            .env("NEEDLE_TELEMETRY", "0")
            .env("DO_NOT_TRACK", "1")
            .spawn()
            .ok()?;
        guard.base = format!("http://127.0.0.1:{port}");
        guard.child = Some(child);
        let base = guard.base.clone();
        // The engine maps its weights before it can answer. Readiness is proven
        // by a real turn succeeding, not by the process merely existing.
        for _ in 0..120 {
            if post_json(
                &format!("{base}/complete"),
                &serde_json::json!({ "input": "ready" }),
            )
            .await
            .is_some()
            {
                guard.ready = true;
                return Some(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        None
    }

    async fn post(&self, path: &str, body: &Value) -> Option<Value> {
        let base = self.state.lock().await.base.clone();
        if base.is_empty() {
            return None;
        }
        // The engine is process-global and one turn at a time; the request
        // timeout is generous because the first turn pays the weight load.
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            post_json(&format!("{base}/{path}"), body),
        )
        .await
        .ok()
        .flatten()
    }
}

async fn post_json(url: &str, body: &Value) -> Option<Value> {
    // A hand-rolled HTTP/1.1 request keeps this crate free of an async HTTP
    // client dependency for one loopback call.
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (authority, route) = url.trim_start_matches("http://").split_once('/')?;
    let payload = serde_json::to_vec(body).ok()?;
    let mut stream = tokio::net::TcpStream::connect(authority).await.ok()?;
    let request = format!(
        "POST /{route} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    stream.write_all(request.as_bytes()).await.ok()?;
    stream.write_all(&payload).await.ok()?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.ok()?;
    let text = String::from_utf8_lossy(&raw);
    let body = text.split("\r\n\r\n").nth(1)?;
    serde_json::from_str(body.trim()).ok()
}

fn parse_proposal(response: &Value, latency_ms: u64) -> ShadowProposal {
    let calls = response.get("function_calls").and_then(Value::as_array);
    let first = calls.and_then(|c| c.first());
    let confidence = response
        .get("confidence")
        .and_then(Value::as_f64)
        .filter(|c| (0.0..=1.0).contains(c));
    match first {
        Some(call) => ShadowProposal {
            tool: call.get("name").and_then(Value::as_str).map(str::to_owned),
            arguments: call.get("arguments").cloned().unwrap_or(Value::Null),
            confidence,
            latency_ms,
        },
        // No call is a refusal, not a failure. Recording it is the point: a
        // model that declines is behaving correctly.
        None => ShadowProposal {
            tool: None,
            arguments: Value::Null,
            confidence,
            latency_ms,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_becomes_a_proposal_with_its_confidence() {
        let response = serde_json::json!({
            "type": "call",
            "function_calls": [{"name": "tool_1", "arguments": {"city": "Lagos"}}],
            "confidence": 0.91
        });
        let p = parse_proposal(&response, 120);
        assert_eq!(p.tool.as_deref(), Some("tool_1"));
        assert_eq!(p.arguments["city"], "Lagos");
        assert_eq!(p.confidence, Some(0.91));
    }

    #[test]
    fn a_refusal_is_an_abstention_not_an_error() {
        let response =
            serde_json::json!({"type": "call", "function_calls": [], "confidence": 0.98});
        let p = parse_proposal(&response, 40);
        assert!(p.tool.is_none(), "declining must not read as a tool choice");
        assert_eq!(p.confidence, Some(0.98));
    }

    #[test]
    fn a_missing_or_impossible_confidence_is_reported_as_uncalibrated() {
        let no_score = serde_json::json!({"function_calls": [], "confidence": null});
        assert_eq!(parse_proposal(&no_score, 1).confidence, None);
        let nonsense = serde_json::json!({"function_calls": [], "confidence": 7.0});
        assert_eq!(parse_proposal(&nonsense, 1).confidence, None);
    }
}
