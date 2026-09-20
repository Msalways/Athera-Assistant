//! Real local-model smoke via the same dispatch commands used by Tauri and the UI.
//! Usage: cargo run -p app-runtime --example companion_smoke -- <database> [--cancel]
use app_runtime::Runtime;
use assistant_contracts::{Error, Id};
use serde_json::json;
use std::{process::ExitCode, time::Instant};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "{}",
                json!({"status":"blocked_or_failed","error":error.to_string()})
            );
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Error> {
    let mut args = std::env::args().skip(1);
    let database = args.next().ok_or(Error::InvalidInput)?;
    let options: Vec<_> = args.collect();
    let cancel_after_text = options.iter().any(|option| option == "--cancel");
    let runtime = Runtime::open(database)?;
    if options.iter().any(|option| option == "--install") {
        let status = runtime.dispatch("model_status", json!({})).await?;
        println!(
            "{}",
            json!({"download_bytes":status["manifest"]["size_bytes"],"manifest":status["manifest"]})
        );
        runtime.dispatch("install_model", json!({})).await?;
        let started = Instant::now();
        let mut last_progress = None;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let status = runtime.dispatch("model_status", json!({})).await?;
            let installation = &status["installation"];
            let progress = installation["downloaded_bytes"].as_u64().unwrap_or(0) / 100_000_000;
            if last_progress != Some(progress) {
                println!("{}", json!({"installation":installation}));
                last_progress = Some(progress);
            }
            match installation["status"].as_str() {
                Some("installed") => break,
                Some("failed" | "cancelled") => return Err(Error::Unavailable),
                _ if started.elapsed().as_secs() > 1800 => {
                    runtime.dispatch("cancel_model_download", json!({})).await?;
                    return Err(Error::Timeout);
                }
                _ => {}
            }
        }
    }
    let model = runtime.dispatch("model_status", json!({})).await?;
    if model["availability"] != "ready" {
        println!("{}", json!({"status":"blocked","model":model}));
        return Err(Error::Unavailable);
    }
    let id = Id::new_v4();
    let started = Instant::now();
    runtime.dispatch("send_message", json!({"conversation_id":id,"text":"In one short sentence, explain why a kite needs wind.","temporary":true})).await?;
    let mut first_text_ms = None;
    loop {
        if started.elapsed().as_secs() > 180 {
            runtime
                .dispatch("cancel_message", json!({"conversation_id":id}))
                .await?;
            return Err(Error::Timeout);
        }
        let conversation = runtime
            .dispatch("get_conversation", json!({"conversation_id":id}))
            .await?;
        let message = conversation["messages"]
            .as_array()
            .and_then(|m| m.last())
            .ok_or(Error::InvalidResponse)?;
        if first_text_ms.is_none() && message["content"].as_str().is_some_and(|s| !s.is_empty()) {
            first_text_ms = Some(started.elapsed().as_millis());
            if cancel_after_text {
                runtime
                    .dispatch("cancel_message", json!({"conversation_id":id}))
                    .await?;
            }
        }
        if message["status"] != "generating" {
            println!(
                "{}",
                json!({"mode":"real_local_runtime","message":message,"first_observed_text_ms":first_text_ms,"elapsed_ms":started.elapsed().as_millis(),"poll_interval_ms":100,"cancel_requested":cancel_after_text,"physical_device":false})
            );
            let expected = if cancel_after_text {
                "cancelled"
            } else {
                "complete"
            };
            return if message["status"] == expected {
                Ok(())
            } else {
                Err(Error::InvalidResponse)
            };
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
