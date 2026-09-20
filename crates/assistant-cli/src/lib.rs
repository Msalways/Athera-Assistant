//! Deterministic evaluation entry point shared by JSON-lines and bridge parity checks.
use assistant_contracts::*;
use assistant_core::{
    registry,
    testing::{EchoExecutor, ScriptedExecutor, ScriptedProvider},
    Assistant,
};
use serde_json::{json, Value};
use std::sync::Arc;
use storage_sqlite::SqliteStore;

pub async fn evaluate_fixture_report(scenario: Value) -> Value {
    match evaluate(scenario).await {
        Ok(mut value) => {
            let status = value
                .pointer("/task/status")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned();
            if status == "completed" {
                value["result"] = json!("pass");
            } else {
                value["result"] = json!("fail");
                value["failure"] = json!(if status == "failed" {
                    "task_failed"
                } else {
                    "task_unfinished"
                });
            }
            value
        }
        Err(error) => json!({"result":"fail","error":error}),
    }
}

async fn evaluate(scenario: Value) -> Result<Value> {
    let store = Arc::new(SqliteStore::memory()?);
    let capabilities: Vec<Capability> =
        serde_json::from_value(scenario.get("capabilities").cloned().unwrap_or(json!([])))
            .map_err(|_| Error::InvalidInput)?;
    for capability in capabilities {
        registry::register(store.as_ref(), &capability)?;
    }
    let actions = |key: &str| {
        serde_json::from_value::<Vec<AgentAction>>(scenario.get(key).cloned().unwrap_or(json!([])))
            .map_err(|_| Error::InvalidInput)
    };
    let events = |key: &str| {
        serde_json::from_value::<Vec<Vec<ProviderEvent>>>(
            scenario.get(key).cloned().unwrap_or(json!([])),
        )
        .map_err(|_| Error::InvalidInput)
    };
    let tool_results = serde_json::from_value::<Vec<Value>>(
        scenario.get("tool_results").cloned().unwrap_or(json!([])),
    )
    .map_err(|_| Error::InvalidInput)?;
    let executor: Arc<dyn ToolExecutor> = if tool_results.is_empty() {
        Arc::new(EchoExecutor)
    } else {
        Arc::new(ScriptedExecutor::new(tool_results))
    };
    let assistant = Assistant::new(
        store.clone(),
        Arc::new(ScriptedProvider::with_events(
            true,
            actions("fast")?,
            events("fast_events")?,
        )),
        Arc::new(ScriptedProvider::with_events(
            false,
            actions("cloud")?,
            events("cloud_events")?,
        )),
        executor,
        EngineConfig::default(),
    );
    let input = UserInput {
        conversation_id: Id::new_v4(),
        text: scenario["input"]
            .as_str()
            .ok_or(Error::InvalidInput)?
            .into(),
        source: InputSource::Text,
    };
    let task = assistant.submit(input)?;
    let task = assistant.run(task.id).await?;
    verify_expected(&scenario, &task)?;
    let results = task
        .result_refs
        .iter()
        .map(|id| store.result(*id))
        .collect::<Result<Vec<_>>>()?;
    Ok(
        json!({"task":task,"events":store.events(0)?,"results":results,"mode":"deterministic_fixture"}),
    )
}

fn verify_expected(scenario: &Value, task: &Task) -> Result<()> {
    let Some(expected) = scenario.get("expected") else {
        return Ok(());
    };
    if expected
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| {
            serde_json::to_value(task.status).ok().as_ref() != Some(&json!(status))
        })
    {
        return Err(Error::InvalidResponse);
    }
    if let Some(expected_ids) = expected.get("tool_ids") {
        let mut actual = task
            .call_counts
            .keys()
            .filter_map(|value| serde_json::from_str::<ToolCall>(value).ok())
            .map(|call| call.tool_id)
            .collect::<Vec<_>>();
        actual.sort();
        if json!(actual) != *expected_ids {
            return Err(Error::InvalidResponse);
        }
    }
    if let Some(expected_urls) = expected.get("source_urls") {
        let mut actual = task
            .output
            .iter()
            .flat_map(|output| &output.blocks)
            .filter_map(|block| match block {
                OutputBlock::Sources { sources, .. } => Some(sources),
                _ => None,
            })
            .flatten()
            .map(|source| source.url.clone())
            .collect::<Vec<_>>();
        actual.sort();
        if json!(actual) != *expected_urls {
            return Err(Error::InvalidResponse);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn setup_fixture_completes_through_the_real_core() {
        let result = evaluate_fixture_report(json!({
            "input": "Run the deterministic setup check.",
            "fast": [{"type": "respond", "text": "Setup completed."}],
            "cloud": []
        }))
        .await;
        assert_eq!(result["result"], "pass");
        assert_eq!(result["task"]["status"], "completed");
        assert!(result["events"]
            .as_array()
            .is_some_and(|events| !events.is_empty()));
    }

    #[tokio::test]
    async fn failed_and_malformed_runs_have_machine_readable_failures() {
        let failed = evaluate_fixture_report(json!({
            "input": "Fail this deterministic task.",
            "fast": [{"type": "fail", "reason": "Scripted failure."}],
            "cloud": []
        }))
        .await;
        assert_eq!(failed["result"], "fail");
        assert_eq!(failed["failure"], "task_failed");

        let malformed = evaluate_fixture_report(json!(null)).await;
        assert_eq!(malformed, json!({"result":"fail","error":"invalid_input"}));

        let mismatch = evaluate_fixture_report(json!({
            "input":"Complete",
            "fast":[{"type":"respond","text":"done"}],
            "expected":{"status":"failed"}
        }))
        .await;
        assert_eq!(
            mismatch,
            json!({"result":"fail","error":"invalid_response"})
        );
    }
}
