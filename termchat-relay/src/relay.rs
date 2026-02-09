//! Relay server core: shared state, WebSocket handler, peer registry, and
//! message routing.
//!
//! The relay server accepts WebSocket connections, registers peers by their
//! `PeerId`, and routes encrypted payloads between them. When a recipient is
//! offline, messages are stored in a [`MessageStore`] and delivered when the
//! peer reconnects.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use termchat_proto::relay::{self, RelayMessage};
use termchat_proto::room;
use tokio::sync::{RwLock, mpsc};

use crate::rooms::{self, RoomRegistry};
use crate::store::MessageStore;

/// Default maximum allowed payload size in bytes (64 KB).
const DEFAULT_MAX_PAYLOAD_SIZE: usize = 64 * 1024;

/// Shared relay server state holding the peer registry and message store.
pub struct RelayState {
    /// Maps `PeerId` to a channel sender for delivering WebSocket messages.
    connections: RwLock<HashMap<String, mpsc::UnboundedSender<Message>>>,
    /// Store-and-forward queue for offline peers.
    pub store: MessageStore,
    /// Room directory for room discovery and join request routing.
    pub rooms: RoomRegistry,
    /// Maximum allowed payload size in bytes.
    max_payload_size: usize,
}

impl Default for RelayState {
    fn default() -> Self {
        Self::new()
    }
}

impl RelayState {
    /// Creates a new relay state with empty peer registry and message store,
    /// using default payload and queue size limits.
    #[must_use]
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            store: MessageStore::new(),
            rooms: RoomRegistry::new(),
            max_payload_size: DEFAULT_MAX_PAYLOAD_SIZE,
        }
    }

    /// Creates a new relay state with custom payload size limit and message store.
    #[must_use]
    pub fn with_config(max_payload_size: usize, store: MessageStore) -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            store,
            rooms: RoomRegistry::new(),
            max_payload_size,
        }
    }

    /// Registers a peer, storing the sender half of its message channel.
    ///
    /// If the peer was already registered, the old sender is replaced and
    /// the old channel is effectively closed (the previous WebSocket writer
    /// task will detect the channel closure and shut down).
    pub async fn register(
        &self,
        peer_id: &str,
        sender: mpsc::UnboundedSender<Message>,
    ) -> Option<mpsc::UnboundedSender<Message>> {
        let mut conns = self.connections.write().await;
        conns.insert(peer_id.to_string(), sender)
    }

    /// Removes a peer from the registry, returning the sender if it existed.
    pub async fn unregister(&self, peer_id: &str) -> Option<mpsc::UnboundedSender<Message>> {
        let mut conns = self.connections.write().await;
        conns.remove(peer_id)
    }

    /// Returns a clone of the sender for the given peer, if registered.
    pub async fn get_sender(&self, peer_id: &str) -> Option<mpsc::UnboundedSender<Message>> {
        let conns = self.connections.read().await;
        conns.get(peer_id).cloned()
    }

    /// Send a WebSocket Close frame to all connected peers.
    ///
    /// This causes each peer's writer task to send a close frame, which
    /// triggers the client-side reader to detect disconnection. Useful for
    /// graceful shutdown and testing.
    pub async fn close_all_connections(&self) {
        let conns = self.connections.read().await;
        for (peer_id, sender) in conns.iter() {
            tracing::info!(peer_id = %peer_id, "sending close frame to peer");
            let _ = sender.send(Message::Close(None));
        }
    }
}

