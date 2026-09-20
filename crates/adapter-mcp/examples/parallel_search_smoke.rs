use adapter_mcp::{ConnectionConfig, McpManager, PARALLEL_SEARCH_URL};
use assistant_contracts::{ToolCall, ToolExecutor, ToolResultRecord, WebToolContext};
use serde_json::{json, Map, Value};

#[tokio::main]
async fn main() {
    let query = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "What changed in the latest Parallel Search MCP documentation?".into());
    let manager = McpManager::default();
    let tools = manager
        .connect(&ConnectionConfig::parallel_search())
        .await
        .expect("anonymous Parallel Search MCP discovery failed");
    let mut search = tools
        .into_iter()
        .find(|tool| tool.id == "web.search")
        .expect("Parallel did not advertise web_search");
    search.enabled = true;
    let call = ToolCall {
        tool_id: search.id.clone(),
        version: search.version.clone(),
        arguments: arguments(&search.input_schema, &query),
    };
    let value = manager
        .execute(&search, &call)
        .await
        .expect("anonymous Parallel web_search failed");
    let record: ToolResultRecord =
        serde_json::from_value(value).expect("normalized result contract changed");
    let context: WebToolContext = serde_json::from_value(record.model_context)
        .expect("normalized web context contract changed");
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "mode":"live_anonymous",
            "endpoint":PARALLEL_SEARCH_URL,
            "query":query,
            "retrieved_at":context.retrieved_at,
            "source_count":context.sources.len(),
            "partial":context.partial,
            "sources":context.sources.into_iter().map(|source| json!({
                "id":source.id,
                "title":source.title,
                "url":source.url
            })).collect::<Vec<_>>()
        }))
        .expect("report serialization failed")
    );
}

fn arguments(schema: &Value, query: &str) -> Value {
    let mut values = Map::new();
    let required = schema["required"].as_array().cloned().unwrap_or_default();
    for name in required.iter().filter_map(Value::as_str) {
        let kind = schema["properties"][name]["type"].as_str();
        let value = match (name, kind) {
            ("search_queries", _) | (_, Some("array")) => json!([query]),
            ("objective" | "query", _) | (_, Some("string")) => json!(query),
            (_, Some("integer" | "number")) => json!(1),
            (_, Some("boolean")) => json!(false),
            _ => Value::Null,
        };
        values.insert(name.to_owned(), value);
    }
    if values.is_empty() {
        values.insert("query".into(), json!(query));
    }
    Value::Object(values)
}
