use assistant_contracts::*;
use provider_needle::NeedleProvider;
use serde_json::json;
#[tokio::main]
async fn main() -> Result<()> {
    let path = std::env::args().nth(1).ok_or(Error::InvalidInput)?;
    // SAFETY: this smoke runner is invoked only with the pinned archive verified by fetch-needle.ps1.
    let provider = unsafe { NeedleProvider::from_library(std::path::Path::new(&path)) }?;
    let context = ContextBundle {
        task_id: Id::new_v4(),
        role: Role::Fast,
        goal: "turn the flashlight on".into(),
        plan: vec![],
        handoff: None,
        history: vec![],
        results: vec![],
        skills: vec![],
        candidates: vec![],
        tools: vec![ToolSpec {
            id: "native.set_flashlight".into(),
            version: "1".into(),
            name: "set_flashlight".into(),
            description: "Turn the flashlight on or off".into(),
            input_schema: json!({"type":"object","properties":{"on":{"type":"boolean"}},"required":["on"],"additionalProperties":false}),
            output_schema: None,
            connection_id: "native".into(),
            source_tool: "set_flashlight".into(),
            risk: Risk::LocalSafeWrite,
            enabled: true,
            requires_auth: false,
            requires_network: false,
        }],
    };
    let started = std::time::Instant::now();
    let action = provider.infer(context).await?;
    println!(
        "{}",
        json!({"action":action,"latency_ms":started.elapsed().as_millis(),"device":"windows-host","executed":false})
    );
    match action {
        AgentAction::CallTool { call }
            if call.tool_id == "native.set_flashlight" && call.arguments == json!({"on":true}) =>
        {
            Ok(())
        }
        _ => Err(Error::InvalidResponse),
    }
}
