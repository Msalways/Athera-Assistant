//! OpenAI Responses and compatible Chat Completions adapters. No SDK types escape.
mod stream;
use assistant_contracts::{protocol, *};
use async_trait::async_trait;
use reqwest::{
    header::{HeaderMap, RETRY_AFTER},
    Client, StatusCode, Url,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};

const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 1024;
const MAX_OUTPUT_TOKENS: u32 = 8192;
const DEFAULT_RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(60);
const MAX_RATE_LIMIT_COOLDOWN_SECONDS: u64 = 24 * 60 * 60;

const fn default_max_output_tokens() -> u32 {
    DEFAULT_MAX_OUTPUT_TOKENS
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKind {
    Responses,
    ChatCompletions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudConfig {
    pub id: String,
    pub endpoint: String,
    pub model: String,
    pub secret_ref: String,
    pub api: ApiKind,
    #[serde(default = "default_max_output_tokens")]
    pub max_output_tokens: u32,
}

/// Implement with Keystore-backed storage on Android. Never persist returned material.
pub trait SecretStore: Send + Sync {
    fn get(&self, reference: &str) -> Result<String>;
}
pub struct EnvironmentSecrets;
/// A reference names a backend environment variable, never the credential itself.
pub fn valid_secret_reference(reference: &str) -> bool {
    reference.starts_with("ASSISTANT_")
        && reference.ends_with("_KEY")
        && reference.len() <= 128
        && reference
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}
impl SecretStore for EnvironmentSecrets {
    fn get(&self, reference: &str) -> Result<String> {
        if !valid_secret_reference(reference) {
            return Err(Error::Denied);
        }
        std::env::var(reference)
            .ok()
            .filter(|secret| !secret.trim().is_empty())
            .ok_or(Error::AuthRequired)
    }
}

pub struct CloudProvider {
    config: CloudConfig,
    client: Client,
    secrets: Arc<dyn SecretStore>,
    cooldown_until: Mutex<Option<Instant>>,
}
impl CloudProvider {
    pub fn new(config: CloudConfig, secrets: Arc<dyn SecretStore>) -> Result<Self> {
        let url = Url::parse(&config.endpoint).map_err(|_| Error::InvalidInput)?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || config.model.is_empty()
            || !(1..=MAX_OUTPUT_TOKENS).contains(&config.max_output_tokens)
        {
            return Err(Error::InvalidInput);
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| Error::Unavailable)?;
        Ok(Self {
            config,
            client,
            secrets,
            cooldown_until: Mutex::new(None),
        })
    }
    pub fn request_body(&self, context: &ContextBundle) -> Result<Value> {
        let functions = protocol::functions(context);
        let packet = protocol::packet(context)?;
        Ok(match self.config.api {
            ApiKind::Responses => {
                json!({"model":self.config.model,"store":false,"instructions":protocol::POLICY,"input":packet,"max_output_tokens":self.config.max_output_tokens,"parallel_tool_calls":false,"tools":functions.into_iter().map(|mut f| { f["type"]=json!("function"); f["strict"]=json!(false); f }).collect::<Vec<_>>() })
            }
            ApiKind::ChatCompletions => {
                json!({"model":self.config.model,"messages":[{"role":"system","content":protocol::POLICY},{"role":"user","content":packet}],"max_tokens":self.config.max_output_tokens,"parallel_tool_calls":false,"tools":functions.into_iter().map(|f| json!({"type":"function","function":f})).collect::<Vec<_>>() })
            }
        })
    }

    fn check_cooldown(&self) -> Result<()> {
        let mut until = self.cooldown_until.lock().map_err(|_| Error::Unavailable)?;
        if until.is_some_and(|deadline| deadline > Instant::now()) {
            return Err(Error::RateLimited);
        }
        *until = None;
        Ok(())
    }

    fn handle_status(&self, status: StatusCode, headers: &HeaderMap) -> Result<()> {
        match status.as_u16() {
            200..=299 => Ok(()),
            401 | 403 => Err(Error::AuthRequired),
            429 => {
                let delay = retry_after(headers);
                *self.cooldown_until.lock().map_err(|_| Error::Unavailable)? =
                    Some(Instant::now() + delay);
                Err(Error::RateLimited)
            }
            _ => Err(Error::Unavailable),
        }
    }
    pub fn parse(&self, value: &Value, context: &ContextBundle) -> Result<AgentAction> {
        let mut calls = Vec::new();
        let mut texts = Vec::new();
        match self.config.api {
            ApiKind::Responses => {
                if value["status"].as_str().is_some_and(|s| s != "completed") {
                    return Err(Error::InvalidResponse);
                }
                for item in value["output"].as_array().ok_or(Error::InvalidResponse)? {
                    if item["type"] == "function_call" {
                        calls.push((
                            item["name"].as_str().ok_or(Error::InvalidResponse)?,
                            item["arguments"].as_str().ok_or(Error::InvalidResponse)?,
                        ));
                    }
                    if let Some(content) = item["content"].as_array() {
                        for part in content {
                            if part["type"] == "output_text" {
                                if let Some(text) = part["text"].as_str() {
                                    texts.push(text);
                                }
                            }
                        }
                    }
                }
            }
            ApiKind::ChatCompletions => {
                let choice = &value["choices"][0];
                if !matches!(
                    choice["finish_reason"].as_str(),
                    Some("stop" | "tool_calls")
                ) {
                    return Err(Error::InvalidResponse);
                }
                let message = &choice["message"];
                if let Some(items) = message["tool_calls"].as_array() {
                    for item in items {
                        calls.push((
                            item["function"]["name"]
                                .as_str()
                                .ok_or(Error::InvalidResponse)?,
                            item["function"]["arguments"]
                                .as_str()
                                .ok_or(Error::InvalidResponse)?,
                        ));
                    }
                }
                if let Some(text) = message["content"].as_str() {
                    texts.push(text);
                }
            }
        }
        match calls.as_slice() {
            [(name, args)] => protocol::decode_call(
                name,
                serde_json::from_str(args).map_err(|_| Error::InvalidResponse)?,
                context,
            ),
            [] if !texts.is_empty() => Ok(AgentAction::Respond {
                text: texts.join("\n"),
            }),
            _ => Err(Error::InvalidResponse),
        }
    }

    async fn send(&self, context: &ContextBundle, streaming: bool) -> Result<reqwest::Response> {
        self.check_cooldown()?;
        let secret = self.secrets.get(&self.config.secret_ref)?;
        let suffix = match self.config.api {
            ApiKind::Responses => "responses",
            ApiKind::ChatCompletions => "chat/completions",
        };
        let mut body = self.request_body(context)?;
        if streaming {
            body["stream"] = json!(true);
        }
        let response = self
            .client
            .post(format!(
                "{}/{suffix}",
                self.config.endpoint.trim_end_matches('/')
            ))
            .bearer_auth(secret)
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    Error::Timeout
                } else {
                    Error::Unavailable
                }
            })?;
        self.handle_status(response.status(), response.headers())?;
        Ok(response)
    }
}

