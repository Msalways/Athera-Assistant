use super::{ApiKind, CloudProvider};
use assistant_contracts::{
    AgentAction, ContextBundle, Error, ProviderEvent, ProviderEventSink, Result,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MAX_STREAM_BYTES: usize = 1_000_000;

pub(crate) struct Decoder {
    api: ApiKind,
    buffer: Vec<u8>,
    received: usize,
    response: Option<Value>,
    chat: Chat,
}

#[derive(Default)]
struct Chat {
    text: String,
    calls: BTreeMap<u64, ToolCall>,
    finish_reason: Option<String>,
}

#[derive(Default)]
struct ToolCall {
    name: String,
    arguments: String,
}

impl Decoder {
    pub(crate) fn new(api: ApiKind) -> Self {
        Self {
            api,
            buffer: Vec::new(),
            received: 0,
            response: None,
            chat: Chat::default(),
        }
    }

    pub(crate) fn push(&mut self, bytes: &[u8], sink: &ProviderEventSink) -> Result<()> {
        self.received = self
            .received
            .checked_add(bytes.len())
            .filter(|size| *size <= MAX_STREAM_BYTES)
            .ok_or(Error::InvalidResponse)?;
        self.buffer.extend_from_slice(bytes);
        while let Some(end) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = self.buffer.drain(..=end).collect();
            self.line(&line[..line.len() - 1], sink)?;
        }
        Ok(())
    }

    fn line(&mut self, line: &[u8], sink: &ProviderEventSink) -> Result<()> {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let Some(data) = line.strip_prefix(b"data:") else {
            return Ok(());
        };
        let data = std::str::from_utf8(data)
            .map_err(|_| Error::InvalidResponse)?
            .trim_start();
        if data.is_empty() || data == "[DONE]" {
            return Ok(());
        }
        let value: Value = serde_json::from_str(data).map_err(|_| Error::InvalidResponse)?;
        match self.api {
            ApiKind::Responses => self.response_event(value, sink),
            ApiKind::ChatCompletions => self.chat_event(value, sink),
        }
    }

    fn response_event(&mut self, value: Value, sink: &ProviderEventSink) -> Result<()> {
        match value["type"].as_str() {
            Some("response.output_text.delta") => emit(&value["delta"], sink),
            Some("response.completed") => {
                self.response = Some(value["response"].clone());
                Ok(())
            }
            Some("response.failed" | "error") => Err(Error::InvalidResponse),
            Some(_) => Ok(()),
            None => Err(Error::InvalidResponse),
        }
    }

    fn chat_event(&mut self, value: Value, sink: &ProviderEventSink) -> Result<()> {
        let choices = value["choices"].as_array().ok_or(Error::InvalidResponse)?;
        if choices.is_empty() {
            return Ok(());
        }
        if choices.len() != 1 || choices[0]["index"].as_u64().unwrap_or(0) != 0 {
            return Err(Error::InvalidResponse);
        }
        let choice = &choices[0];
        let delta = &choice["delta"];
        if let Some(text) = delta["content"].as_str() {
            emit(&Value::String(text.into()), sink)?;
            self.chat.text.push_str(text);
        }
        if let Some(calls) = delta["tool_calls"].as_array() {
            for call in calls {
                let index = call["index"].as_u64().ok_or(Error::InvalidResponse)?;
                let target = self.chat.calls.entry(index).or_default();
                if let Some(name) = call["function"]["name"].as_str() {
                    target.name.push_str(name);
                }
                if let Some(arguments) = call["function"]["arguments"].as_str() {
                    target.arguments.push_str(arguments);
                }
            }
        }
        if let Some(reason) = choice["finish_reason"].as_str() {
            self.chat.finish_reason = Some(reason.into());
        }
        Ok(())
    }

    pub(crate) fn finish(
        mut self,
        provider: &CloudProvider,
        context: &ContextBundle,
        sink: &ProviderEventSink,
    ) -> Result<AgentAction> {
        let remaining = std::mem::take(&mut self.buffer);
        if !remaining.iter().all(u8::is_ascii_whitespace) {
            self.line(&remaining, sink)?;
        }
        match self.api {
            ApiKind::Responses => provider.parse(
                &self.response.take().ok_or(Error::InvalidResponse)?,
                context,
            ),
            ApiKind::ChatCompletions => {
                let finish_reason = self.chat.finish_reason.ok_or(Error::InvalidResponse)?;
                let calls: Vec<_> = self
                    .chat
                    .calls
                    .into_values()
                    .map(|call| json!({"function":{"name":call.name,"arguments":call.arguments}}))
                    .collect();
                provider.parse(
                    &json!({"choices":[{"finish_reason":finish_reason,"message":{"content":self.chat.text,"tool_calls":calls}}]}),
                    context,
                )
            }
        }
    }
}

