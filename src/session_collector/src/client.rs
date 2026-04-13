use session_common::{CollectorMessage, Snapshot};
use futures_util::SinkExt;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tokio_tungstenite::tungstenite::Message;
use url::Url;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[derive(Clone)]
pub struct HubClient {
    url: String,
    token: String,
    /// Persistent WebSocket stream to the hub. None means disconnected / needs reconnect.
    stream: Arc<Mutex<Option<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
    /// Exponential backoff state. Reset to INITIAL_BACKOFF on success.
    reconnect_delay_secs: Arc<AtomicU64>,
}

const INITIAL_BACKOFF_SECS: u64 = 1;
const MAX_BACKOFF_SECS: u64 = 60;

impl HubClient {
    pub fn new(url: String, token: String) -> Self {
        Self {
            url,
            token,
            stream: Arc::new(Mutex::new(None)),
            reconnect_delay_secs: Arc::new(AtomicU64::new(INITIAL_BACKOFF_SECS)),
        }
    }

    /// Attempt to connect (or reconnect) to the hub. Idempotent — does nothing if already connected.
    async fn ensure_connected(&self) -> anyhow::Result<()> {
        // Fast path: already connected
        {
            let guard = self.stream.lock().await;
            if guard.is_some() {
                return Ok(());
            }
        }

        // Slow path: connect — release lock before network call to avoid races
        let url_with_token = format!("{}?token={}", self.url, self.token);
        tracing::debug!("Connecting to hub at {}", self.url);

        match connect_async(Url::parse(&url_with_token)?).await {
            Ok((ws_stream, _)) => {
                tracing::info!("Connected to hub");
                self.reconnect_delay_secs
                    .store(INITIAL_BACKOFF_SECS, std::sync::atomic::Ordering::Relaxed);
                let mut guard = self.stream.lock().await;
                *guard = Some(ws_stream);
                Ok(())
            }
            Err(e) => {
                let delay = self
                    .reconnect_delay_secs
                    .load(std::sync::atomic::Ordering::Relaxed);
                tracing::warn!(
                    "Failed to connect to hub (retry in {}s): {}",
                    delay,
                    e
                );
                // Double the backoff, capped at MAX_BACKOFF
                let new_delay = (delay * 2).min(MAX_BACKOFF_SECS);
                self.reconnect_delay_secs
                    .store(new_delay, std::sync::atomic::Ordering::Relaxed);
                Err(e.into())
            }
        }
    }

    /// Send a snapshot to the hub, reconnecting if necessary.
    pub async fn send_snapshot(&self, snapshot: Snapshot) -> anyhow::Result<()> {
        // Ensure we have a live connection
        if let Err(e) = self.ensure_connected().await {
            // If we can't connect, apply backoff and return error so caller knows
            let delay = self
                .reconnect_delay_secs
                .load(std::sync::atomic::Ordering::Relaxed);
            tracing::warn!("Skipping snapshot send — hub unreachable (next retry in {}s)", delay);
            return Err(e);
        }

        let msg = CollectorMessage::Snapshot {
            collector_id: snapshot.collector_id,
            timestamp: snapshot.timestamp,
            fingerprint: snapshot.fingerprint,
            sessions: snapshot.sessions,
        };

        let text = serde_json::to_string(&msg)
            .map_err(|e| anyhow::anyhow!("serialization failed: {}", e))?;

        // Take ownership of the stream, send, then put it back (or None if dead)
        let stream = {
            let mut guard = self.stream.lock().await;
            guard.take() // Take ownership out of the mutex
        };

        if let Some(mut write) = stream {
            match write.send(Message::Text(text)).await {
                Ok(()) => {
                    // Put stream back (still alive)
                    let mut guard = self.stream.lock().await;
                    *guard = Some(write);
                    return Ok(());
                }
                Err(e) => {
                    // Stream is dead — don't put it back; next call will reconnect
                    tracing::warn!("Send failed, will reconnect on next attempt: {}", e);
                    return Err(e.into());
                }
            }
        }

        Err(anyhow::anyhow!("No stream available"))
    }
}
