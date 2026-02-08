//! Agent participant adapter for room-level messaging.
//!
//! The [`AgentParticipant`] bridges the gap between the agent's JSON lines
//! protocol and the `TermChat` room messaging pipeline. It handles message
//! fan-out (agent -> room members), receive forwarding (room -> agent),
//! and lifecycle management (join, heartbeat, disconnect).

use tokio::sync::mpsc;

use super::AgentError;
use super::bridge::AgentConnection;
use super::protocol::BridgeMessage;

/// Maximum message size in bytes (64 KB).
const MAX_MESSAGE_SIZE: usize = 64 * 1024;

/// An outbound message from the agent to be fan-out delivered to room members.
///
/// Sent via the `outbound_tx` channel to the app layer, which handles
/// encryption and transport delivery for each room member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundAgentMessage {
    /// Room the message targets.
    pub room_id: String,
    /// Agent's peer ID (the sender).
    pub sender_peer_id: String,
    /// Agent's display name.
    pub sender_display_name: String,
    /// Text content of the message.
    pub content: String,
}

/// An inbound room event to be forwarded to the connected agent.
///
/// Sent via the `room_event_tx` channel from the app layer when room
/// messages or membership changes occur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomEvent {
    /// A chat message from another room participant.
    Message {
        /// Peer ID of the sender.
        sender_id: String,
        /// Display name of the sender.
        sender_name: String,
        /// Text content of the message.
        content: String,
        /// ISO 8601 timestamp string.
        timestamp: String,
    },
    /// A room membership change.
    MembershipChange {
        /// What changed: `"joined"` or `"left"`.
        action: String,
        /// Peer ID of the affected member.
        peer_id: String,
        /// Display name of the affected member.
        display_name: String,
        /// Whether the affected member is an AI agent.
        is_agent: bool,
    },
}

/// The reason an agent participant event loop terminated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisconnectReason {
    /// Agent sent a graceful Goodbye message.
    Goodbye,
    /// The underlying connection was lost (broken pipe / EOF).
    BrokenPipe,
    /// The room event channel was closed (room deleted or app shutting down).
    RoomClosed,
    /// The heartbeat pong was not received within the timeout.
    HeartbeatTimeout,
}

/// Context returned by [`AgentParticipant::run`] after the event loop terminates.
///
/// Contains all the information the caller needs to perform cleanup:
/// remove the agent from the room, broadcast `MemberLeft`, cancel the
/// heartbeat task, and remove the socket file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupContext {
    /// Why the agent disconnected.
    pub reason: DisconnectReason,
    /// The room the agent was in.
    pub room_id: String,
    /// The agent's peer ID (for `RoomManager::remove_member`).
    pub peer_id: String,
    /// The agent's display name (for `MemberLeft` broadcast).
    pub display_name: String,
}

/// Bridges a single agent connection to a room's messaging pipeline.
///
/// The participant manages:
/// - **Outbound fan-out**: Agent `SendMessage` -> validated -> sent to app layer
///   via `outbound_tx` for encryption + transport delivery to each room member.
/// - **Inbound forwarding**: Room messages/membership changes received via
///   `room_event_rx` -> converted to [`BridgeMessage`] -> written to agent.
/// - **Lifecycle**: Tracks readiness state, handles Goodbye, detects broken pipes.
///
/// # Usage
///
/// ```ignore
/// let (outbound_tx, outbound_rx) = mpsc::channel(64);
/// let (room_event_tx, room_event_rx) = mpsc::channel(64);
///
/// let participant = AgentParticipant::new(
///     conn, "room-abc", "agent:claude", "Claude", outbound_tx, room_event_rx,
/// );
/// let cleanup = participant.run().await;
/// // cleanup.reason tells you why the agent disconnected
/// // Use cleanup.peer_id with RoomManager::remove_member()
/// ```
pub struct AgentParticipant {
    /// The agent's socket connection.
    conn: AgentConnection,
    /// Room this agent has joined.
    room_id: String,
    /// Unique peer ID for the agent (e.g. `"agent:claude"`).
    peer_id: String,
    /// Display name for the agent.
    display_name: String,
    /// Whether the handshake has completed (Welcome sent).
    is_ready: bool,
    /// Channel to send outbound messages to the app layer for fan-out.
    outbound_tx: mpsc::Sender<OutboundAgentMessage>,
    /// Channel to receive room events for forwarding to the agent.
    room_event_rx: mpsc::Receiver<RoomEvent>,
}

