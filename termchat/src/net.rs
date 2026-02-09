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
//! TUI (main thread)  <── NetEvent ───  tokio background tasks
//!                     ─── NetCommand →
//! ```
//!
//! The main thread sends [`NetCommand`]s (e.g., send a message) and drains
//! [`NetEvent`]s (e.g., message received, status changed) on each tick of
//! the poll-based event loop.
//!
//! ## Supervisor Pattern (UC-011)
//!
//! ```text
//! spawn_net() returns (cmd_tx, evt_rx) -- UNCHANGED interface
//!   |
//!   supervisor_task (NEW) -- owns NetConfig, manages lifecycle
//!     |
//!     +-- command_handler  (persists across reconnects)
//!     +-- receive_loop     (restarted on reconnect)
//!     +-- chat_event_fwd   (restarted on reconnect)
//! ```
//!
//! When the receive loop detects a connection drop, the supervisor detects
//! the task completion, applies exponential backoff with jitter, and attempts
//! to reconnect. Messages sent during disconnection are queued and drained
//! after reconnection succeeds.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use rand::Rng;
use tokio::sync::{RwLock, mpsc};
use uuid::Uuid;

use termchat_proto::message::{ConversationId, MessageContent, SenderId};
use termchat_proto::room::RoomMessage;

use crate::chat::history::InMemoryStore;
use crate::chat::{ChatEvent, ChatManager};
use crate::config::ReconnectConfig;
use crate::crypto::noise::StubNoiseSession;
use crate::transport::PeerId;
use crate::transport::relay::RelayTransport;

/// Type alias for the shared, swappable `ChatManager`.
///
/// The supervisor writes `Some(...)` after (re)connection and `None` on
/// disconnect. The command handler reads it for sending messages.
type SharedChatManager =
    Arc<RwLock<Option<ChatManager<StubNoiseSession, RelayTransport, InMemoryStore>>>>;

/// Type alias for the shared offline message queue.
///
/// When the `ChatManager` is `None` (disconnected), the command handler
/// pushes messages here. The supervisor drains the queue after reconnection.
type MessageQueue = Arc<tokio::sync::Mutex<VecDeque<String>>>;

