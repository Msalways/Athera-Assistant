//! Real host inference, JSON lines. No tools execute; quality is reported separately.
use assistant_contracts::*;
use assistant_core::sms::{draft_tool, operation, packet};
use provider_needle::NeedleProvider;
use serde_json::json;
#[tokio::main]
async fn main() -> Result<()> {
    let path = std::env::args().nth(1).ok_or(Error::InvalidInput)?;
    let provider = NeedleProvider::verified_windows(std::path::Path::new(&path))?;
    let cases = [
        ("competing_tools", "Open the SMS composer for +15551234567 with message Hello.", vec![operation("inspect_screen"), operation("open_composer"), operation("enter_text")], None, Some(("sms.open_composer", json!({"recipient":"+15551234567","message":"Hello."})))),
        ("argument_extraction", "Enter exactly I will arrive at 6 pm. in message editor target 3 at revision 12.", vec![operation("enter_text"), operation("select_element"), operation("inspect_screen")], None, Some(("sms.enter_text", json!({"target":3,"revision":12,"text":"I will arrive at 6 pm."})))),
        ("rewriting", "Write a warmer and more polite SMS from this wording: running late wait 10 mins. Return final wording in propose_draft.message.", vec![draft_tool()], None, None),
        ("reported_android_draft", "Use the only available tool. Set message to a clear SMS based on this wording: Hey leave tomorrow take care of tasks", vec![draft_tool()], None, None),
        ("ambiguous_instruction", "Send it to Alex. You do not know which Alex, phone number, or what message. Ask for assistance.", vec![operation("open_composer"), draft_tool(), operation("inspect_screen")], None, None),
        ("screen_before_entry", "Enter the approved text Hello. in the current message editor. Use current revision and target.", vec![operation("inspect_screen"),operation("enter_text"),operation("select_element")], Some(json!({"revision":4,"elements":[{"id":1,"kind":"message","text":""},{"id":2,"kind":"send"}]})), Some(("sms.enter_text",json!({"revision":4,"target":1,"text":"Hello."})))),
        ("screen_after_entry", "The approved text Hello. is now entered. Select the send button using the current revision and target.", vec![operation("inspect_screen"),operation("enter_text"),operation("select_element")], Some(json!({"revision":5,"elements":[{"id":1,"kind":"message","text":"Hello."},{"id":2,"kind":"send"}]})), Some(("sms.select_element",json!({"revision":5,"target":2})))),
    ];
    for (name, goal, tools, screen, expected) in cases {
        let results = screen
            .into_iter()
            .map(|s| ResultExcerpt {
                id: Id::new_v4(),
                untrusted_data: s.to_string(),
            })
            .collect();
        let started = std::time::Instant::now();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            provider.infer(packet(Id::new_v4(), goal.into(), tools, results)),
        )
        .await
        .unwrap_or(Err(Error::Timeout));
        let exact_match = expected.map(|(id, args)| matches!(&result, Ok(AgentAction::CallTool { call }) if call.tool_id == id && call.arguments == args));
        println!(
            "{}",
            json!({"case":name,"host":"windows","latency_ms":started.elapsed().as_millis(),"executed":false,"exact_tool_and_arguments":exact_match,"draft_quality":if name=="rewriting" {"review_actual_wording_separately"} else {"not_applicable"},"result":result})
        );
    }
    Ok(())
}