fn emit(value: &Value, sink: &ProviderEventSink) -> Result<()> {
    let text = value.as_str().ok_or(Error::InvalidResponse)?;
    if text.is_empty() || text.len() > 16_000 {
        return Err(Error::InvalidResponse);
    }
    sink(ProviderEvent::TextDelta { text: text.into() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use assistant_contracts::{ModelProvider, Role};
    use std::sync::{Arc, Mutex};

    fn context() -> ContextBundle {
        ContextBundle {
            task_id: assistant_contracts::Id::new_v4(),
            role: Role::Reasoner,
            goal: "answer".into(),
            plan: vec![],
            handoff: None,
            history: vec![],
            results: vec![],
            skills: vec![],
            candidates: vec![],
            tools: vec![],
        }
    }

    fn provider(api: ApiKind) -> CloudProvider {
        CloudProvider::new(
            super::super::CloudConfig {
                id: "fixture".into(),
                endpoint: "https://example.com/v1".into(),
                model: "fixture".into(),
                secret_ref: "ASSISTANT_FIXTURE_KEY".into(),
                api,
                max_output_tokens: 64,
            },
            Arc::new(super::super::EnvironmentSecrets),
        )
        .unwrap()
    }

    fn sink() -> (ProviderEventSink, Arc<Mutex<Vec<String>>>) {
        let deltas = Arc::new(Mutex::new(Vec::new()));
        let captured = deltas.clone();
        let sink = Arc::new(move |event| {
            let ProviderEvent::TextDelta { text } = event;
            captured.lock().unwrap().push(text);
            Ok(())
        });
        (sink, deltas)
    }

    #[test]
    fn responses_stream_emits_fragmented_deltas_and_finishes_from_final_response() {
        let provider = provider(ApiKind::Responses);
        let (sink, deltas) = sink();
        let mut decoder = Decoder::new(ApiKind::Responses);
        decoder
            .push(
                b"data: {\"type\":\"response.output_text.delta\",\"del",
                &sink,
            )
            .unwrap();
        decoder
            .push(b"ta\":\"Hel\"}\n\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"lo\"}\n", &sink)
            .unwrap();
        decoder
            .push(b"data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[{\"content\":[{\"type\":\"output_text\",\"text\":\"Hello\"}]}]}}", &sink)
            .unwrap();
        let action = decoder.finish(&provider, &context(), &sink).unwrap();
        assert!(matches!(action, AgentAction::Respond { text } if text == "Hello"));
        assert_eq!(*deltas.lock().unwrap(), ["Hel", "lo"]);
    }

    #[test]
    fn chat_stream_reassembles_tool_arguments_without_emitting_them_as_text() {
        let provider = provider(ApiKind::ChatCompletions);
        let (sink, deltas) = sink();
        let mut decoder = Decoder::new(ApiKind::ChatCompletions);
        for line in [
            r#"data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"name":"capabilities_search","arguments":"{\"que"}}]},"finish_reason":null}]}"#,
            r#"data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ry\":\"weather\"}"}}]},"finish_reason":"tool_calls"}]}"#,
            "data: [DONE]",
        ] {
            decoder
                .push(format!("{line}\n\n").as_bytes(), &sink)
                .unwrap();
        }
        let action = decoder.finish(&provider, &context(), &sink).unwrap();
        assert!(matches!(action, AgentAction::Search { query } if query == "weather"));
        assert!(deltas.lock().unwrap().is_empty());
        assert!(provider.capabilities().planning);
    }
}