/// Handles an upgraded WebSocket connection for a single peer.
///
/// The connection lifecycle:
/// 1. Wait for a `Register` message.
/// 2. Register the peer and send `Registered` back.
/// 3. Drain any queued messages for the peer.
/// 4. Enter the message loop, routing payloads to recipients.
/// 5. On disconnect, unregister the peer.
pub async fn handle_socket(socket: WebSocket, state: Arc<RelayState>) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Wait for the Register message.
    let Some(peer_id) = wait_for_register(&mut ws_receiver).await else {
        tracing::warn!("connection closed before registration");
        return;
    };

    tracing::info!(peer_id = %peer_id, "peer registering");

    // Create a channel for sending messages to this peer's WebSocket writer.
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    // Register the peer (replaces old connection if duplicate, ext 6b).
    if let Some(_old_sender) = state.register(&peer_id, tx).await {
        tracing::info!(peer_id = %peer_id, "replaced existing connection (duplicate register)");
        // Old sender is dropped, closing the old channel.
    }

    // Send Registered acknowledgment.
    let ack = RelayMessage::Registered {
        peer_id: peer_id.clone(),
    };
    if let Err(e) = send_relay_msg(&mut ws_sender, &ack).await {
        tracing::error!(peer_id = %peer_id, error = %e, "failed to send Registered ack");
        state.unregister(&peer_id).await;
        return;
    }

    tracing::info!(peer_id = %peer_id, "peer registered");

    // Drain queued messages for this peer.
    let queued = state.store.drain(&peer_id).await;
    if !queued.is_empty() {
        tracing::info!(
            peer_id = %peer_id,
            count = queued.len(),
            "draining queued messages"
        );
        for stored in queued {
            let msg = RelayMessage::RelayPayload {
                from: stored.from,
                to: peer_id.clone(),
                payload: stored.payload,
            };
            if let Err(e) = send_relay_msg(&mut ws_sender, &msg).await {
                tracing::warn!(
                    peer_id = %peer_id,
                    error = %e,
                    "failed to deliver queued message, stopping drain"
                );
                break;
            }
        }
    }

    // Spawn a writer task that forwards messages from the channel to the WebSocket.
    let writer_peer_id = peer_id.clone();
    let mut write_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                tracing::warn!(peer_id = %writer_peer_id, "WebSocket write failed");
                break;
            }
        }
    });

    // Reader loop: process incoming messages from this peer.
    let reader_peer_id = peer_id.clone();
    let reader_state = Arc::clone(&state);
    let mut read_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            match msg {
                Message::Binary(data) => {
                    handle_binary_message(&reader_peer_id, &data, &reader_state).await;
                }
                Message::Close(_) => {
                    tracing::info!(peer_id = %reader_peer_id, "received close frame");
                    break;
                }
                _ => {
                    // Ignore text, ping, pong frames.
                }
            }
        }
    });

    // Wait for either task to finish, then abort the other.
    tokio::select! {
        _ = &mut read_task => {
            write_task.abort();
        }
        _ = &mut write_task => {
            read_task.abort();
        }
    }

    // Clean up: unregister the peer.
    state.unregister(&peer_id).await;
    tracing::info!(peer_id = %peer_id, "peer disconnected and unregistered");
}

/// Waits for the first message on the WebSocket, expecting a `Register` message.
///
/// Returns the peer ID if a valid `Register` is received, or `None` if the
/// connection closes or an invalid message arrives.
async fn wait_for_register(
    receiver: &mut (impl StreamExt<Item = Result<Message, axum::Error>> + Unpin),
) -> Option<String> {
    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Binary(data) => match relay::decode(&data) {
                Ok(RelayMessage::Register { peer_id }) => {
                    if peer_id.is_empty() {
                        tracing::warn!("received Register with empty peer_id");
                        return None;
                    }
                    return Some(peer_id);
                }
                Ok(other) => {
                    tracing::warn!(msg = ?other, "expected Register, got different message");
                    return None;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to decode registration message");
                    return None;
                }
            },
            Message::Close(_) => return None,
            _ => {
                // Skip non-binary frames (ping/pong) during registration.
            }
        }
    }
    None
}

/// Handles a binary WebSocket message from a registered peer.
async fn handle_binary_message(peer_id: &str, data: &[u8], state: &Arc<RelayState>) {
    let msg = match relay::decode(data) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(peer_id = %peer_id, error = %e, "failed to decode message");
            return;
        }
    };

    match msg {
        RelayMessage::RelayPayload {
            from: _,
            to,
            payload,
        } => {
            // Enforce payload size limit (ext 7b).
            if payload.len() > state.max_payload_size {
                tracing::warn!(
                    peer_id = %peer_id,
                    size = payload.len(),
                    max = state.max_payload_size,
                    "payload exceeds size limit"
                );
                let err = RelayMessage::Error {
                    reason: format!(
                        "payload too large: {} bytes (max {})",
                        payload.len(),
                        state.max_payload_size
                    ),
                };
                send_to_peer(state, peer_id, &err).await;
                return;
            }

            // Server-side `PeerId` enforcement (ext 11a): override `from` with
            // the registered peer_id to prevent spoofing.
            let enforced_from = peer_id.to_string();

            tracing::debug!(
                from = %enforced_from,
                to = %to,
                payload_len = payload.len(),
                "routing payload"
            );

            route_payload(state, &enforced_from, &to, payload).await;
        }
        RelayMessage::Register { peer_id: new_id } => {
            tracing::warn!(
                peer_id = %peer_id,
                new_id = %new_id,
                "received duplicate Register from already-registered peer"
            );
        }
        RelayMessage::Room(room_bytes) => {
            handle_room_message(peer_id, &room_bytes, state).await;
        }
        other => {
            tracing::warn!(
                peer_id = %peer_id,
                msg = ?other,
                "unexpected message type from client"
            );
        }
    }
}

