use session_common::{CollectorMessage, Snapshot};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use url::Url;

#[derive(Clone)]
pub struct HubClient {
    url: String,
    token: String,
}

impl HubClient {
    pub fn new(url: String, token: String) -> Self {
        Self { url, token }
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
