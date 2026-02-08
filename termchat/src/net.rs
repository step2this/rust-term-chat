//! Networking coordinator for wiring the TUI to the async transport layer.
//!
//! This module bridges the synchronous TUI event loop (crossterm poll-based)
//! with the async [`ChatManager`] / [`RelayTransport`] stack. It spawns
//! background tokio tasks and communicates with the main thread via
//! [`NetCommand`] / [`NetEvent`] channels.
//!
//! # Architecture
//!
//! ```text
//! TUI (main thread)  ←── NetEvent ───  tokio background tasks
//!                     ─── NetCommand →
//! ```
//!
//! The main thread sends [`NetCommand`]s (e.g., send a message) and drains
//! [`NetEvent`]s (e.g., message received, status changed) on each tick of
//! the poll-based event loop.

use std::sync::Arc;

use tokio::sync::mpsc;

use termchat_proto::message::{ConversationId, MessageContent, SenderId};

use crate::chat::history::InMemoryStore;
use crate::chat::{ChatEvent, ChatManager};
use crate::crypto::noise::StubNoiseSession;
use crate::transport::PeerId;
use crate::transport::relay::RelayTransport;

/// Commands sent from the TUI main loop to the networking background tasks.
#[derive(Debug)]
pub enum NetCommand {
    /// Send a text message to the remote peer.
    SendMessage {
        /// The message text to send.
        text: String,
    },
    /// Gracefully shut down the networking tasks.
    Shutdown,
}

/// Events sent from the networking background tasks to the TUI main loop.
#[derive(Debug)]
pub enum NetEvent {
    /// A chat message was received from a remote peer.
    MessageReceived {
        /// The sender's peer ID (display name).
        sender: String,
        /// The message content.
        content: String,
        /// Timestamp in milliseconds since epoch.
        timestamp_ms: u64,
    },
    /// A previously sent message's delivery status changed.
    StatusChanged {
        /// Index of the message in the display list (set by the caller).
        message_index: usize,
        /// Whether the message was delivered (ack received).
        delivered: bool,
    },
    /// Connection status update.
    ConnectionStatus {
        /// Whether currently connected to the relay.
        connected: bool,
        /// Human-readable transport description.
        transport_type: String,
    },
    /// An error occurred in the networking layer.
    Error(String),
}

/// Configuration for the networking layer.
#[derive(Debug, Clone)]
pub struct NetConfig {
    /// WebSocket URL of the relay server (e.g., `ws://127.0.0.1:9000/ws`).
    pub relay_url: String,
    /// Local peer identity string.
    pub local_peer_id: String,
    /// Remote peer identity string (who we're chatting with).
    pub remote_peer_id: String,
    /// Channel capacity for command/event mpsc channels.
    pub channel_capacity: usize,
    /// Buffer size for the `ChatManager` event channel.
    pub chat_event_buffer: usize,
}

/// Default channel capacity for commands and events.
const DEFAULT_CHANNEL_CAPACITY: usize = 256;

/// Default channel capacity for `ChatManager` internal events.
const DEFAULT_CHAT_EVENT_BUFFER: usize = 64;

impl NetConfig {
    /// Creates a `NetConfig` with default channel capacities.
    #[must_use]
    pub const fn new(relay_url: String, local_peer_id: String, remote_peer_id: String) -> Self {
        Self {
            relay_url,
            local_peer_id,
            remote_peer_id,
            channel_capacity: DEFAULT_CHANNEL_CAPACITY,
            chat_event_buffer: DEFAULT_CHAT_EVENT_BUFFER,
        }
    }
}

/// Spawn the networking background tasks and return channel handles.
///
/// This connects to the relay server, registers the local peer, creates
/// a [`ChatManager`] with [`StubNoiseSession`] encryption, and spawns:
///
/// 1. A **receive loop** that calls `chat_mgr.receive_one()` and forwards
///    decoded events as [`NetEvent`]s.
/// 2. A **command handler** that listens for [`NetCommand`]s and calls
///    `chat_mgr.send_message()`.
/// 3. A **chat event forwarder** that maps [`ChatEvent`]s to [`NetEvent`]s.
///
/// # Errors
///
/// Returns an error string if relay connection or registration fails.
/// The caller should fall back to offline demo mode on error.
pub async fn spawn_net(
    config: NetConfig,
) -> Result<(mpsc::Sender<NetCommand>, mpsc::Receiver<NetEvent>), String> {
    // Connect to the relay server.
    let transport = RelayTransport::connect(&config.relay_url, PeerId::new(&config.local_peer_id))
        .await
        .map_err(|e| format!("relay connection failed: {e}"))?;

    // Create the ChatManager with stub encryption and in-memory history.
    let crypto = StubNoiseSession::new(true);
    let sender_id = SenderId::new(config.local_peer_id.as_bytes().to_vec());
    let remote_peer = PeerId::new(&config.remote_peer_id);

    let (chat_mgr, chat_event_rx) =
        ChatManager::<StubNoiseSession, RelayTransport, InMemoryStore>::new(
            crypto,
            transport,
            sender_id,
            remote_peer.clone(),
            config.chat_event_buffer,
        );

    let chat_mgr = Arc::new(chat_mgr);

    // Create the command/event channels for TUI communication.
    let (cmd_tx, cmd_rx) = mpsc::channel::<NetCommand>(config.channel_capacity);
    let (evt_tx, evt_rx) = mpsc::channel::<NetEvent>(config.channel_capacity);

    // Send initial connection status.
    let _ = evt_tx
        .send(NetEvent::ConnectionStatus {
            connected: true,
            transport_type: "Relay".to_string(),
        })
        .await;

    // Spawn the receive loop.
    let recv_mgr = Arc::clone(&chat_mgr);
    let recv_evt_tx = evt_tx.clone();
    tokio::spawn(async move {
        receive_loop(recv_mgr, recv_evt_tx).await;
    });

    // Spawn the command handler.
    let cmd_mgr = Arc::clone(&chat_mgr);
    let cmd_evt_tx = evt_tx.clone();
    let conversation = ConversationId::new();
    tokio::spawn(async move {
        command_handler(cmd_mgr, cmd_rx, cmd_evt_tx, conversation).await;
    });

    // Spawn the chat event forwarder.
    tokio::spawn(async move {
        chat_event_forwarder(chat_event_rx, evt_tx).await;
    });

    Ok((cmd_tx, evt_rx))
}