/// Handles a room protocol message from a registered peer.
#[allow(clippy::too_many_lines)]
async fn handle_room_message(peer_id: &str, room_bytes: &[u8], state: &Arc<RelayState>) {
    let room_msg = match room::decode(room_bytes) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(peer_id = %peer_id, error = %e, "failed to decode room message");
            return;
        }
    };

    match room_msg {
        room::RoomMessage::RegisterRoom {
            room_id,
            name,
            admin_peer_id,
        } => {
            match state.rooms.register(&room_id, &name, &admin_peer_id).await {
                Ok(()) => {
                    tracing::info!(
                        peer_id = %peer_id,
                        room_id = %room_id,
                        name = %name,
                        "room registered"
                    );
                    // Echo back the RegisterRoom as confirmation.
                    let confirm = room::RoomMessage::RegisterRoom {
                        room_id,
                        name,
                        admin_peer_id,
                    };
                    send_room_to_peer(state, peer_id, &confirm).await;
                }
                Err(e) => {
                    tracing::warn!(
                        peer_id = %peer_id,
                        room_id = %room_id,
                        error = %e,
                        "room registration failed"
                    );
                    let err = RelayMessage::Error {
                        reason: e.to_string(),
                    };
                    send_to_peer(state, peer_id, &err).await;
                }
            }
        }
        room::RoomMessage::UnregisterRoom { room_id } => {
            let existed = state.rooms.unregister(&room_id).await;
            tracing::info!(
                peer_id = %peer_id,
                room_id = %room_id,
                existed = existed,
                "room unregistered"
            );
        }
        room::RoomMessage::ListRooms => {
            let rooms = state.rooms.list().await;
            let response = room::RoomMessage::RoomList { rooms };
            send_room_to_peer(state, peer_id, &response).await;
        }
        room::RoomMessage::JoinRequest {
            room_id,
            peer_id: requester_id,
            display_name,
        } => {
            if let Err(e) = rooms::route_join_request(
                &state.rooms,
                state,
                &room_id,
                &requester_id,
                &display_name,
            )
            .await
            {
                tracing::warn!(
                    peer_id = %peer_id,
                    room_id = %room_id,
                    error = %e,
                    "join request routing failed"
                );
                let err = RelayMessage::Error {
                    reason: e.to_string(),
                };
                send_to_peer(state, peer_id, &err).await;
            }
        }
        room::RoomMessage::JoinApproved {
            ref room_id,
            ref target_peer_id,
            ..
        } => {
            tracing::info!(
                peer_id = %peer_id,
                room_id = %room_id,
                target = %target_peer_id,
                "routing JoinApproved to target peer"
            );
            let target = target_peer_id.clone();
            route_room_to_peer(state, peer_id, &target, &room_msg).await;
        }
        room::RoomMessage::JoinDenied {
            ref room_id,
            ref target_peer_id,
            ..
        } => {
            tracing::info!(
                peer_id = %peer_id,
                room_id = %room_id,
                target = %target_peer_id,
                "routing JoinDenied to target peer"
            );
            let target = target_peer_id.clone();
            route_room_to_peer(state, peer_id, &target, &room_msg).await;
        }
        room::RoomMessage::MembershipUpdate { .. } | room::RoomMessage::RoomList { .. } => {
            // Client-to-client or server-to-client only; no relay action needed.
        }
    }
}