impl AgentParticipant {
    /// Creates a new participant in the "not ready" state.
    ///
    /// Call [`mark_ready`](Self::mark_ready) after the Welcome handshake completes
    /// to allow the agent to send messages.
    #[must_use]
    pub fn new(
        conn: AgentConnection,
        room_id: &str,
        peer_id: &str,
        display_name: &str,
        outbound_tx: mpsc::Sender<OutboundAgentMessage>,
        room_event_rx: mpsc::Receiver<RoomEvent>,
    ) -> Self {
        Self {
            conn,
            room_id: room_id.to_string(),
            peer_id: peer_id.to_string(),
            display_name: display_name.to_string(),
            is_ready: false,
            outbound_tx,
            room_event_rx,
        }
    }

    /// Marks the participant as ready (Welcome has been sent).
    ///
    /// After this call, `SendMessage` from the agent will be processed
    /// rather than rejected with a `not_ready` error.
    pub const fn mark_ready(&mut self) {
        self.is_ready = true;
    }

    /// Returns whether the participant is ready to process messages.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        self.is_ready
    }

    /// Returns the agent's peer ID.
    #[must_use]
    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }

    /// Returns the agent's display name.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// Returns the room ID this participant is associated with.
    #[must_use]
    pub fn room_id(&self) -> &str {
        &self.room_id
    }

    /// Runs the main event loop, processing agent messages and room events.
    ///
    /// This method consumes `self` and loops until the agent disconnects,
    /// the room event channel closes, or an unrecoverable error occurs.
    /// The connection is closed before returning.
    ///
    /// Returns a [`CleanupContext`] that the caller uses to:
    /// 1. Remove the agent from the room via `RoomManager::remove_member()`
    /// 2. Broadcast a `MemberLeft` event to remaining room members
    /// 3. Cancel any heartbeat task
    /// 4. Remove the socket file
    pub async fn run(mut self) -> CleanupContext {
        let reason = self.run_event_loop().await;
        self.conn.close().await;

        CleanupContext {
            reason,
            room_id: self.room_id,
            peer_id: self.peer_id,
            display_name: self.display_name,
        }
    }

    /// Internal event loop. Returns the disconnect reason without closing
    /// the connection (that's done by [`run`](Self::run)).
    async fn run_event_loop(&mut self) -> DisconnectReason {
        loop {
            tokio::select! {
                // Branch 1: Read from agent connection
                agent_msg = self.conn.read_message() => {
                    match agent_msg {
                        Ok(msg) => {
                            if let Ok(Some(reason)) = self.handle_agent_message(msg).await {
                                return reason;
                            }
                            // Ok(None) or Err(_): continue loop.
                            // Errors are non-fatal (already sent to agent).
                        }
                        Err(_) => {
                            // Connection closed, I/O error, or JSON error — treat as broken pipe
                            return DisconnectReason::BrokenPipe;
                        }
                    }
                }
                // Branch 2: Receive room events to forward to agent
                room_event = self.room_event_rx.recv() => {
                    match room_event {
                        Some(event) => {
                            if self.forward_room_event(event).await.is_err() {
                                return DisconnectReason::BrokenPipe;
                            }
                        }
                        None => {
                            // Channel closed — room deleted or app shutting down
                            return DisconnectReason::RoomClosed;
                        }
                    }
                }
            }
        }
    }

    /// Handles a single agent message.
    ///
    /// Returns:
    /// - `Ok(Some(reason))` if the loop should terminate
    /// - `Ok(None)` to continue the loop
    /// - `Err(AgentError)` for non-fatal errors (error already sent to agent)
    async fn handle_agent_message(
        &mut self,
        msg: super::protocol::AgentMessage,
    ) -> Result<Option<DisconnectReason>, AgentError> {
        use super::protocol::AgentMessage;

        match msg {
            AgentMessage::SendMessage { content } => {
                self.handle_send_message(content).await?;
                Ok(None)
            }
            AgentMessage::Goodbye => Ok(Some(DisconnectReason::Goodbye)),
            AgentMessage::Pong => {
                // Pong is handled by the heartbeat loop, not here.
                // If we receive it here, just ignore it.
                Ok(None)
            }
            AgentMessage::Hello { .. } => {
                // Hello after connection is established is a protocol error
                let err_msg = BridgeMessage::Error {
                    code: "protocol_error".to_string(),
                    message: "unexpected Hello after handshake".to_string(),
                };
                let _ = self.conn.write_message(&err_msg).await;
                Ok(None)
            }
        }
    }

    /// Handles a `SendMessage` from the agent.
    ///
    /// Validates the message and sends it to the app layer for fan-out.
    /// The app layer performs best-effort per-member delivery: if transport
    /// fails for one member or no Noise session exists, that member is
    /// skipped and delivery continues to remaining members.
    ///
    /// # Errors
    ///
    /// Returns `AgentError` if the message fails validation or the outbound
    /// channel is closed. An error `BridgeMessage` is sent to the agent.
    async fn handle_send_message(&mut self, content: String) -> Result<(), AgentError> {
        // Check readiness
        if !self.is_ready {
            let err_msg = BridgeMessage::Error {
                code: "not_ready".to_string(),
                message: "handshake not complete, cannot send messages yet".to_string(),
            };
            self.conn.write_message(&err_msg).await?;
            return Err(AgentError::ProtocolError("not ready".to_string()));
        }

        // Validate: non-empty
        if content.trim().is_empty() {
            let err_msg = BridgeMessage::Error {
                code: "empty_message".to_string(),
                message: "message content cannot be empty".to_string(),
            };
            self.conn.write_message(&err_msg).await?;
            return Err(AgentError::InvalidMessage("empty message".to_string()));
        }

        // Validate: size limit
        if content.len() > MAX_MESSAGE_SIZE {
            let err_msg = BridgeMessage::Error {
                code: "message_too_large".to_string(),
                message: format!(
                    "message exceeds size limit ({} bytes, max {MAX_MESSAGE_SIZE})",
                    content.len()
                ),
            };
            self.conn.write_message(&err_msg).await?;
            return Err(AgentError::InvalidMessage("message too large".to_string()));
        }

        // Send to app layer for fan-out
        let outbound = OutboundAgentMessage {
            room_id: self.room_id.clone(),
            sender_peer_id: self.peer_id.clone(),
            sender_display_name: self.display_name.clone(),
            content,
        };

        if self.outbound_tx.send(outbound).await.is_err() {
            // App layer dropped the receiver — treat as room closed
            let err_msg = BridgeMessage::Error {
                code: "room_closed".to_string(),
                message: "room is no longer available".to_string(),
            };
            let _ = self.conn.write_message(&err_msg).await;
            return Err(AgentError::RoomNotFound(self.room_id.clone()));
        }

        Ok(())
    }

    /// Forwards a room event to the connected agent as a [`BridgeMessage`].
    ///
    /// # Errors
    ///
    /// Returns `AgentError` if writing to the agent connection fails.
    async fn forward_room_event(&mut self, event: RoomEvent) -> Result<(), AgentError> {
        let bridge_msg = match event {
            RoomEvent::Message {
                sender_id,
                sender_name,
                content,
                timestamp,
            } => BridgeMessage::RoomMessage {
                sender_id,
                sender_name,
                content,
                timestamp,
            },
            RoomEvent::MembershipChange {
                action,
                peer_id,
                display_name,
                is_agent,
            } => BridgeMessage::MembershipUpdate {
                action,
                peer_id,
                display_name,
                is_agent,
            },
        };
        self.conn.write_message(&bridge_msg).await
    }

    /// Forwards a room message to the connected agent.
    ///
    /// Convenience method for forwarding a single chat message. Equivalent
    /// to calling [`forward_room_event`](Self::forward_room_event) with a
    /// [`RoomEvent::Message`].
    ///
    /// # Errors
    ///
    /// Returns `AgentError` if writing to the agent connection fails.
    pub async fn forward_room_message(
        &mut self,
        sender_id: &str,
        sender_name: &str,
        content: &str,
        timestamp: &str,
    ) -> Result<(), AgentError> {
        let msg = BridgeMessage::RoomMessage {
            sender_id: sender_id.to_string(),
            sender_name: sender_name.to_string(),
            content: content.to_string(),
            timestamp: timestamp.to_string(),
        };
        self.conn.write_message(&msg).await
    }

    /// Forwards a membership update to the connected agent.
    ///
    /// Convenience method for forwarding a single membership change.
    ///
    /// # Errors
    ///
    /// Returns `AgentError` if writing to the agent connection fails.
    pub async fn forward_membership_update(
        &mut self,
        action: &str,
        peer_id: &str,
        display_name: &str,
        is_agent: bool,
    ) -> Result<(), AgentError> {
        let msg = BridgeMessage::MembershipUpdate {
            action: action.to_string(),
            peer_id: peer_id.to_string(),
            display_name: display_name.to_string(),
            is_agent,
        };
        self.conn.write_message(&msg).await
    }

    /// Gracefully closes the agent connection.
    pub async fn close(mut self) {
        self.conn.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::bridge::AgentBridge;
    use crate::agent::protocol::{AgentMessage, encode_line};
    use std::path::PathBuf;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
    use tokio::net::UnixStream;

    /// Creates a unique temporary socket path for each test.
    fn temp_socket_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("termchat-test-participant");
        dir.join(format!("{name}-{}.sock", std::process::id()))
    }

    /// Helper: set up a bridge + client pair, returning the AgentConnection
    /// and a handle to the client's reader/writer.
    async fn setup_pair(
        name: &str,
    ) -> (
        AgentConnection,
        BufReader<tokio::net::unix::OwnedReadHalf>,
        BufWriter<tokio::net::unix::OwnedWriteHalf>,
        AgentBridge,
    ) {
        let path = temp_socket_path(name);
        let bridge = AgentBridge::start(&path, "room-1").expect("start");

        let client_path = path.clone();
        let client_handle = tokio::spawn(async move {
            let stream = UnixStream::connect(&client_path).await.expect("connect");
            let (read_half, write_half) = stream.into_split();
            (BufReader::new(read_half), BufWriter::new(write_half))
        });

        let conn = bridge.accept_connection().await.expect("accept");
        let (reader, writer) = client_handle.await.expect("join");
        (conn, reader, writer, bridge)
    }

    /// Helper: read one JSON line from the client reader.
    async fn read_bridge_msg(
        reader: &mut BufReader<tokio::net::unix::OwnedReadHalf>,
    ) -> BridgeMessage {
        let mut line = String::new();
        reader.read_line(&mut line).await.expect("read");
        crate::agent::protocol::decode_line(&line).expect("decode")
    }

    /// Helper: write an AgentMessage as JSON line from the client writer.
    async fn write_agent_msg(
        writer: &mut BufWriter<tokio::net::unix::OwnedWriteHalf>,
        msg: &AgentMessage,
    ) {
        let line = encode_line(msg).expect("encode");
        writer.write_all(line.as_bytes()).await.expect("write");
        writer.flush().await.expect("flush");
    }

    #[tokio::test]
    async fn send_message_when_not_ready_returns_error() {
        let (conn, mut reader, mut writer, _bridge) = setup_pair("not-ready").await;
        let (outbound_tx, _outbound_rx) = mpsc::channel(64);
        let (_room_tx, room_rx) = mpsc::channel(64);

        let mut participant =
            AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);

        // Don't mark ready — send a message from the agent
        let send_msg = AgentMessage::SendMessage {
            content: "hello".to_string(),
        };
        write_agent_msg(&mut writer, &send_msg).await;

        // The participant should send an error back
        let agent_msg = participant.conn.read_message().await.expect("read");
        let result = participant.handle_agent_message(agent_msg).await;
        assert!(result.is_err());

        // Read the error on the client side
        let response = read_bridge_msg(&mut reader).await;
        match response {
            BridgeMessage::Error { code, .. } => assert_eq!(code, "not_ready"),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn send_message_when_ready_fans_out() {
        let (conn, _reader, mut writer, _bridge) = setup_pair("fan-out").await;
        let (outbound_tx, mut outbound_rx) = mpsc::channel(64);
        let (_room_tx, room_rx) = mpsc::channel(64);

        let mut participant =
            AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);
        participant.mark_ready();

        // Agent sends a message
        let send_msg = AgentMessage::SendMessage {
            content: "hello world".to_string(),
        };
        write_agent_msg(&mut writer, &send_msg).await;

        // Read and handle
        let agent_msg = participant.conn.read_message().await.expect("read");
        let result = participant.handle_agent_message(agent_msg).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none()); // no disconnect

        // Check outbound channel
        let outbound = outbound_rx.try_recv().expect("outbound");
        assert_eq!(outbound.room_id, "room-1");
        assert_eq!(outbound.sender_peer_id, "agent:bot");
        assert_eq!(outbound.sender_display_name, "Bot");
        assert_eq!(outbound.content, "hello world");
    }

    #[tokio::test]
    async fn send_empty_message_returns_error() {
        let (conn, mut reader, mut writer, _bridge) = setup_pair("empty-msg").await;
        let (outbound_tx, _outbound_rx) = mpsc::channel(64);
        let (_room_tx, room_rx) = mpsc::channel(64);

        let mut participant =
            AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);
        participant.mark_ready();

        let send_msg = AgentMessage::SendMessage {
            content: "   ".to_string(),
        };
        write_agent_msg(&mut writer, &send_msg).await;

        let agent_msg = participant.conn.read_message().await.expect("read");
        let result = participant.handle_agent_message(agent_msg).await;
        assert!(result.is_err());

        let response = read_bridge_msg(&mut reader).await;
        match response {
            BridgeMessage::Error { code, .. } => assert_eq!(code, "empty_message"),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn send_oversized_message_returns_error() {
        let (conn, mut reader, mut writer, _bridge) = setup_pair("oversize").await;
        let (outbound_tx, _outbound_rx) = mpsc::channel(64);
        let (_room_tx, room_rx) = mpsc::channel(64);

        let mut participant =
            AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);
        participant.mark_ready();

        let big_content = "x".repeat(MAX_MESSAGE_SIZE + 1);
        let send_msg = AgentMessage::SendMessage {
            content: big_content,
        };
        write_agent_msg(&mut writer, &send_msg).await;

        let agent_msg = participant.conn.read_message().await.expect("read");
        let result = participant.handle_agent_message(agent_msg).await;
        assert!(result.is_err());

        let response = read_bridge_msg(&mut reader).await;
        match response {
            BridgeMessage::Error { code, .. } => assert_eq!(code, "message_too_large"),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn goodbye_returns_disconnect_reason() {
        let (conn, _reader, mut writer, _bridge) = setup_pair("goodbye").await;
        let (outbound_tx, _outbound_rx) = mpsc::channel(64);
        let (_room_tx, room_rx) = mpsc::channel(64);

        let mut participant =
            AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);
        participant.mark_ready();

        write_agent_msg(&mut writer, &AgentMessage::Goodbye).await;

        let agent_msg = participant.conn.read_message().await.expect("read");
        let result = participant.handle_agent_message(agent_msg).await;
        assert_eq!(result.unwrap(), Some(DisconnectReason::Goodbye));
    }

    #[tokio::test]
    async fn forward_room_message_writes_to_agent() {
        let (conn, mut reader, _writer, _bridge) = setup_pair("fwd-msg").await;
        let (outbound_tx, _outbound_rx) = mpsc::channel(64);
        let (_room_tx, room_rx) = mpsc::channel(64);

        let mut participant =
            AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);
        participant.mark_ready();

        participant
            .forward_room_message("peer-alice", "Alice", "hey bot!", "2025-01-15T12:00:00Z")
            .await
            .expect("forward");

        let msg = read_bridge_msg(&mut reader).await;
        match msg {
            BridgeMessage::RoomMessage {
                sender_id,
                sender_name,
                content,
                timestamp,
            } => {
                assert_eq!(sender_id, "peer-alice");
                assert_eq!(sender_name, "Alice");
                assert_eq!(content, "hey bot!");
                assert_eq!(timestamp, "2025-01-15T12:00:00Z");
            }
            other => panic!("expected RoomMessage, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn forward_membership_update_writes_to_agent() {
        let (conn, mut reader, _writer, _bridge) = setup_pair("fwd-member").await;
        let (outbound_tx, _outbound_rx) = mpsc::channel(64);
        let (_room_tx, room_rx) = mpsc::channel(64);

        let mut participant =
            AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);

        participant
            .forward_membership_update("joined", "peer-bob", "Bob", false)
            .await
            .expect("forward");

        let msg = read_bridge_msg(&mut reader).await;
        match msg {
            BridgeMessage::MembershipUpdate {
                action,
                peer_id,
                display_name,
                is_agent,
            } => {
                assert_eq!(action, "joined");
                assert_eq!(peer_id, "peer-bob");
                assert_eq!(display_name, "Bob");
                assert!(!is_agent);
            }
            other => panic!("expected MembershipUpdate, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn run_loop_goodbye_terminates() {
        let (conn, _reader, mut writer, _bridge) = setup_pair("run-goodbye").await;
        let (outbound_tx, _outbound_rx) = mpsc::channel(64);
        let (_room_tx, room_rx) = mpsc::channel(64);

        let mut participant =
            AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);
        participant.mark_ready();

        // Send Goodbye from client
        write_agent_msg(&mut writer, &AgentMessage::Goodbye).await;

        let cleanup = participant.run().await;
        assert_eq!(cleanup.reason, DisconnectReason::Goodbye);
        assert_eq!(cleanup.room_id, "room-1");
        assert_eq!(cleanup.peer_id, "agent:bot");
        assert_eq!(cleanup.display_name, "Bot");
    }

    #[tokio::test]
    async fn run_loop_broken_pipe_terminates() {
        let (conn, _reader, writer, _bridge) = setup_pair("run-broken").await;
        let (outbound_tx, _outbound_rx) = mpsc::channel(64);
        let (_room_tx, room_rx) = mpsc::channel(64);

        let participant =
            AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);

        // Drop the client writer to cause EOF
        drop(writer);

        let cleanup = participant.run().await;
        assert_eq!(cleanup.reason, DisconnectReason::BrokenPipe);
    }

    #[tokio::test]
    async fn run_loop_room_closed_terminates() {
        let (conn, _reader, _writer, _bridge) = setup_pair("run-room-close").await;
        let (outbound_tx, _outbound_rx) = mpsc::channel(64);
        let (room_tx, room_rx) = mpsc::channel(64);

        let participant =
            AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);

        // Drop the room event sender to close the channel
        drop(room_tx);

        let cleanup = participant.run().await;
        assert_eq!(cleanup.reason, DisconnectReason::RoomClosed);
    }

    #[tokio::test]
    async fn run_loop_forwards_room_events() {
        let (conn, mut reader, _writer, _bridge) = setup_pair("run-fwd").await;
        let (outbound_tx, _outbound_rx) = mpsc::channel(64);
        let (room_tx, room_rx) = mpsc::channel(64);

        let mut participant =
            AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);
        participant.mark_ready();

        // Send a room event, then close the channel to terminate the loop
        room_tx
            .send(RoomEvent::Message {
                sender_id: "peer-alice".to_string(),
                sender_name: "Alice".to_string(),
                content: "hello agent".to_string(),
                timestamp: "2025-01-15T12:00:00Z".to_string(),
            })
            .await
            .expect("send");
        drop(room_tx);

        let cleanup = participant.run().await;

        // Should have forwarded the message before terminating
        let msg = read_bridge_msg(&mut reader).await;
        assert!(matches!(msg, BridgeMessage::RoomMessage { .. }));

        assert_eq!(cleanup.reason, DisconnectReason::RoomClosed);
    }

    #[tokio::test]
    async fn accessors_return_correct_values() {
        let (conn, _reader, _writer, _bridge) = setup_pair("accessors").await;
        let (outbound_tx, _outbound_rx) = mpsc::channel(64);
        let (_room_tx, room_rx) = mpsc::channel(64);

        let participant = AgentParticipant::new(
            conn,
            "room-abc",
            "agent:claude",
            "Claude",
            outbound_tx,
            room_rx,
        );

        assert_eq!(participant.peer_id(), "agent:claude");
        assert_eq!(participant.display_name(), "Claude");
        assert_eq!(participant.room_id(), "room-abc");
        assert!(!participant.is_ready());
    }

    #[tokio::test]
    async fn mark_ready_changes_state() {
        let (conn, _reader, _writer, _bridge) = setup_pair("mark-ready").await;
        let (outbound_tx, _outbound_rx) = mpsc::channel(64);
        let (_room_tx, room_rx) = mpsc::channel(64);

        let mut participant =
            AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);

        assert!(!participant.is_ready());
        participant.mark_ready();
        assert!(participant.is_ready());
    }

    #[tokio::test]
    async fn unexpected_hello_after_handshake_sends_error() {
        let (conn, mut reader, mut writer, _bridge) = setup_pair("extra-hello").await;
        let (outbound_tx, _outbound_rx) = mpsc::channel(64);
        let (_room_tx, room_rx) = mpsc::channel(64);

        let mut participant =
            AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);
        participant.mark_ready();

        // Send a Hello (which is wrong after handshake)
        let hello = AgentMessage::Hello {
            protocol_version: 1,
            agent_id: "bot".to_string(),
            display_name: "Bot".to_string(),
            capabilities: vec![],
        };
        write_agent_msg(&mut writer, &hello).await;

        let agent_msg = participant.conn.read_message().await.expect("read");
        let result = participant.handle_agent_message(agent_msg).await;
        // Should not terminate the loop, just send error
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());

        let response = read_bridge_msg(&mut reader).await;
        match response {
            BridgeMessage::Error { code, .. } => assert_eq!(code, "protocol_error"),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn send_message_outbound_channel_closed_returns_room_closed() {
        let (conn, mut reader, mut writer, _bridge) = setup_pair("outbound-closed").await;
        let (outbound_tx, outbound_rx) = mpsc::channel(64);
        let (_room_tx, room_rx) = mpsc::channel(64);

        let mut participant =
            AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);
        participant.mark_ready();

        // Drop the outbound receiver to simulate room/app shutdown
        drop(outbound_rx);

        let send_msg = AgentMessage::SendMessage {
            content: "hello".to_string(),
        };
        write_agent_msg(&mut writer, &send_msg).await;

        let agent_msg = participant.conn.read_message().await.expect("read");
        let result = participant.handle_agent_message(agent_msg).await;
        assert!(result.is_err());

        let response = read_bridge_msg(&mut reader).await;
        match response {
            BridgeMessage::Error { code, .. } => assert_eq!(code, "room_closed"),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn multiple_sends_succeed_independently() {
        let (conn, _reader, mut writer, _bridge) = setup_pair("multi-send").await;
        let (outbound_tx, mut outbound_rx) = mpsc::channel(64);
        let (_room_tx, room_rx) = mpsc::channel(64);

        let mut participant =
            AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);
        participant.mark_ready();

        // Send multiple messages
        for i in 0..3 {
            let send_msg = AgentMessage::SendMessage {
                content: format!("message {i}"),
            };
            write_agent_msg(&mut writer, &send_msg).await;

            let agent_msg = participant.conn.read_message().await.expect("read");
            let result = participant.handle_agent_message(agent_msg).await;
            assert!(result.is_ok());
        }

        // All three should arrive independently
        for i in 0..3 {
            let outbound = outbound_rx.try_recv().expect("outbound");
            assert_eq!(outbound.content, format!("message {i}"));
        }
    }
}
