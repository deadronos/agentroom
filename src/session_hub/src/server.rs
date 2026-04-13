use crate::auth::Auth;
use crate::state::HubState;
use session_common::{CollectorMessage, HubMessage};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use futures_util::{SinkExt, StreamExt};

#[allow(dead_code)]
pub struct HubServer {
    state: HubState,
    auth: Auth,
    collector_port: u16,
    frontend_port: u16,
}

#[allow(dead_code)]
impl HubServer {
    pub fn new(auth_token: String, collector_port: u16, frontend_port: u16) -> Self {
        Self {
            state: HubState::new(),
            auth: Auth::new(auth_token),
            collector_port,
            frontend_port,
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        let collector_addr = format!("0.0.0.0:{}", self.collector_port);
        let collector_listener = TcpListener::bind(&collector_addr).await?;
        tracing::info!("Collector WebSocket server listening on {}", collector_addr);

        let frontend_addr = format!("0.0.0.0:{}", self.frontend_port);
        let frontend_listener = TcpListener::bind(&frontend_addr).await?;
        tracing::info!("Frontend WebSocket server listening on {}", frontend_addr);

        let collector_state = self.state.clone();
        let collector_auth = self.auth.clone();
        tokio::spawn(async move {
            loop {
                match collector_listener.accept().await {
                    Ok((stream, _)) => {
                        let state = collector_state.clone();
                        let auth = collector_auth.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_collector_connection(stream, state, auth).await {
                                tracing::warn!("Collector connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("Failed to accept collector connection: {}", e);
                    }
                }
            }
        });

        let frontend_state = self.state.clone();
        tokio::spawn(async move {
            loop {
                match frontend_listener.accept().await {
                    Ok((stream, _)) => {
                        let state = frontend_state.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_frontend_connection(stream, state).await {
                                tracing::warn!("Frontend connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("Failed to accept frontend connection: {}", e);
                    }
                }
            }
        });

        let cleanup_state = self.state.clone();
        let cleanup_auth = self.auth.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let stale = cleanup_auth.cleanup_stale_collectors(60).await;
                for collector_id in stale {
                    cleanup_state.remove_collector(&collector_id).await;
                    tracing::info!("Removed stale collector: {}", collector_id);
                }
            }
        });

        tokio::signal::ctrl_c().await?;
        Ok(())
    }
}

/// Extract token from the request URI query string (e.g. "/?token=abc123")
fn token_from_request(req: &Request) -> Option<String> {
    let uri = req.uri();
    let query = uri.query()?;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        if let Some(key) = parts.next() {
            if key == "token" {
                if let Some(val) = parts.next() {
                    if !val.is_empty() {
                        return Some(val.to_string());
                    }
                }
            }
        }
    }
    None
}

#[allow(dead_code)]
async fn handle_collector_connection(
    stream: tokio::net::TcpStream,
    state: HubState,
    auth: Auth,
) -> anyhow::Result<()> {
    let auth = auth.clone();

    // Use accept_hdr_async to inspect the request URI before completing the handshake
    let ws_stream = tokio_tungstenite::accept_hdr_async(stream, |req: &Request, response: Response| {
        let token = token_from_request(req);
        match token {
            Some(t) if auth.validate_token(&t) => Ok(response),
            _ => {
                tracing::warn!(
                    "Collector rejected: invalid or missing token from {}",
                    req.uri()
                );
                Err(Response::builder()
                    .status(401)
                    .body(Some("Invalid auth token".to_string()))
                    .unwrap())
            }
        }
    })
    .await?;

    let (mut write, mut read) = ws_stream.split();

    while let Some(msg) = read.next().await {
        let msg = msg?;

        if let Message::Text(text) = msg {
            if let Ok(collector_msg) = serde_json::from_str::<CollectorMessage>(&text) {
                match collector_msg {
                    CollectorMessage::Snapshot {
                        collector_id,
                        timestamp,
                        fingerprint,
                        sessions,
                    } => {
                        let ack_fingerprint = fingerprint.clone();
                        let snapshot = session_common::Snapshot {
                            collector_id: collector_id.clone(),
                            timestamp,
                            fingerprint,
                            sessions,
                        };

                        let diff = state.apply_snapshot(snapshot).await;
                        state.broadcast_state().await;

                        let ack = HubMessage::Ack {
                            fingerprint: ack_fingerprint,
                        };
                        write.send(Message::Text(serde_json::to_string(&ack)?)).await?;

                        tracing::debug!(
                            "Applied snapshot from {}: {} started, {} ended",
                            collector_id,
                            diff.started.len(),
                            diff.ended.len()
                        );
                    }
                }
            }
        } else if let Message::Close(_) = msg {
            break;
        }
    }

    Ok(())
}

#[allow(dead_code)]
async fn handle_frontend_connection(
    stream: tokio::net::TcpStream,
    state: HubState,
) -> anyhow::Result<()> {
    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    let (mut write, mut read) = ws_stream.split();

    // Send initial state sync
    let sessions = state.get_all_sessions().await;
    let sync = HubMessage::StateSync { sessions };
    write.send(Message::Text(serde_json::to_string(&sync)?)).await?;

    // Subscribe to broadcasts from collectors
    let mut rx = state.subscribe_frontend();

    loop {
        tokio::select! {
            // Forward broadcast messages to frontend
            msg = rx.recv() => {
                if let Ok(hub_msg) = msg {
                    write.send(Message::Text(serde_json::to_string(&hub_msg)?)).await?;
                }
            }
            // Handle incoming messages from frontend (e.g., close)
            msg = read.next() => {
                if msg.is_none() {
                    break;
                }
                if let Ok(Message::Close(_)) = msg.unwrap() {
                    break;
                }
            }
        }
    }

    Ok(())
}
