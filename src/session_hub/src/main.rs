mod auth;
mod server;
mod state;

use server::HubServer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let auth_token = std::env::var("HUB_AUTH_TOKEN")
        .expect("HUB_AUTH_TOKEN must be set");
    let collector_port: u16 = std::env::var("HUB_PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .expect("HUB_PORT must be a valid port");
    let frontend_port: u16 = std::env::var("HUB_FRONTEND_PORT")
        .unwrap_or_else(|_| "8081".to_string())
        .parse()
        .expect("HUB_FRONTEND_PORT must be a valid port");

    let server = HubServer::new(auth_token, collector_port, frontend_port);
    server.run().await?;

    Ok(())
}
