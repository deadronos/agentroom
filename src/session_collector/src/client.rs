use session_common::{CollectorMessage, HubMessage, Snapshot};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use url::Url;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct HubClient {
    url: String,
    token: String,
}

impl HubClient {
    pub fn new(url: String, token: String) -> Self {
        Self { url, token }
    }

    pub async fn connect(&self) -> anyhow::Result<()> {
        loop {
            let url_with_token = format!("{}?token={}", self.url, self.token);
            match connect_async(Url::parse(&url_with_token)?).await {
                Ok((ws_stream, _)) => {
                    let (_write, mut read) = ws_stream.split();
                    tracing::info!("Connected to hub");
                    
                    tokio::spawn(async move {
                        while let Some(msg) = read.next().await {
                            if let Ok(Message::Text(text)) = msg {
                                if let Ok(hub_msg) = serde_json::from_str::<HubMessage>(&text) {
                                    match hub_msg {
                                        HubMessage::Ack { .. } => {}
                                        HubMessage::Error { message } => {
                                            tracing::error!("Hub error: {}", message);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    });
                    
                    return Ok(());
                }
                Err(e) => {
                    let backoff = Duration::from_secs(2);
                    tracing::warn!("Failed to connect to hub, retrying in {:?}: {}", 
                        backoff, e);
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    }

    pub async fn send_snapshot(&self, snapshot: Snapshot) -> anyhow::Result<()> {
        let url_with_token = format!("{}?token={}", self.url, self.token);
        let (ws_stream, _) = connect_async(Url::parse(&url_with_token)?).await?;
        let (mut write, _read) = ws_stream.split();
        
        let msg = CollectorMessage::Snapshot {
            collector_id: snapshot.collector_id,
            timestamp: snapshot.timestamp,
            fingerprint: snapshot.fingerprint,
            sessions: snapshot.sessions,
        };
        
        let text = serde_json::to_string(&msg)?;
        write.send(Message::Text(text)).await?;
        write.close().await?;
        
        Ok(())
    }
}