fn retry_after(headers: &HeaderMap) -> Duration {
    let Some(value) = headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
    else {
        return DEFAULT_RATE_LIMIT_COOLDOWN;
    };
    let delay = value
        .trim()
        .parse::<u64>()
        .map(Duration::from_secs)
        .or_else(|_| {
            httpdate::parse_http_date(value).map(|deadline| {
                deadline
                    .duration_since(SystemTime::now())
                    .unwrap_or_default()
            })
        })
        .unwrap_or(DEFAULT_RATE_LIMIT_COOLDOWN);
    delay.min(Duration::from_secs(MAX_RATE_LIMIT_COOLDOWN_SECONDS))
}
#[async_trait]
impl ModelProvider for CloudProvider {
    fn id(&self) -> &str {
        &self.config.id
    }
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            tool_calls: true,
            planning: true,
            local: false,
        }
    }
    async fn infer(&self, context: ContextBundle) -> Result<AgentAction> {
        let mut response = self.send(&context, false).await?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::InvalidResponse)? {
            if bytes.len() + chunk.len() > 1_000_000 {
                return Err(Error::InvalidResponse);
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)?;
        self.parse(&value, &context)
    }

    async fn infer_stream(
        &self,
        context: ContextBundle,
        sink: ProviderEventSink,
    ) -> Result<AgentAction> {
        let mut response = self.send(&context, true).await?;
        let mut decoder = stream::Decoder::new(self.config.api);
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::InvalidResponse)? {
            decoder.push(&chunk, &sink)?;
        }
        decoder.finish(self, &context, &sink)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixtureSecrets;
    impl SecretStore for FixtureSecrets {
        fn get(&self, _: &str) -> Result<String> {
            Ok("fixture-not-a-real-key".into())
        }
    }

    fn provider() -> CloudProvider {
        CloudProvider::new(
            CloudConfig {
                id: "fixture".into(),
                endpoint: "https://example.com/v1".into(),
                model: "fixture".into(),
                secret_ref: "ASSISTANT_FIXTURE_KEY".into(),
                api: ApiKind::Responses,
                max_output_tokens: DEFAULT_MAX_OUTPUT_TOKENS,
            },
            Arc::new(FixtureSecrets),
        )
        .unwrap()
    }

    #[test]
    fn retry_after_seconds_starts_provider_cooldown() {
        let provider = provider();
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, "120".parse().unwrap());
        assert_eq!(
            provider.handle_status(StatusCode::TOO_MANY_REQUESTS, &headers),
            Err(Error::RateLimited)
        );
        assert_eq!(provider.check_cooldown(), Err(Error::RateLimited));
    }

    #[test]
    fn retry_after_http_date_starts_provider_cooldown() {
        let provider = provider();
        let mut headers = HeaderMap::new();
        headers.insert(
            RETRY_AFTER,
            httpdate::fmt_http_date(SystemTime::now() + Duration::from_secs(120))
                .parse()
                .unwrap(),
        );
        assert_eq!(
            provider.handle_status(StatusCode::TOO_MANY_REQUESTS, &headers),
            Err(Error::RateLimited)
        );
        assert_eq!(provider.check_cooldown(), Err(Error::RateLimited));
    }

    #[test]
    fn legacy_config_gets_bounded_output_default() {
        let config: CloudConfig = serde_json::from_value(json!({
            "id":"fixture",
            "endpoint":"https://example.com/v1",
            "model":"fixture",
            "secret_ref":"ASSISTANT_FIXTURE_KEY",
            "api":"responses"
        }))
        .unwrap();
        assert_eq!(config.max_output_tokens, DEFAULT_MAX_OUTPUT_TOKENS);
    }
}