/// Routes a room message to a target peer, queuing it if the peer is offline.
///
/// On encoding failure, sends a `RelayMessage::Error` back to the sender.
async fn route_room_to_peer(
    state: &Arc<RelayState>,
    sender_peer_id: &str,
    target_peer_id: &str,
    room_msg: &room::RoomMessage,
) {
    let room_bytes = match room::encode(room_msg) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "failed to encode room message for routing");
            let err = RelayMessage::Error {
                reason: format!("room message encoding failed: {e}"),
            };
            send_to_peer(state, sender_peer_id, &err).await;
            return;
        }
    };
    let relay_msg = RelayMessage::Room(room_bytes.clone());

    if let Some(sender) = state.get_sender(target_peer_id).await {
        if let Ok(bytes) = relay::encode(&relay_msg)
            && sender.send(Message::Binary(bytes.into())).is_err()
        {
            // Send failed, queue for later delivery.
            tracing::warn!(
                target = %target_peer_id,
                "forward room message failed, queuing"
            );
            state.unregister(target_peer_id).await;
            state
                .store
                .enqueue(target_peer_id, sender_peer_id, room_bytes)
                .await;
        }
    } else {
        // Target peer is offline — queue the encoded room message bytes.
        state
            .store
            .enqueue(target_peer_id, sender_peer_id, room_bytes)
            .await;
        tracing::info!(
            target = %target_peer_id,
            "target peer offline, room message queued"
        );
    }
}

/// Encodes a room message and sends it to a peer as a `RelayMessage::Room`.
async fn send_room_to_peer(state: &Arc<RelayState>, peer_id: &str, room_msg: &room::RoomMessage) {
    if let Ok(room_bytes) = room::encode(room_msg) {
        let relay_msg = RelayMessage::Room(room_bytes);
        send_to_peer(state, peer_id, &relay_msg).await;
    }
}

/// Routes an encrypted payload to a recipient, queuing it if the recipient is
/// offline.
async fn route_payload(state: &Arc<RelayState>, from: &str, to: &str, payload: Vec<u8>) {
    if let Some(sender) = state.get_sender(to).await {
        // Recipient is connected — forward the payload.
        let msg = RelayMessage::RelayPayload {
            from: from.to_string(),
            to: to.to_string(),
            payload: payload.clone(),
        };
        match relay::encode(&msg) {
            Ok(bytes) => {
                if sender.send(Message::Binary(bytes.into())).is_err() {
                    // Forwarding failed (ext 9a): re-queue and unregister.
                    tracing::warn!(to = %to, "forward failed, re-queuing and unregistering recipient");
                    state.unregister(to).await;
                    state.store.enqueue(to, from, payload).await;
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to encode relay payload for forwarding");
            }
        }
    } else {
        // Recipient not connected (ext 8a/8b): queue the message.
        let count = state.store.enqueue(to, from, payload).await;
        tracing::info!(to = %to, count = count, "recipient offline, message queued");

        // Send Queued acknowledgment back to sender.
        let ack = RelayMessage::Queued {
            to: to.to_string(),
            count,
        };
        send_to_peer(state, from, &ack).await;
    }
}

/// Sends a relay message to a registered peer via its channel.
async fn send_to_peer(state: &Arc<RelayState>, peer_id: &str, msg: &RelayMessage) {
    if let Some(sender) = state.get_sender(peer_id).await
        && let Ok(bytes) = relay::encode(msg)
    {
        let _ = sender.send(Message::Binary(bytes.into()));
    }
}

/// Encodes and sends a relay message directly on a WebSocket sender.
async fn send_relay_msg(
    ws_sender: &mut (impl SinkExt<Message, Error = axum::Error> + Unpin),
    msg: &RelayMessage,
) -> Result<(), String> {
    let bytes = relay::encode(msg)?;
    ws_sender
        .send(Message::Binary(bytes.into()))
        .await
        .map_err(|e| format!("WebSocket send error: {e}"))
}

/// Starts the relay server on the given address and returns the bound address
/// and a join handle.
///
/// This is the primary entry point used by both `main.rs` and test code.
///
/// # Errors
///
/// Returns an error if the TCP listener cannot bind to the given address.
pub async fn start_server(
    addr: &str,
) -> Result<
    (std::net::SocketAddr, tokio::task::JoinHandle<()>),
    Box<dyn std::error::Error + Send + Sync>,
> {
    start_server_with_state(addr, Arc::new(RelayState::new())).await
}

/// Starts the relay server with a pre-configured [`RelayState`].
///
/// Use [`RelayState::with_config`] to create a state with custom payload
/// and queue size limits from the resolved [`crate::config::RelayConfig`].
///
/// # Errors
///
/// Returns an error if the TCP listener cannot bind to the given address.
pub async fn start_server_with_state(
    addr: &str,
    state: Arc<RelayState>,
) -> Result<
    (std::net::SocketAddr, tokio::task::JoinHandle<()>),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let app = axum::Router::new()
        .route("/ws", axum::routing::get(ws_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound_addr = listener.local_addr()?;

    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!(error = %e, "relay server error");
        }
    });

    Ok((bound_addr, handle))
}

