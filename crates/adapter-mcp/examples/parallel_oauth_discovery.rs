use adapter_mcp::OAuthHttpClient;
use reqwest::Url;

#[tokio::main]
async fn main() {
    let endpoint = Url::parse("https://search.parallel.ai/mcp-oauth").unwrap();
    let discovery = OAuthHttpClient::new()
        .expect("OAuth HTTP client construction failed")
        .discover(&endpoint, Some("https://platform.parallel.ai"))
        .await
        .expect("Parallel OAuth discovery failed");
    println!(
        "{}",
        serde_json::to_string_pretty(&discovery).expect("OAuth discovery serialization failed")
    );
}