/// Background task: continuously receive messages from the transport.
///
/// Calls `chat_mgr.receive_one()` in a loop. The `ChatManager` handles
/// decryption, deserialization, duplicate detection, and auto-acking.
/// We forward decoded messages as [`NetEvent::MessageReceived`].
async fn receive_loop(
    chat_mgr: Arc<ChatManager<StubNoiseSession, RelayTransport, InMemoryStore>>,
    evt_tx: mpsc::Sender<NetEvent>,
) {
    loop {
        match chat_mgr.receive_one().await {
            Ok(_envelope) => {
                // The ChatManager already emits ChatEvents for received messages
                // and acks. The chat_event_forwarder task handles those.
                // We just need to keep calling receive_one() to drive the loop.
            }
            Err(e) => {
                let err_str = e.to_string();
                tracing::warn!(error = %err_str, "receive_one error");

                // Check if the connection is closed (fatal).
                if err_str.contains("connection closed") {
                    let _ = evt_tx
                        .send(NetEvent::ConnectionStatus {
                            connected: false,
                            transport_type: "Relay".to_string(),
                        })
                        .await;
                    break;
                }

                // Non-fatal errors: log and continue.
                let _ = evt_tx
                    .send(NetEvent::Error(format!("Receive error: {err_str}")))
                    .await;
            }
        }
    }
}

/// Background task: handle commands from the TUI main loop.
///
/// Listens for [`NetCommand`]s and dispatches them to the `ChatManager`.
async fn command_handler(
    chat_mgr: Arc<ChatManager<StubNoiseSession, RelayTransport, InMemoryStore>>,
    mut cmd_rx: mpsc::Receiver<NetCommand>,
    evt_tx: mpsc::Sender<NetEvent>,
    conversation: ConversationId,
) {
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            NetCommand::SendMessage { text } => {
                let content = MessageContent::Text(text);
                match chat_mgr.send_message(content, conversation.clone()).await {
                    Ok((_msg_id, _status)) => {
                        // Status update will come through ChatEvent -> NetEvent pipeline.
                    }
                    Err(e) => {
                        let _ = evt_tx
                            .send(NetEvent::Error(format!("Send failed: {e}")))
                            .await;
                    }
                }
            }
            NetCommand::Shutdown => {
                tracing::info!("net command handler shutting down");
                break;
            }
        }
    }
}

/// Background task: forward `ChatEvent`s as `NetEvent`s to the TUI.
///
/// Maps the internal `ChatEvent` variants to the simpler `NetEvent` enum
/// that the TUI main loop consumes.
async fn chat_event_forwarder(
    mut chat_rx: mpsc::Receiver<ChatEvent>,
    evt_tx: mpsc::Sender<NetEvent>,
) {
    while let Some(event) = chat_rx.recv().await {
        let net_event = match event {
            ChatEvent::MessageReceived { message, from }
            | ChatEvent::MessageReceivedWithClockSkew { message, from, .. } => {
                let termchat_proto::message::MessageContent::Text(ref text) = message.content;
                Some(NetEvent::MessageReceived {
                    sender: from.as_str().to_string(),
                    content: text.clone(),
                    timestamp_ms: message.metadata.timestamp.as_millis(),
                })
            }
            ChatEvent::StatusChanged { status, .. } => {
                let delivered = status == termchat_proto::message::MessageStatus::Delivered;
                Some(NetEvent::StatusChanged {
                    message_index: 0, // Placeholder; TUI resolves by timestamp
                    delivered,
                })
            }
            ChatEvent::PresenceChanged { .. } | ChatEvent::TypingChanged { .. } => {
                // Presence and typing are not wired to the TUI in this UC.
                None
            }
        };

        if let Some(evt) = net_event
            && evt_tx.send(evt).await.is_err()
        {
            // TUI dropped; exit.
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_config_fields_accessible() {
        let config = NetConfig::new(
            "ws://localhost:9000/ws".to_string(),
            "alice".to_string(),
            "bob".to_string(),
        );
        assert_eq!(config.relay_url, "ws://localhost:9000/ws");
        assert_eq!(config.local_peer_id, "alice");
        assert_eq!(config.remote_peer_id, "bob");
        assert_eq!(config.channel_capacity, 256);
        assert_eq!(config.chat_event_buffer, 64);
    }

    #[test]
    fn net_command_debug_format() {
        let cmd = NetCommand::SendMessage {
            text: "hello".to_string(),
        };
        let debug = format!("{cmd:?}");
        assert!(debug.contains("SendMessage"));
    }

    #[test]
    fn net_event_debug_format() {
        let evt = NetEvent::MessageReceived {
            sender: "bob".to_string(),
            content: "hi".to_string(),
            timestamp_ms: 12345,
        };
        let debug = format!("{evt:?}");
        assert!(debug.contains("MessageReceived"));
    }
}