/// Starts the relay server in-process for testing.
///
/// Binds to `127.0.0.1:0` (OS-assigned port) and returns the bound address
/// and a [`tokio::task::JoinHandle`] for cleanup.
#[cfg(test)]
pub async fn start_test_server() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    start_server("127.0.0.1:0")
        .await
        .expect("failed to start test server")
}

/// axum handler that upgrades an HTTP request to a WebSocket connection.
async fn ws_handler(
    ws: axum::extract::ws::WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<Arc<RelayState>>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite;

    /// Helper: connect a WebSocket client to the test server and register.
    async fn connect_and_register(
        addr: std::net::SocketAddr,
        peer_id: &str,
    ) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>
    {
        use futures_util::SinkExt;

        let url = format!("ws://{addr}/ws");
        let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

        // Send Register.
        let reg = RelayMessage::Register {
            peer_id: peer_id.to_string(),
        };
        let bytes = relay::encode(&reg).unwrap();
        ws.send(tungstenite::Message::Binary(bytes.into()))
            .await
            .unwrap();

        // Wait for Registered ack.
        let ack_msg = ws.next().await.unwrap().unwrap();
        let ack_data = ack_msg.into_data();
        let ack = relay::decode(&ack_data).unwrap();
        assert_eq!(
            ack,
            RelayMessage::Registered {
                peer_id: peer_id.to_string()
            }
        );

        ws
    }

    /// Helper: send a relay message on a tungstenite WebSocket.
    async fn ws_send(
        ws: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        msg: &RelayMessage,
    ) {
        use futures_util::SinkExt;
        let bytes = relay::encode(msg).unwrap();
        ws.send(tungstenite::Message::Binary(bytes.into()))
            .await
            .unwrap();
    }

    /// Helper: receive a relay message from a tungstenite WebSocket.
    async fn ws_recv(
        ws: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ) -> RelayMessage {
        let msg = ws.next().await.unwrap().unwrap();
        relay::decode(&msg.into_data()).unwrap()
    }

    // --- RelayState unit tests ---

    #[tokio::test]
    async fn register_and_get_sender() {
        let state = RelayState::new();
        let (tx, _rx) = mpsc::unbounded_channel();
        state.register("alice", tx).await;
        assert!(state.get_sender("alice").await.is_some());
    }

    #[tokio::test]
    async fn unregister_removes_peer() {
        let state = RelayState::new();
        let (tx, _rx) = mpsc::unbounded_channel();
        state.register("alice", tx).await;
        state.unregister("alice").await;
        assert!(state.get_sender("alice").await.is_none());
    }

    #[tokio::test]
    async fn duplicate_register_replaces_old() {
        let state = RelayState::new();
        let (tx1, _rx1) = mpsc::unbounded_channel();
        let (tx2, _rx2) = mpsc::unbounded_channel();

        let old = state.register("alice", tx1).await;
        assert!(old.is_none());

        let old = state.register("alice", tx2).await;
        assert!(old.is_some()); // old sender returned
        assert!(state.get_sender("alice").await.is_some());
    }

    #[tokio::test]
    async fn get_sender_unknown_returns_none() {
        let state = RelayState::new();
        assert!(state.get_sender("nobody").await.is_none());
    }

    // --- End-to-end via test server ---

    #[tokio::test]
    async fn two_clients_exchange_messages() {
        let (addr, _handle) = start_test_server().await;

        let mut ws_alice = connect_and_register(addr, "alice").await;
        let mut ws_bob = connect_and_register(addr, "bob").await;

        // Alice sends to Bob.
        let msg = RelayMessage::RelayPayload {
            from: "alice".to_string(),
            to: "bob".to_string(),
            payload: vec![1, 2, 3],
        };
        ws_send(&mut ws_alice, &msg).await;

        // Bob receives it.
        let received = ws_recv(&mut ws_bob).await;
        match received {
            RelayMessage::RelayPayload { from, to, payload } => {
                assert_eq!(from, "alice");
                assert_eq!(to, "bob");
                assert_eq!(payload, vec![1, 2, 3]);
            }
            other => panic!("expected RelayPayload, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn peer_id_enforcement() {
        let (addr, _handle) = start_test_server().await;

        let mut ws_alice = connect_and_register(addr, "alice").await;
        let mut ws_bob = connect_and_register(addr, "bob").await;

        // Alice sends with a spoofed `from` field.
        let msg = RelayMessage::RelayPayload {
            from: "fake-sender".to_string(),
            to: "bob".to_string(),
            payload: vec![42],
        };
        ws_send(&mut ws_alice, &msg).await;

        // Bob should receive with the real sender id ("alice"), not "fake-sender".
        let received = ws_recv(&mut ws_bob).await;
        match received {
            RelayMessage::RelayPayload { from, .. } => {
                assert_eq!(from, "alice", "server must enforce PeerId");
            }
            other => panic!("expected RelayPayload, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn oversized_payload_rejected() {
        let (addr, _handle) = start_test_server().await;

        let mut ws_alice = connect_and_register(addr, "alice").await;

        // Send a payload larger than 64KB.
        let msg = RelayMessage::RelayPayload {
            from: "alice".to_string(),
            to: "bob".to_string(),
            payload: vec![0u8; 65 * 1024],
        };
        ws_send(&mut ws_alice, &msg).await;

        // Alice should receive an error.
        let response = ws_recv(&mut ws_alice).await;
        match response {
            RelayMessage::Error { reason } => {
                assert!(reason.contains("payload too large"), "got: {reason}");
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn queue_drain_on_connect() {
        let (addr, _handle) = start_test_server().await;

        // Alice connects and sends to offline Bob.
        let mut ws_alice = connect_and_register(addr, "alice").await;

        let msg = RelayMessage::RelayPayload {
            from: "alice".to_string(),
            to: "bob".to_string(),
            payload: vec![10, 20, 30],
        };
        ws_send(&mut ws_alice, &msg).await;

        // Alice should get a Queued ack.
        let ack = ws_recv(&mut ws_alice).await;
        match ack {
            RelayMessage::Queued { to, count } => {
                assert_eq!(to, "bob");
                assert_eq!(count, 1);
            }
            other => panic!("expected Queued, got {other:?}"),
        }

        // Now Bob connects — should receive the queued message immediately.
        let mut ws_bob = connect_and_register(addr, "bob").await;

        let received = ws_recv(&mut ws_bob).await;
        match received {
            RelayMessage::RelayPayload { from, payload, .. } => {
                assert_eq!(from, "alice");
                assert_eq!(payload, vec![10, 20, 30]);
            }
            other => panic!("expected RelayPayload, got {other:?}"),
        }
    }

    /// Helper: send a room message wrapped in `RelayMessage::Room`.
    async fn ws_send_room(
        ws: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        room_msg: &room::RoomMessage,
    ) {
        let room_bytes = room::encode(room_msg).unwrap();
        let relay_msg = RelayMessage::Room(room_bytes);
        ws_send(ws, &relay_msg).await;
    }

    /// Helper: receive a relay message and extract the room message inside.
    fn extract_room_msg(relay_msg: &RelayMessage) -> room::RoomMessage {
        match relay_msg {
            RelayMessage::Room(room_bytes) => room::decode(room_bytes).unwrap(),
            other => panic!("expected RelayMessage::Room, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn join_approved_routed_to_target() {
        let (addr, _handle) = start_test_server().await;

        // Alice is the admin, Bob is the joiner.
        let mut ws_alice = connect_and_register(addr, "alice").await;
        let mut ws_bob = connect_and_register(addr, "bob").await;

        // Alice registers a room.
        let register = room::RoomMessage::RegisterRoom {
            room_id: "room-1".to_string(),
            name: "General".to_string(),
            admin_peer_id: "alice".to_string(),
        };
        ws_send_room(&mut ws_alice, &register).await;

        // Alice receives the RegisterRoom echo confirmation.
        let _confirm = ws_recv(&mut ws_alice).await;

        // Bob sends a JoinRequest.
        let join_req = room::RoomMessage::JoinRequest {
            room_id: "room-1".to_string(),
            peer_id: "bob".to_string(),
            display_name: "Bob".to_string(),
        };
        ws_send_room(&mut ws_bob, &join_req).await;

        // Alice receives the JoinRequest.
        let alice_msg = ws_recv(&mut ws_alice).await;
        let room_msg = extract_room_msg(&alice_msg);
        assert_eq!(
            room_msg,
            room::RoomMessage::JoinRequest {
                room_id: "room-1".to_string(),
                peer_id: "bob".to_string(),
                display_name: "Bob".to_string(),
            }
        );

        // Alice sends JoinApproved with target_peer_id="bob".
        let approved = room::RoomMessage::JoinApproved {
            room_id: "room-1".to_string(),
            name: "General".to_string(),
            members: vec![
                room::MemberInfo {
                    peer_id: "alice".to_string(),
                    display_name: "Alice".to_string(),
                    is_admin: true,
                    is_agent: false,
                },
                room::MemberInfo {
                    peer_id: "bob".to_string(),
                    display_name: "Bob".to_string(),
                    is_admin: false,
                    is_agent: false,
                },
            ],
            target_peer_id: "bob".to_string(),
        };
        ws_send_room(&mut ws_alice, &approved).await;

        // Bob should receive the JoinApproved.
        let bob_msg = ws_recv(&mut ws_bob).await;
        let bob_room_msg = extract_room_msg(&bob_msg);
        match bob_room_msg {
            room::RoomMessage::JoinApproved {
                room_id,
                name,
                members,
                target_peer_id,
            } => {
                assert_eq!(room_id, "room-1");
                assert_eq!(name, "General");
                assert_eq!(target_peer_id, "bob");
                assert_eq!(members.len(), 2);
            }
            other => panic!("expected JoinApproved, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn join_denied_routed_to_target() {
        let (addr, _handle) = start_test_server().await;

        // Alice is the admin, Bob is the joiner.
        let mut ws_alice = connect_and_register(addr, "alice").await;
        let mut ws_bob = connect_and_register(addr, "bob").await;

        // Alice registers a room.
        let register = room::RoomMessage::RegisterRoom {
            room_id: "room-2".to_string(),
            name: "Private".to_string(),
            admin_peer_id: "alice".to_string(),
        };
        ws_send_room(&mut ws_alice, &register).await;

        // Alice receives the RegisterRoom echo confirmation.
        let _confirm = ws_recv(&mut ws_alice).await;

        // Bob sends a JoinRequest.
        let join_req = room::RoomMessage::JoinRequest {
            room_id: "room-2".to_string(),
            peer_id: "bob".to_string(),
            display_name: "Bob".to_string(),
        };
        ws_send_room(&mut ws_bob, &join_req).await;

        // Alice receives the JoinRequest.
        let alice_msg = ws_recv(&mut ws_alice).await;
        let room_msg = extract_room_msg(&alice_msg);
        assert_eq!(
            room_msg,
            room::RoomMessage::JoinRequest {
                room_id: "room-2".to_string(),
                peer_id: "bob".to_string(),
                display_name: "Bob".to_string(),
            }
        );

        // Alice sends JoinDenied with target_peer_id="bob".
        let denied = room::RoomMessage::JoinDenied {
            room_id: "room-2".to_string(),
            reason: "invite only".to_string(),
            target_peer_id: "bob".to_string(),
        };
        ws_send_room(&mut ws_alice, &denied).await;

        // Bob should receive the JoinDenied.
        let bob_msg = ws_recv(&mut ws_bob).await;
        let bob_room_msg = extract_room_msg(&bob_msg);
        match bob_room_msg {
            room::RoomMessage::JoinDenied {
                room_id,
                reason,
                target_peer_id,
            } => {
                assert_eq!(room_id, "room-2");
                assert_eq!(reason, "invite only");
                assert_eq!(target_peer_id, "bob");
            }
            other => panic!("expected JoinDenied, got {other:?}"),
        }
    }
}
