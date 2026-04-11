mod adapters;
mod client;
mod collector;
mod watcher;

use adapters::{ClaudeAdapter, OpenClawAdapter, CopilotAdapter, CodexAdapter, OpenCodeAdapter, GeminiAdapter};
use client::HubClient;
use collector::Collector;
use session_common::SessionAdapter;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let hub_url = std::env::var("HUB_URL")
        .unwrap_or_else(|_| "ws://localhost:8080".to_string());
    let auth_token = std::env::var("HUB_AUTH_TOKEN")
        .expect("HUB_AUTH_TOKEN must be set");
    let collector_id = std::env::var("COLLECTOR_ID")
        .unwrap_or_else(|_| hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string()));
    let flush_interval_ms: u64 = std::env::var("FLUSH_INTERVAL_MS")
        .unwrap_or_else(|_| "2000".to_string())
        .parse()
        .unwrap_or(2000);

    tracing::info!("Starting session-collector {} -> {}", collector_id, hub_url);

    let adapters: Vec<Box<dyn SessionAdapter + Send + Sync>> = vec![
        Box::new(ClaudeAdapter::new()),
        Box::new(OpenClawAdapter::new()),
        Box::new(CopilotAdapter::new()),
        Box::new(CodexAdapter::new()),
        Box::new(OpenCodeAdapter::new()),
        Box::new(GeminiAdapter::new()),
    ];

    let hub_client = HubClient::new(hub_url, auth_token);
    let mut collector = Collector::new(adapters, hub_client, collector_id);
    
    collector.setup_watchers()?;
    collector.run(flush_interval_ms).await?;

    Ok(())
}