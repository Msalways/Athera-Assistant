//! JSON-lines evaluation runner against the real Rust core.
use serde_json::{json, Value};
use std::io::{self, BufRead};

#[tokio::main]
async fn main() {
    let mut failed = false;
    for line in io::stdin().lock().lines() {
        let report = match line {
            Ok(line) => match serde_json::from_str::<Value>(&line) {
                Ok(scenario) => assistant_cli::evaluate_fixture_report(scenario).await,
                Err(_) => json!({"result":"fail","error":"invalid_input"}),
            },
            Err(_) => json!({"result":"fail","error":"invalid_input"}),
        };
        failed |= report["result"] != "pass";
        println!("{report}");
    }
    if failed {
        std::process::exit(1);
    }
}
