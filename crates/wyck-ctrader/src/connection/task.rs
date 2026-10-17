//! The task that owns the socket: it reads frames, writes what the client queues and keeps the
//! connection alive.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, warn};

use crate::connection::{Outgoing, Shared};
use crate::event::DisconnectReason;
use crate::transport::wire::{Envelope, payload};

/// The connection task: reads the socket, writes what the client queues, sends heartbeats.
pub(super) async fn run<S>(
    socket: tokio_tungstenite::WebSocketStream<S>,
    mut outgoing: mpsc::Receiver<Outgoing>,
    shared: Arc<Shared>,
    heartbeat: Duration,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut sink, mut stream) = socket.split();
    let mut beat = tokio::time::interval(heartbeat);
    beat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // The first tick of an interval is immediate; the connection is fresh, skip it.
    beat.tick().await;
    let mut shutdown = shared.shutdown.subscribe();
    let mut last_heard = tokio::time::Instant::now();

    let reason = loop {
        if *shutdown.borrow() {
            break DisconnectReason::ClosedByClient;
        }
        // Past this moment without a frame from the server, the connection is taken for dead.
        let silent = async {
            if shared.silence_timeout.is_zero() {
                std::future::pending::<()>().await;
            } else {
                tokio::time::sleep_until(last_heard + shared.silence_timeout).await;
            }
        };
        tokio::select! {
            biased;
            () = silent => break DisconnectReason::Failed(format!(
                "nothing heard from the server for {} seconds",
                shared.silence_timeout.as_secs()
            )),
            _ = shutdown.changed() => break DisconnectReason::ClosedByClient,
            message = outgoing.recv() => match message {
                Some(Outgoing::Text(text)) => {
                    let sent = tokio::select! {
                        _ = shutdown.changed() => break DisconnectReason::ClosedByClient,
                        sent = tokio::time::timeout(
                            shared.request_timeout,
                            sink.send(Message::text(text)),
                        ) => sent,
                    };
                    match sent {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => break DisconnectReason::Failed(error.to_string()),
                        Err(_) => break DisconnectReason::Failed("WebSocket write timed out".into()),
                    }
                }
                None => break DisconnectReason::ClosedByClient,
            },
            frame = stream.next() => {
                last_heard = tokio::time::Instant::now();
                match frame {
                Some(Ok(Message::Text(text))) => {
                    if let Some(reason) = handle_text(&shared, text.as_str()) {
                        break reason;
                    }
                }
                Some(Ok(Message::Binary(bytes))) => if let Ok(text) = std::str::from_utf8(&bytes) {
                    if let Some(reason) = handle_text(&shared, text) {
                        break reason;
                    }
                } else { warn!("a binary frame that is not text was ignored") },
                // The library answers pings by itself; pongs and raw frames need nothing.
                Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => {}
                Some(Ok(Message::Close(_))) | None => break DisconnectReason::ClosedByServer,
                Some(Err(error)) => break DisconnectReason::Failed(error.to_string()),
                }
            },
            _ = beat.tick() => {
                let Ok(text) = Envelope::heartbeat().to_text() else { continue };
                let sent = tokio::select! {
                    _ = shutdown.changed() => break DisconnectReason::ClosedByClient,
                    sent = tokio::time::timeout(
                        shared.request_timeout,
                        sink.send(Message::text(text)),
                    ) => sent,
                };
                match sent {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => break DisconnectReason::Failed(error.to_string()),
                    Err(_) => break DisconnectReason::Failed("WebSocket heartbeat timed out".into()),
                }
                // A ping too: an idle connection that is alive answers it, which is what the
                // silence check listens for.
                if sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break DisconnectReason::Failed("WebSocket ping failed".into());
                }
            }
        }
    };
    if matches!(reason, DisconnectReason::ClosedByClient) {
        match tokio::time::timeout(Duration::from_secs(1), sink.send(Message::Close(None))).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => debug!(%error, "the close message could not be sent"),
            Err(_) => debug!("the close message was not sent in time"),
        }
    }
    debug!(?reason, "the Open API connection ended");
    shared.finish(reason);
}

/// Reads one text frame from the server. Returns the reason when the server announced that it is
/// ending the connection, which is more telling than the socket closing after it.
fn handle_text(shared: &Shared, text: &str) -> Option<DisconnectReason> {
    let envelope = match Envelope::from_text(text) {
        Ok(envelope) => envelope,
        Err(error) => {
            warn!(%error, "a message that could not be read was ignored");
            return None;
        }
    };
    let announced = (envelope.payload_type == payload::CLIENT_DISCONNECT_EVENT).then(|| {
        DisconnectReason::ServerAnnounced(
            envelope
                .payload
                .get("reason")
                .and_then(|r| r.as_str())
                .map(str::to_owned),
        )
    });
    shared.route(envelope);
    announced
}
