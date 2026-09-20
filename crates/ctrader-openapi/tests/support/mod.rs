//! A scriptable Open API server for the tests: a local WebSocket that answers like the real one.
//!
//! The server keeps every request it receives, counts heartbeats and connections, answers each
//! request as the test's handler says, and can push events, close the current connection, or go
//! away altogether. It accepts one connection after another, so reconnection can be tested.

#![allow(dead_code)]

pub mod http;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ctrader_openapi::wire::{Envelope, payload};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

/// What the server does for one request.
pub enum Reply {
    /// Answer at once with this payload type and payload (the request's id is added).
    Answer(u32, Value),
    /// Answer after a delay.
    AnswerAfter(Duration, u32, Value),
    /// Send an event with no request id.
    Event(u32, Value),
    /// Say nothing.
    Silence,
    /// Close the socket.
    CloseSocket,
}

/// Decides the replies to a request.
pub type Handler = Arc<dyn Fn(&Envelope) -> Vec<Reply> + Send + Sync>;

enum Command {
    Send(Envelope),
    Close,
}

type CommandSender = mpsc::UnboundedSender<Command>;

/// A running mock server.
pub struct MockServer {
    /// Where to connect.
    pub url: String,
    received: Arc<Mutex<Vec<Envelope>>>,
    heartbeats: Arc<AtomicUsize>,
    connections: Arc<AtomicUsize>,
    current: Arc<Mutex<Option<CommandSender>>>,
    accepting: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl MockServer {
    /// Starts a server that handles requests with `handler`.
    pub async fn start(handler: Handler) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let received = Arc::new(Mutex::new(Vec::new()));
        let heartbeats = Arc::new(AtomicUsize::new(0));
        let connections = Arc::new(AtomicUsize::new(0));
        let current: Arc<Mutex<Option<CommandSender>>> = Arc::new(Mutex::new(None));

        let (seen, beats, count, slot) = (
            received.clone(),
            heartbeats.clone(),
            connections.clone(),
            current.clone(),
        );
        let accepting = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let (commands, command_rx) = mpsc::unbounded_channel::<Command>();
                *slot.lock().unwrap() = Some(commands);
                count.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(serve(
                    stream,
                    handler.clone(),
                    seen.clone(),
                    beats.clone(),
                    command_rx,
                ));
            }
        });

        Self {
            url,
            received,
            heartbeats,
            connections,
            current,
            accepting: Mutex::new(Some(accepting)),
        }
    }

    /// Every request received so far, over all connections, in order (heartbeats excluded).
    pub fn received(&self) -> Vec<Envelope> {
        self.received.lock().unwrap().clone()
    }

    /// The requests of one payload type.
    pub fn received_of(&self, payload_type: u32) -> Vec<Envelope> {
        self.received()
            .into_iter()
            .filter(|e| e.payload_type == payload_type)
            .collect()
    }

    /// How many heartbeats the client has sent.
    pub fn heartbeats(&self) -> usize {
        self.heartbeats.load(Ordering::SeqCst)
    }

    /// How many connections have been accepted.
    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }

    /// Pushes an event to the client on the current connection.
    pub fn push(&self, payload_type: u32, body: Value) {
        if let Some(commands) = self.current.lock().unwrap().as_ref() {
            let _ = commands.send(Command::Send(Envelope {
                client_msg_id: None,
                payload_type,
                payload: body,
            }));
        }
    }

    /// Closes the current connection from the server side. The server keeps accepting.
    pub fn close(&self) {
        if let Some(commands) = self.current.lock().unwrap().as_ref() {
            let _ = commands.send(Command::Close);
        }
    }

    /// Closes the current connection and stops accepting: the server goes away.
    pub fn shutdown(&self) {
        if let Some(task) = self.accepting.lock().unwrap().take() {
            task.abort();
        }
        self.close();
    }
}

/// Serves one connection until it ends.
async fn serve(
    stream: TcpStream,
    handler: Handler,
    seen: Arc<Mutex<Vec<Envelope>>>,
    beats: Arc<AtomicUsize>,
    mut command_rx: mpsc::UnboundedReceiver<Command>,
) {
    let Ok(socket) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    let (mut sink, mut source) = socket.split();
    let (out, mut out_rx) = mpsc::unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            if sink.send(message).await.is_err() {
                break;
            }
        }
    });
    let close = |out: &mpsc::UnboundedSender<Message>| {
        let _ = out.send(Message::Close(None));
    };
    loop {
        tokio::select! {
            frame = source.next() => match frame {
                Some(Ok(Message::Text(text))) => {
                    let Ok(request) = Envelope::from_text(text.as_str()) else { continue };
                    if request.payload_type == payload::HEARTBEAT_EVENT {
                        beats.fetch_add(1, Ordering::SeqCst);
                        continue;
                    }
                    seen.lock().unwrap().push(request.clone());
                    for reply in handler(&request) {
                        match reply {
                            Reply::Answer(kind, body) => {
                                let _ = out.send(text_of(kind, request.client_msg_id.clone(), body));
                            }
                            Reply::AnswerAfter(delay, kind, body) => {
                                let (out, id) = (out.clone(), request.client_msg_id.clone());
                                tokio::spawn(async move {
                                    tokio::time::sleep(delay).await;
                                    let _ = out.send(text_of(kind, id, body));
                                });
                            }
                            Reply::Event(kind, body) => {
                                let _ = out.send(text_of(kind, None, body));
                            }
                            Reply::Silence => {}
                            Reply::CloseSocket => {
                                close(&out);
                                tokio::time::sleep(Duration::from_millis(50)).await;
                                writer.abort();
                                return;
                            }
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
            command = command_rx.recv() => match command {
                Some(Command::Send(envelope)) => {
                    let _ = out.send(Message::text(envelope.to_text().unwrap()));
                }
                Some(Command::Close) | None => {
                    close(&out);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    break;
                }
            },
        }
    }
    writer.abort();
}

fn text_of(payload_type: u32, id: Option<String>, body: Value) -> Message {
    let envelope = Envelope {
        client_msg_id: id,
        payload_type,
        payload: body,
    };
    Message::text(envelope.to_text().unwrap())
}

/// A handler that answers each request type with a fixed payload and ignores the rest.
pub fn answers(table: Vec<(u32, u32, Value)>) -> Handler {
    Arc::new(move |request| {
        table
            .iter()
            .find(|(asked, _, _)| *asked == request.payload_type)
            .map(|(_, kind, body)| vec![Reply::Answer(*kind, body.clone())])
            .unwrap_or_default()
    })
}