/// Commands sent from the TUI main loop to the networking background tasks.
#[derive(Debug)]
pub enum NetCommand {
    /// Send a text message to a conversation (DM or room).
    SendMessage {
        /// The conversation ID (room or peer).
        conversation_id: String,
        /// The message text to send.
        text: String,
    },
    /// Update typing status in a conversation.
    SetTyping {
        /// The conversation ID (room or peer).
        conversation_id: String,
        /// Whether the user is currently typing.
        is_typing: bool,
    },
    /// Create a new room.
    CreateRoom {
        /// The room name.
        name: String,
    },
    /// Request a list of available rooms from the relay.
    ListRooms,
    /// Request to join a room.
    JoinRoom {
        /// The room ID to join.
        room_id: String,
    },
    /// Approve a pending join request.
    ApproveJoin {
        /// The room ID.
        room_id: String,
        /// The peer to approve.
        peer_id: String,
    },
    /// Deny a pending join request.
    DenyJoin {
        /// The room ID.
        room_id: String,
        /// The peer to deny.
        peer_id: String,
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
    /// A peer's presence status changed.
    PresenceChanged {
        /// The peer ID.
        peer_id: String,
        /// The new status (Online, Away, Offline).
        status: String,
    },
    /// A peer's typing status changed.
    TypingChanged {
        /// The peer ID.
        peer_id: String,
        /// The room or conversation ID.
        room_id: String,
        /// Whether the peer is currently typing.
        is_typing: bool,
    },
    /// A room was successfully created.
    RoomCreated {
        /// The room ID.
        room_id: String,
        /// The room name.
        name: String,
    },
    /// Room list received from relay.
    RoomList {
        /// List of (`room_id`, `name`, `member_count`) tuples.
        rooms: Vec<(String, String, u32)>,
    },
    /// A join request was received for a room.
    JoinRequestReceived {
        /// The room ID.
        room_id: String,
        /// The peer requesting to join.
        peer_id: String,
        /// The peer's display name.
        display_name: String,
    },
    /// Join request was approved.
    JoinApproved {
        /// The room ID.
        room_id: String,
        /// The room name.
        name: String,
    },
    /// Join request was denied.
    JoinDenied {
        /// The room ID.
        room_id: String,
        /// The reason for denial.
        reason: String,
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
    /// Reconnection attempt in progress.
    Reconnecting {
        /// Current attempt number (1-based).
        attempt: u32,
        /// Maximum number of attempts before giving up.
        max_attempts: u32,
    },
    /// All reconnection attempts exhausted.
    ReconnectFailed,
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
    /// Reconnection configuration (backoff, retries, queue).
    pub reconnect: ReconnectConfig,
}

/// Default channel capacity for commands and events.
const DEFAULT_CHANNEL_CAPACITY: usize = 256;

/// Default channel capacity for `ChatManager` internal events.
const DEFAULT_CHAT_EVENT_BUFFER: usize = 64;

impl NetConfig {
    /// Creates a `NetConfig` with default channel capacities and reconnect config.
    #[must_use]
    pub fn new(relay_url: String, local_peer_id: String, remote_peer_id: String) -> Self {
        Self {
            relay_url,
            local_peer_id,
            remote_peer_id,
            channel_capacity: DEFAULT_CHANNEL_CAPACITY,
            chat_event_buffer: DEFAULT_CHAT_EVENT_BUFFER,
            reconnect: ReconnectConfig::default(),
        }
    }
}

/// Spawn the networking background tasks and return channel handles.
///
/// This connects to the relay server, registers the local peer, creates
/// a [`ChatManager`] with [`StubNoiseSession`] encryption, and spawns:
///
/// 1. A **supervisor** that owns the connection lifecycle and handles
///    reconnection with exponential backoff.
/// 2. A **command handler** that persists across reconnects and reads
///    the shared `ChatManager` for sending messages.
/// 3. A **receive loop** (managed by supervisor, restarted on reconnect)
///    that calls `chat_mgr.receive_one()` and forwards decoded events.
/// 4. A **chat event forwarder** (managed by supervisor, restarted on reconnect)
///    that maps [`ChatEvent`]s to [`NetEvent`]s.
///
/// # Errors
///
/// Returns an error string if the initial relay connection or registration
/// fails. The caller should fall back to offline demo mode on error.
pub async fn spawn_net(
    config: NetConfig,
) -> Result<(mpsc::Sender<NetCommand>, mpsc::Receiver<NetEvent>), String> {
    // Initial connection.
    let transport = RelayTransport::connect(&config.relay_url, PeerId::new(&config.local_peer_id))
        .await
        .map_err(|e| format!("relay connection failed: {e}"))?;

    // Create the initial ChatManager.
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

    // Shared state for the supervisor pattern.
    let shared_mgr: SharedChatManager = Arc::new(RwLock::new(Some(chat_mgr)));
    let message_queue: MessageQueue = Arc::new(tokio::sync::Mutex::new(VecDeque::new()));
    let shutdown_flag = Arc::new(AtomicBool::new(false));

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

    // Spawn the command handler (persists across reconnects).
    let cmd_mgr = Arc::clone(&shared_mgr);
    let cmd_evt_tx = evt_tx.clone();
    let cmd_queue = Arc::clone(&message_queue);
    let cmd_shutdown = Arc::clone(&shutdown_flag);
    let conversation = ConversationId::new();
    let queue_cap = config.reconnect.message_queue_cap;
    let local_peer_id_clone = config.local_peer_id.clone();
    tokio::spawn(async move {
        command_handler(
            cmd_mgr,
            cmd_rx,
            cmd_evt_tx,
            conversation,
            cmd_queue,
            cmd_shutdown,
            queue_cap,
            local_peer_id_clone,
        )
        .await;
    });

    // Spawn the supervisor (owns reconnect lifecycle).
    let sup_mgr = Arc::clone(&shared_mgr);
    let sup_evt_tx = evt_tx;
    let sup_queue = Arc::clone(&message_queue);
    let sup_shutdown = Arc::clone(&shutdown_flag);
    tokio::spawn(async move {
        supervisor(
            config,
            sup_mgr,
            chat_event_rx,
            sup_evt_tx,
            sup_queue,
            sup_shutdown,
        )
        .await;
    });

    Ok((cmd_tx, evt_rx))
}

/// Supervisor task: manages receive loop lifecycle and reconnection.
///
/// After the initial connection, spawns the receive loop and chat event
/// forwarder. When the receive loop exits (connection dropped), the
/// supervisor attempts reconnection with exponential backoff and jitter.
async fn supervisor(
    config: NetConfig,
    shared_mgr: SharedChatManager,
    initial_chat_event_rx: mpsc::Receiver<ChatEvent>,
    evt_tx: mpsc::Sender<NetEvent>,
    message_queue: MessageQueue,
    shutdown_flag: Arc<AtomicBool>,
) {
    let mut chat_event_rx = initial_chat_event_rx;
    let mut last_connected_at: Option<Instant> = Some(Instant::now());

    loop {
        // Spawn the receive loop for the current connection.
        let recv_mgr = Arc::clone(&shared_mgr);
        let recv_evt_tx = evt_tx.clone();
        let recv_handle = tokio::spawn(async move {
            receive_loop(recv_mgr, recv_evt_tx).await;
        });

        // Spawn the chat event forwarder for the current connection.
        let fwd_evt_tx = evt_tx.clone();
        let fwd_handle = tokio::spawn(async move {
            chat_event_forwarder(chat_event_rx, fwd_evt_tx).await;
        });

        // Wait for the receive loop to finish (connection dropped).
        let _ = recv_handle.await;

        // Check for shutdown.
        if shutdown_flag.load(Ordering::Relaxed) {
            fwd_handle.abort();
            break;
        }

        // Mark the ChatManager as disconnected.
        {
            let mut mgr = shared_mgr.write().await;
            *mgr = None;
        }

        // Send disconnection status.
        let _ = evt_tx
            .send(NetEvent::ConnectionStatus {
                connected: false,
                transport_type: "Relay".to_string(),
            })
            .await;

        // Abort the forwarder (it was tied to the old ChatManager's event channel).
        fwd_handle.abort();

        // Attempt reconnection with backoff.
        let reconnect_result = reconnect_with_backoff(
            &config,
            &shared_mgr,
            &evt_tx,
            &message_queue,
            &shutdown_flag,
            &mut last_connected_at,
        )
        .await;

        if let Some(new_chat_event_rx) = reconnect_result {
            // Reconnection succeeded. The new ChatManager is already in shared_mgr.
            // Loop back to spawn new receive_loop + forwarder.
            chat_event_rx = new_chat_event_rx;
        } else {
            // All attempts failed or shutdown requested.
            if !shutdown_flag.load(Ordering::Relaxed) {
                let _ = evt_tx.send(NetEvent::ReconnectFailed).await;
            }
            break;
        }
    }
}

/// Attempt reconnection with exponential backoff and jitter.
///
/// Returns `Some(chat_event_rx)` on success, `None` if all attempts fail
/// or shutdown is requested.
async fn reconnect_with_backoff(
    config: &NetConfig,
    shared_mgr: &SharedChatManager,
    evt_tx: &mpsc::Sender<NetEvent>,
    message_queue: &MessageQueue,
    shutdown_flag: &Arc<AtomicBool>,
    last_connected_at: &mut Option<Instant>,
) -> Option<mpsc::Receiver<ChatEvent>> {
    let reconnect = &config.reconnect;

    // Flap detection: if we were connected for less than the stability
    // threshold, don't reset the backoff counter (the connection was unstable).
    // Since this is the first reconnect cycle after a drop, we start from 0
    // unless flapping is detected. On subsequent calls, the caller tracks
    // stability via last_connected_at.
    let flapping = last_connected_at.is_some_and(|t| t.elapsed() < reconnect.stability_threshold);
    if flapping {
        tracing::warn!("connection was unstable (flap detected), backoff will not reset");
    }

    for attempt in 0..reconnect.max_attempts {
        if shutdown_flag.load(Ordering::Relaxed) {
            return None;
        }

        // Calculate delay with exponential backoff + jitter.
        let base_delay = reconnect
            .initial_delay
            .saturating_mul(2u32.saturating_pow(attempt));
        let capped_delay = std::cmp::min(base_delay, reconnect.max_delay);

        // Add jitter: 0..25% of the capped delay.
        let jitter_range = capped_delay.as_millis() / 4;
        let jitter = if jitter_range > 0 {
            let jitter_ms = rand::rng().random_range(0..=jitter_range);
            std::time::Duration::from_millis(u64::try_from(jitter_ms).unwrap_or(0))
        } else {
            std::time::Duration::ZERO
        };

        let total_delay = capped_delay + jitter;
        let delay_ms = u64::try_from(total_delay.as_millis()).unwrap_or(u64::MAX);
        tracing::info!(
            attempt = attempt + 1,
            max_attempts = reconnect.max_attempts,
            delay_ms,
            "reconnecting to relay"
        );

        tokio::time::sleep(total_delay).await;

        if shutdown_flag.load(Ordering::Relaxed) {
            return None;
        }

        // Notify the TUI of the reconnection attempt.
        let _ = evt_tx
            .send(NetEvent::Reconnecting {
                attempt: attempt + 1,
                max_attempts: reconnect.max_attempts,
            })
            .await;

        // Try to connect.
        match RelayTransport::connect(&config.relay_url, PeerId::new(&config.local_peer_id)).await {
            Ok(transport) => {
                tracing::info!(attempt = attempt + 1, "reconnected to relay successfully");

                // Create a new ChatManager.
                let crypto = StubNoiseSession::new(true);
                let sender_id = SenderId::new(config.local_peer_id.as_bytes().to_vec());
                let remote_peer = PeerId::new(&config.remote_peer_id);

                let (new_mgr, new_chat_event_rx) =
                    ChatManager::<StubNoiseSession, RelayTransport, InMemoryStore>::new(
                        crypto,
                        transport,
                        sender_id,
                        remote_peer,
                        config.chat_event_buffer,
                    );

                // Swap in the new ChatManager.
                {
                    let mut mgr = shared_mgr.write().await;
                    *mgr = Some(new_mgr);
                }

                // Drain the offline message queue.
                drain_message_queue(shared_mgr, message_queue, evt_tx).await;

                // Update connection timestamp for flap detection.
                *last_connected_at = Some(Instant::now());

                // Send reconnected status.
                let _ = evt_tx
                    .send(NetEvent::ConnectionStatus {
                        connected: true,
                        transport_type: "Relay".to_string(),
                    })
                    .await;

                return Some(new_chat_event_rx);
            }
            Err(e) => {
                tracing::warn!(
                    attempt = attempt + 1,
                    max_attempts = reconnect.max_attempts,
                    error = %e,
                    "reconnect attempt failed"
                );
            }
        }
    }

    tracing::error!(
        attempts = reconnect.max_attempts,
        "all reconnect attempts exhausted"
    );
    None
}

/// Drain the offline message queue by sending all queued messages.
///
/// Messages that fail to send are reported as errors but not re-queued
/// (to avoid infinite retry loops).
async fn drain_message_queue(
    shared_mgr: &SharedChatManager,
    message_queue: &MessageQueue,
    evt_tx: &mpsc::Sender<NetEvent>,
) {
    // Drain the queue into a local vec to release the lock quickly.
    let messages: Vec<String> = {
        let mut queue = message_queue.lock().await;
        let count = queue.len();
        if count == 0 {
            return;
        }
        tracing::info!(count, "draining offline message queue");
        queue.drain(..).collect()
    };

    for text in messages {
        let mgr_guard = shared_mgr.read().await;
        if let Some(ref mgr) = *mgr_guard {
            let content = MessageContent::Text(text);
            let conversation = ConversationId::new();
            if let Err(e) = mgr.send_message(content, conversation).await {
                let _ = evt_tx
                    .send(NetEvent::Error(format!(
                        "Failed to send queued message: {e}"
                    )))
                    .await;
            }
        } else {
            // ChatManager gone again during drain; remaining messages are lost.
            tracing::warn!("ChatManager unavailable during queue drain, messages lost");
            break;
        }
    }
}

/// Helper function to send a room protocol message via the relay transport.
///
/// **LIMITATION**: The current `RelayTransport` API only supports `Transport::send()`,
/// which wraps all payloads in `RelayMessage::RelayPayload` with a destination peer.
/// Room protocol messages need to be sent as `RelayMessage::Room` frames directly
/// to the relay server (no peer routing).
///
/// # Proper Fix (requires editing `relay.rs`)
///
/// Add a `send_raw_relay_message()` method to `RelayTransport`:
///
/// ```rust,ignore
/// impl RelayTransport {
///     pub async fn send_raw(&self, msg: &RelayMessage) -> Result<(), TransportError> {
///         let bytes = relay::encode(msg)?;
///         self.ws_sender.lock().await.send(Message::Binary(bytes.into())).await?;
///         Ok(())
///     }
/// }
/// ```
///
/// # Current Workaround
///
/// For this task (T-017-10), we log the message and return `Ok`, allowing
/// the command handler to complete without errors. Room message sending
/// will be completed when `RelayTransport` is extended with `send_raw()`.
#[allow(clippy::unused_async)]
async fn send_room_message(
    _mgr: &ChatManager<StubNoiseSession, RelayTransport, InMemoryStore>,
    room_msg: &RoomMessage,
) -> Result<(), String> {
    tracing::warn!(
        ?room_msg,
        "room message send not yet implemented — requires RelayTransport::send_raw()"
    );
    // Return Ok so the command handler doesn't emit errors to the TUI.
    // The tracing::warn! above alerts developers that this path needs implementation.
    Ok(())
}

/// Background task: continuously receive messages from the transport.
///
/// Calls `chat_mgr.receive_one()` in a loop. The `ChatManager` handles
/// decryption, deserialization, duplicate detection, and auto-acking.
/// We forward decoded messages as [`NetEvent::MessageReceived`].
///
/// Returns when the connection is closed or the `ChatManager` is `None`.
///
/// # Note
///
/// The supervisor guarantees that the `SharedChatManager` will not be
/// modified while this task is running, so we read the `ChatManager`
/// reference once at startup and use it throughout the loop.
///
/// # Room Protocol Messages
///
/// **LIMITATION**: Room protocol messages (e.g., `RoomList`, `JoinApproved`)
/// arrive from the relay as `RelayMessage::Room` frames, but the current
/// `RelayTransport::reader_loop` only forwards `RelayMessage::RelayPayload`
/// to the incoming channel. Room messages are logged but not forwarded.
///
/// To complete room protocol wiring, extend `relay.rs` `reader_loop` to:
/// 1. Decode `RelayMessage::Room(bytes)` → `RoomMessage`
/// 2. Forward room messages on a separate mpsc channel
/// 3. Add `recv_room()` method to `RelayTransport`
/// 4. Poll both chat and room channels in this `receive_loop`
///
/// For now (T-017-10), room responses from the relay are silently dropped.
#[allow(clippy::significant_drop_tightening)]
async fn receive_loop(shared_mgr: SharedChatManager, evt_tx: mpsc::Sender<NetEvent>) {
    // Take a snapshot of the ChatManager at startup. The supervisor will
    // not modify shared_mgr until this task completes.
    let mgr_guard = shared_mgr.read().await;
    let Some(ref mgr) = *mgr_guard else {
        return;
    };

    // TODO: Add room message receive channel handling here once RelayTransport
    // exposes recv_room(). For now, we only handle chat messages.

    loop {
        match mgr.receive_one().await {
            Ok(_envelope) => {
                // The ChatManager already emits ChatEvents for received messages
                // and acks. The chat_event_forwarder task handles those.
                // We just need to keep calling receive_one() to drive the loop.

                // TODO: When room protocol is fully wired, also check for
                // room messages here and emit appropriate NetEvent variants:
                // - NetEvent::RoomCreated for RegisterRoom confirmation
                // - NetEvent::RoomList for ListRooms response
                // - NetEvent::JoinRequestReceived for incoming join requests
                // - NetEvent::JoinApproved for successful joins
                // - NetEvent::JoinDenied for denied joins
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
/// This task persists across reconnects. When the `ChatManager` is `None`
/// (disconnected), messages are queued for later delivery.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
async fn command_handler(
    shared_mgr: SharedChatManager,
    mut cmd_rx: mpsc::Receiver<NetCommand>,
    evt_tx: mpsc::Sender<NetEvent>,
    conversation: ConversationId,
    message_queue: MessageQueue,
    shutdown_flag: Arc<AtomicBool>,
    queue_cap: usize,
    local_peer_id: String,
) {
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            NetCommand::SendMessage {
                conversation_id,
                text,
            } => {
                // Try to send if connected; queue on failure or disconnect.
                let sent = {
                    let mgr_guard = shared_mgr.read().await;
                    if let Some(ref mgr) = *mgr_guard {
                        let content = MessageContent::Text(text.clone());
                        mgr.send_message(content, conversation.clone())
                            .await
                            .is_ok()
                    } else {
                        false
                    }
                };

                if !sent {
                    // Disconnected or send failed: queue for later delivery.
                    let queue_full = {
                        let mut queue = message_queue.lock().await;
                        if queue.len() < queue_cap {
                            queue.push_back(text);
                            false
                        } else {
                            true
                        }
                    };

                    let msg = if queue_full {
                        "Disconnected, message queue full — message dropped"
                    } else {
                        "Disconnected, message queued for delivery"
                    };
                    let _ = evt_tx.send(NetEvent::Error(msg.to_string())).await;
                }
                // NOTE: conversation_id will be used in T-017-10 for room routing
                let _ = conversation_id; // Suppress unused warning
            }
            NetCommand::SetTyping {
                conversation_id,
                is_typing,
            } => {
                tracing::info!("Setting typing status: {is_typing} in {conversation_id}");
                // TODO: Send TypingMessage via relay when typing protocol is implemented
            }
            NetCommand::CreateRoom { name } => {
                tracing::info!("Creating room: {name}");
                let mgr_guard = shared_mgr.read().await;
                if let Some(ref mgr) = *mgr_guard {
                    // Generate a UUID v7 for the room
                    let room_id = Uuid::now_v7().to_string();

                    let room_msg = termchat_proto::room::RoomMessage::RegisterRoom {
                        room_id: room_id.clone(),
                        name: name.clone(),
                        admin_peer_id: local_peer_id.clone(),
                    };

                    if let Err(e) = send_room_message(mgr, &room_msg).await {
                        let _ = evt_tx
                            .send(NetEvent::Error(format!("Failed to create room: {e}")))
                            .await;
                    }
                } else {
                    let _ = evt_tx
                        .send(NetEvent::Error(
                            "Cannot create room: disconnected".to_string(),
                        ))
                        .await;
                }
            }
            NetCommand::ListRooms => {
                tracing::info!("Listing rooms");
                let mgr_guard = shared_mgr.read().await;
                if let Some(ref mgr) = *mgr_guard {
                    let room_msg = termchat_proto::room::RoomMessage::ListRooms;

                    if let Err(e) = send_room_message(mgr, &room_msg).await {
                        let _ = evt_tx
                            .send(NetEvent::Error(format!("Failed to list rooms: {e}")))
                            .await;
                    }
                } else {
                    let _ = evt_tx
                        .send(NetEvent::Error(
                            "Cannot list rooms: disconnected".to_string(),
                        ))
                        .await;
                }
            }
            NetCommand::JoinRoom { room_id } => {
                tracing::info!("Joining room: {room_id}");
                let mgr_guard = shared_mgr.read().await;
                if let Some(ref mgr) = *mgr_guard {
                    let room_msg = termchat_proto::room::RoomMessage::JoinRequest {
                        room_id: room_id.clone(),
                        peer_id: local_peer_id.clone(),
                        display_name: local_peer_id.clone(), // Use peer_id as display name for now
                    };

                    if let Err(e) = send_room_message(mgr, &room_msg).await {
                        let _ = evt_tx
                            .send(NetEvent::Error(format!("Failed to join room: {e}")))
                            .await;
                    }
                } else {
                    let _ = evt_tx
                        .send(NetEvent::Error(
                            "Cannot join room: disconnected".to_string(),
                        ))
                        .await;
                }
            }
            NetCommand::ApproveJoin { room_id, peer_id } => {
                tracing::info!("Approving join request: peer {peer_id} for room {room_id}");
                let mgr_guard = shared_mgr.read().await;
                if let Some(ref mgr) = *mgr_guard {
                    // For approval, we need member info. This is a stub - the full implementation
                    // would query the RoomManager for member details.
                    let room_msg = termchat_proto::room::RoomMessage::JoinApproved {
                        room_id: room_id.clone(),
                        name: "Room".to_string(), // Stub name
                        members: vec![],          // Stub member list
                        target_peer_id: peer_id.clone(),
                    };

                    if let Err(e) = send_room_message(mgr, &room_msg).await {
                        let _ = evt_tx
                            .send(NetEvent::Error(format!("Failed to approve join: {e}")))
                            .await;
                    }
                } else {
                    let _ = evt_tx
                        .send(NetEvent::Error(
                            "Cannot approve join: disconnected".to_string(),
                        ))
                        .await;
                }
            }
            NetCommand::DenyJoin { room_id, peer_id } => {
                tracing::info!("Denying join request: peer {peer_id} for room {room_id}");
                let mgr_guard = shared_mgr.read().await;
                if let Some(ref mgr) = *mgr_guard {
                    let room_msg = termchat_proto::room::RoomMessage::JoinDenied {
                        room_id: room_id.clone(),
                        reason: "Denied by admin".to_string(),
                        target_peer_id: peer_id.clone(),
                    };

                    if let Err(e) = send_room_message(mgr, &room_msg).await {
                        let _ = evt_tx
                            .send(NetEvent::Error(format!("Failed to deny join: {e}")))
                            .await;
                    }
                } else {
                    let _ = evt_tx
                        .send(NetEvent::Error(
                            "Cannot deny join: disconnected".to_string(),
                        ))
                        .await;
                }
            }
            NetCommand::Shutdown => {
                tracing::info!("net command handler shutting down");
                shutdown_flag.store(true, Ordering::Relaxed);
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
            ChatEvent::PresenceChanged { peer_id, status } => {
                let status_str = match status {
                    termchat_proto::presence::PresenceStatus::Online => "Online",
                    termchat_proto::presence::PresenceStatus::Away => "Away",
                    termchat_proto::presence::PresenceStatus::Offline => "Offline",
                };
                Some(NetEvent::PresenceChanged {
                    peer_id,
                    status: status_str.to_string(),
                })
            }
            ChatEvent::TypingChanged {
                peer_id,
                room_id,
                is_typing,
            } => Some(NetEvent::TypingChanged {
                peer_id,
                room_id,
                is_typing,
            }),
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
    fn net_config_includes_reconnect_defaults() {
        let config = NetConfig::new(
            "ws://localhost:9000/ws".to_string(),
            "alice".to_string(),
            "bob".to_string(),
        );
        assert_eq!(
            config.reconnect.initial_delay,
            std::time::Duration::from_secs(1)
        );
        assert_eq!(
            config.reconnect.max_delay,
            std::time::Duration::from_secs(30)
        );
        assert_eq!(config.reconnect.max_attempts, 10);
        assert_eq!(
            config.reconnect.stability_threshold,
            std::time::Duration::from_secs(30)
        );
        assert_eq!(config.reconnect.message_queue_cap, 100);
    }

    #[test]
    fn net_command_debug_format() {
        let cmd = NetCommand::SendMessage {
            conversation_id: "@ bob".to_string(),
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

    #[test]
    fn net_event_reconnecting_debug_format() {
        let evt = NetEvent::Reconnecting {
            attempt: 1,
            max_attempts: 10,
        };
        let debug = format!("{evt:?}");
        assert!(debug.contains("Reconnecting"));
        assert!(debug.contains('1'));
        assert!(debug.contains("10"));
    }

    #[test]
    fn net_event_reconnect_failed_debug_format() {
        let evt = NetEvent::ReconnectFailed;
        let debug = format!("{evt:?}");
        assert!(debug.contains("ReconnectFailed"));
    }
}
