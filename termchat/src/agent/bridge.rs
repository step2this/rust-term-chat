//! Unix domain socket bridge for agent connections.
//!
//! Manages the lifecycle of a Unix socket listener that accepts a single
//! agent connection, handles JSON line I/O, and provides connection
//! timeout and cleanup functionality.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::{UnixListener, UnixStream};

use super::AgentError;
use super::protocol::{
    AgentMessage, BridgeHistoryEntry, BridgeMemberInfo, BridgeMessage, PROTOCOL_VERSION,
    decode_line, encode_line, make_unique_agent_peer_id, validate_agent_id,
};

/// A Unix domain socket listener that accepts agent connections.
///
/// The bridge creates a Unix socket at the given path and listens for
/// a single agent process to connect. Stale sockets are automatically
/// cleaned up on startup, and parent directories are created as needed.
pub struct AgentBridge {
    /// The bound listener (taken via `Option::take` when consumed by
    /// [`accept_single_connection`](Self::accept_single_connection)).
    listener: Option<UnixListener>,
    /// Path to the socket file (for cleanup on drop).
    socket_path: PathBuf,
    /// Room ID this bridge is associated with.
    room_id: String,
}

impl AgentBridge {
    /// Creates a new bridge bound to a Unix socket at `socket_path`.
    ///
    /// - Removes a stale socket file if one already exists at the path.
    /// - Creates the parent directory (with mode 0o700) if it does not exist.
    /// - Binds a [`UnixListener`] on the path.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::SocketCreationFailed`] if directory creation,
    /// stale socket removal, or socket binding fails.
    pub fn start(socket_path: &Path, room_id: &str) -> Result<Self, AgentError> {
        // Create parent directory if needed
        if let Some(parent) = socket_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
            // Set owner-only permissions on the directory
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o700);
                std::fs::set_permissions(parent, perms)?;
            }
        }

        // Remove stale socket if it exists
        if socket_path.exists() {
            std::fs::remove_file(socket_path)?;
        }

        let listener = UnixListener::bind(socket_path)?;

        Ok(Self {
            listener: Some(listener),
            socket_path: socket_path.to_path_buf(),
            room_id: room_id.to_string(),
        })
    }

    /// Returns a reference to the inner listener, or an error if it
    /// was already consumed by [`accept_single_connection`](Self::accept_single_connection).
    fn listener_ref(&self) -> Result<&UnixListener, AgentError> {
        self.listener.as_ref().ok_or(AgentError::AlreadyConnected)
    }

    /// Waits for and accepts a single agent connection.
    ///
    /// # Errors
    ///
    /// - [`AgentError::SocketCreationFailed`] if the accept call fails.
    /// - [`AgentError::AlreadyConnected`] if the listener was consumed.
    pub async fn accept_connection(&self) -> Result<AgentConnection, AgentError> {
        let listener = self.listener_ref()?;
        let (stream, _addr) = listener.accept().await?;
        Ok(AgentConnection::new(stream, self.room_id.clone()))
    }

    /// Waits for a single agent connection with a timeout.
    ///
    /// If no agent connects within `timeout`, the socket file is cleaned up
    /// and [`AgentError::Timeout`] is returned.
    ///
    /// # Errors
    ///
    /// - [`AgentError::Timeout`] if no connection arrives before the deadline.
    /// - [`AgentError::SocketCreationFailed`] if the accept call itself fails.
    /// - [`AgentError::AlreadyConnected`] if the listener was consumed.
    pub async fn accept_connection_with_timeout(
        &self,
        timeout: Duration,
    ) -> Result<AgentConnection, AgentError> {
        let listener = self.listener_ref()?;
        match tokio::time::timeout(timeout, listener.accept()).await {
            Ok(Ok((stream, _addr))) => Ok(AgentConnection::new(stream, self.room_id.clone())),
            Ok(Err(io_err)) => Err(AgentError::SocketCreationFailed(io_err)),
            Err(_elapsed) => {
                self.cleanup();
                Err(AgentError::Timeout)
            }
        }
    }

    /// Accepts one connection, then rejects all subsequent connections.
    ///
    /// Takes ownership of the internal listener. After the first connection
    /// is accepted, a background task accepts incoming connections and
    /// immediately sends an [`BridgeMessage::Error`] with code
    /// `"already_connected"` before closing them. Abort the returned
    /// [`tokio::task::JoinHandle`] to stop the rejection loop.
    ///
    /// # Errors
    ///
    /// - [`AgentError::SocketCreationFailed`] if the initial accept fails.
    /// - [`AgentError::AlreadyConnected`] if the listener was already consumed.
    pub async fn accept_single_connection(
        &mut self,
    ) -> Result<(AgentConnection, tokio::task::JoinHandle<()>), AgentError> {
        let listener = self.listener.take().ok_or(AgentError::AlreadyConnected)?;
        let (stream, _addr) = listener.accept().await?;
        let conn = AgentConnection::new(stream, self.room_id.clone());

        let reject_handle = Self::spawn_reject_loop(listener);

        Ok((conn, reject_handle))
    }

    /// Spawns a background task that accepts and immediately rejects connections.
    fn spawn_reject_loop(listener: UnixListener) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            while let Ok((stream, _addr)) = listener.accept().await {
                // Send error and close
                let mut conn = AgentConnection::new(stream, String::new());
                let err_msg = BridgeMessage::Error {
                    code: "already_connected".to_string(),
                    message: "an agent is already connected to this bridge".to_string(),
                };
                let _ = conn.write_message(&err_msg).await;
                conn.close().await;
            }
        })
    }

    /// Shuts down the bridge, removing the socket file.
    ///
    /// After this call, no new connections can be established. The
    /// listener is not explicitly closed (it closes on drop), but the
    /// socket path is removed from the filesystem.
    pub fn shutdown(&mut self) {
        self.cleanup();
    }

    /// Returns the path to the Unix socket file.
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Returns the room ID associated with this bridge.
    #[must_use]
    pub fn room_id(&self) -> &str {
        &self.room_id
    }

    /// Cleans up the socket file from the filesystem.
    ///
    /// This is called automatically on drop but can also be called
    /// explicitly for deterministic cleanup.
    pub fn cleanup(&self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

impl Drop for AgentBridge {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// A connected agent session over a Unix socket.
///
/// Wraps a [`UnixStream`] with buffered JSON line I/O. Messages are
/// read line-by-line and deserialized as [`AgentMessage`]; outgoing
/// [`BridgeMessage`]s are serialized and flushed immediately.
pub struct AgentConnection {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: BufWriter<tokio::net::unix::OwnedWriteHalf>,
    room_id: String,
}

impl AgentConnection {
    /// Creates a new connection wrapper from a raw Unix stream.
    fn new(stream: UnixStream, room_id: String) -> Self {
        let (read_half, write_half) = stream.into_split();
        Self {
            reader: BufReader::new(read_half),
            writer: BufWriter::new(write_half),
            room_id,
        }
    }

    /// Reads the next JSON-line message from the agent.
    ///
    /// Returns the deserialized [`AgentMessage`], or an error if the
    /// connection is closed or the message is malformed.
    ///
    /// # Errors
    ///
    /// - [`AgentError::ConnectionClosed`] if the stream has ended (EOF).
    /// - [`AgentError::SocketCreationFailed`] on I/O errors.
    /// - [`AgentError::JsonError`] if the line is not valid JSON.
    pub async fn read_message(&mut self) -> Result<AgentMessage, AgentError> {
        let mut line = String::new();
        let bytes_read = self.reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            return Err(AgentError::ConnectionClosed);
        }
        decode_line(&line)
    }

    /// Writes a bridge message to the agent as a JSON line.
    ///
    /// The message is serialized, written, and the buffer is flushed
    /// immediately so the agent receives it without delay.
    ///
    /// # Errors
    ///
    /// - [`AgentError::JsonError`] if serialization fails.
    /// - [`AgentError::SocketCreationFailed`] on I/O errors.
    pub async fn write_message(&mut self, msg: &BridgeMessage) -> Result<(), AgentError> {
        let line = encode_line(msg)?;
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }

    /// Gracefully shuts down the connection.
    ///
    /// Flushes any pending writes and shuts down the write half of the
    /// socket. Read errors after this call are expected.
    pub async fn close(&mut self) {
        let _ = self.writer.flush().await;
        let _ = self.writer.shutdown().await;
    }

    /// Returns the room ID associated with this connection.
    #[must_use]
    pub fn room_id(&self) -> &str {
        &self.room_id
    }

    /// Performs the Hello/Welcome handshake on this connection.
    ///
    /// Reads the first message (must be [`AgentMessage::Hello`]), validates
    /// the protocol version and agent ID, generates a unique peer ID, builds
    /// a [`BridgeMessage::Welcome`], and sends it back.
    ///
    /// # Parameters
    ///
    /// - `room_name`: display name of the room the agent is joining.
    /// - `members`: current room member list (converted to [`BridgeMemberInfo`]).
    /// - `history`: recent message history (converted to [`BridgeHistoryEntry`]).
    /// - `existing_peer_ids`: list of existing peer IDs for collision avoidance.
    /// - `max_members`: maximum room capacity (returns `room_full` error if reached).
    ///
    /// # Returns
    ///
    /// On success, returns a [`HandshakeResult`] with the validated agent
    /// identity and the unique peer ID assigned to it.
    ///
    /// # Errors
    ///
    /// Sends an appropriate [`BridgeMessage::Error`] to the agent and returns
    /// an [`AgentError`] if the handshake fails.
    pub async fn perform_handshake(
        &mut self,
        room_name: &str,
        members: &[BridgeMemberInfo],
        history: &[BridgeHistoryEntry],
        existing_peer_ids: &[String],
        max_members: usize,
    ) -> Result<HandshakeResult, AgentError> {
        // Check room capacity before reading Hello
        if members.len() >= max_members {
            let err_msg = BridgeMessage::Error {
                code: "room_full".to_string(),
                message: format!("room is full (max {max_members} members)"),
            };
            let _ = self.write_message(&err_msg).await;
            self.close().await;
            return Err(AgentError::RoomFull);
        }

        // Read first message — must be Hello
        let first_msg = match self.read_message().await {
            Ok(msg) => msg,
            Err(e) => {
                let err_msg = BridgeMessage::Error {
                    code: "invalid_hello".to_string(),
                    message: format!("failed to read Hello message: {e}"),
                };
                let _ = self.write_message(&err_msg).await;
                self.close().await;
                return Err(AgentError::ProtocolError(format!(
                    "expected Hello, got error: {e}"
                )));
            }
        };

        let (protocol_version, agent_id_raw, display_name, capabilities) = match first_msg {
            AgentMessage::Hello {
                protocol_version,
                agent_id,
                display_name,
                capabilities,
            } => (protocol_version, agent_id, display_name, capabilities),
            other => {
                let err_msg = BridgeMessage::Error {
                    code: "invalid_hello".to_string(),
                    message: format!("expected Hello, got {:?}", std::mem::discriminant(&other)),
                };
                let _ = self.write_message(&err_msg).await;
                self.close().await;
                return Err(AgentError::ProtocolError(
                    "first message was not Hello".to_string(),
                ));
            }
        };

        // Validate protocol version
        if protocol_version != PROTOCOL_VERSION {
            let err_msg = BridgeMessage::Error {
                code: "unsupported_version".to_string(),
                message: format!(
                    "unsupported protocol version {protocol_version}, expected {PROTOCOL_VERSION}"
                ),
            };
            let _ = self.write_message(&err_msg).await;
            self.close().await;
            return Err(AgentError::ProtocolError(format!(
                "unsupported version: {protocol_version}"
            )));
        }

        // Validate and sanitize agent ID
        let agent_id = match validate_agent_id(&agent_id_raw) {
            Ok(id) => id,
            Err(e) => {
                let err_msg = BridgeMessage::Error {
                    code: "invalid_agent_id".to_string(),
                    message: e.to_string(),
                };
                let _ = self.write_message(&err_msg).await;
                self.close().await;
                return Err(e);
            }
        };

        // Generate unique peer ID
        let peer_id = make_unique_agent_peer_id(&agent_id, existing_peer_ids);

        // Build and send Welcome
        let welcome = BridgeMessage::Welcome {
            room_id: self.room_id.clone(),
            room_name: room_name.to_string(),
            members: members.to_vec(),
            history: history.to_vec(),
        };
        self.write_message(&welcome).await?;

        Ok(HandshakeResult {
            agent_id,
            display_name,
            peer_id,
            capabilities,
        })
    }
}

/// Result of a successful Hello/Welcome handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeResult {
    /// Validated and sanitized agent ID.
    pub agent_id: String,
    /// Human-readable display name from the Hello message.
    pub display_name: String,
    /// Unique peer ID assigned to this agent (prefixed with `agent:`).
    pub peer_id: String,
    /// Capabilities declared by the agent.
    pub capabilities: Vec<String>,
}

// ---------------------------------------------------------------------------
// Heartbeat
// ---------------------------------------------------------------------------

/// Configuration for the heartbeat ping/pong mechanism.
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// How often to send a `Ping` to the agent.
    pub ping_interval: Duration,
    /// How long to wait for a `Pong` response before declaring the agent dead.
    pub pong_timeout: Duration,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            ping_interval: Duration::from_secs(30),
            pong_timeout: Duration::from_secs(30),
        }
    }
}

/// Runs a heartbeat loop using channels for message I/O.
///
/// This function is intended to be spawned as a tokio task. The main read
/// loop reads all messages from the agent, routes [`AgentMessage::Pong`]
/// replies to `pong_rx`, and drains `ping_tx` to write outbound Pings.
///
/// # Cancellation
///
/// To stop the heartbeat, drop the `pong_tx` side (sender that feeds
/// `pong_rx`). The loop will detect the channel closure and exit cleanly.
///
/// # Returns
///
/// - `Ok(())` if the pong channel is closed (normal shutdown).
/// - `Err(AgentError::Timeout)` if the agent misses a pong deadline.
/// - `Err(AgentError::ConnectionClosed)` if the ping channel is closed.
///
/// # Errors
///
/// Returns [`AgentError::Timeout`] if the agent does not respond to a ping
/// within the configured `pong_timeout`, or [`AgentError::ConnectionClosed`]
/// if the ping channel is closed (connection dropped).
pub async fn heartbeat_loop(
    ping_tx: tokio::sync::mpsc::Sender<BridgeMessage>,
    mut pong_rx: tokio::sync::mpsc::Receiver<()>,
    config: HeartbeatConfig,
) -> Result<(), AgentError> {
    loop {
        tokio::time::sleep(config.ping_interval).await;

        // Send Ping via channel (main loop writes it to the socket)
        if ping_tx.send(BridgeMessage::Ping).await.is_err() {
            // Receiver dropped — connection closed
            return Err(AgentError::ConnectionClosed);
        }

        // Wait for Pong or timeout
        tokio::select! {
            pong = pong_rx.recv() => {
                match pong {
                    Some(()) => {
                        // Pong received, continue to next ping cycle
                    }
                    None => {
                        // Channel closed — connection is gone, clean exit
                        return Ok(());
                    }
                }
            }
            () = tokio::time::sleep(config.pong_timeout) => {
                return Err(AgentError::Timeout);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Creates a unique temporary socket path for each test.
    fn temp_socket_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("termchat-test-bridge");
        dir.join(format!("{name}-{}.sock", std::process::id()))
    }

    #[tokio::test]
    async fn bridge_creates_socket_file() {
        let path = temp_socket_path("creates");
        let bridge = AgentBridge::start(&path, "room-1").expect("start");
        assert!(path.exists());
        drop(bridge);
    }

    #[tokio::test]
    async fn bridge_cleans_up_on_drop() {
        let path = temp_socket_path("cleanup");
        {
            let _bridge = AgentBridge::start(&path, "room-1").expect("start");
            assert!(path.exists());
        }
        // Socket file removed by Drop
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn bridge_handles_stale_socket() {
        let path = temp_socket_path("stale");
        // Create a first bridge
        let bridge1 = AgentBridge::start(&path, "room-1").expect("first start");
        // Drop it but file might linger in some edge cases — simulate by leaking
        std::mem::forget(bridge1);

        // Second bridge should succeed by removing the stale socket
        let bridge2 = AgentBridge::start(&path, "room-1").expect("second start");
        assert!(path.exists());
        drop(bridge2);
    }

    #[tokio::test]
    async fn bridge_creates_parent_directory() {
        let dir = std::env::temp_dir()
            .join("termchat-test-bridge")
            .join(format!("nested-{}", std::process::id()));
        let path = dir.join("agent.sock");

        // Ensure the nested dir doesn't exist yet
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!dir.exists());

        let bridge = AgentBridge::start(&path, "room-1").expect("start");
        assert!(dir.exists());
        assert!(path.exists());
        drop(bridge);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn bridge_room_id_accessor() {
        let path = temp_socket_path("room-acc");
        let bridge = AgentBridge::start(&path, "test-room-42").expect("start");
        assert_eq!(bridge.room_id(), "test-room-42");
        drop(bridge);
    }

    #[tokio::test]
    async fn connection_read_write_round_trip() {
        let path = temp_socket_path("rw");
        let bridge = AgentBridge::start(&path, "room-1").expect("start");

        // Spawn a client that sends a Hello and reads the Welcome back
        let client_path = path.clone();
        let client_handle = tokio::spawn(async move {
            let stream = UnixStream::connect(&client_path).await.expect("connect");
            let (read_half, write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);
            let mut writer = BufWriter::new(write_half);

            // Send a Hello message
            let hello = AgentMessage::Hello {
                protocol_version: 1,
                agent_id: "test-agent".to_string(),
                display_name: "TestBot".to_string(),
                capabilities: vec![],
            };
            let line = encode_line(&hello).expect("encode");
            writer.write_all(line.as_bytes()).await.expect("write");
            writer.flush().await.expect("flush");

            // Read the Welcome back
            let mut resp_line = String::new();
            reader.read_line(&mut resp_line).await.expect("read");
            let resp: BridgeMessage = decode_line(&resp_line).expect("decode");
            resp
        });

        // Accept the connection and handle the exchange
        let mut conn = bridge.accept_connection().await.expect("accept");

        // Read the Hello
        let msg = conn.read_message().await.expect("read");
        assert!(matches!(msg, AgentMessage::Hello { .. }));

        // Send a Welcome back
        let welcome = BridgeMessage::Welcome {
            room_id: "room-1".to_string(),
            room_name: "Test Room".to_string(),
            members: vec![],
            history: vec![],
        };
        conn.write_message(&welcome).await.expect("write");

        // Verify the client received it
        let resp = client_handle.await.expect("join");
        assert!(matches!(resp, BridgeMessage::Welcome { .. }));

        conn.close().await;
        drop(bridge);
    }

    #[tokio::test]
    async fn connection_read_eof_returns_connection_closed() {
        let path = temp_socket_path("eof");
        let bridge = AgentBridge::start(&path, "room-1").expect("start");

        let client_path = path.clone();
        let client_handle = tokio::spawn(async move {
            let stream = UnixStream::connect(&client_path).await.expect("connect");
            // Immediately drop — EOF on the bridge side
            drop(stream);
        });

        let mut conn = bridge.accept_connection().await.expect("accept");
        client_handle.await.expect("join");

        let result = conn.read_message().await;
        assert!(matches!(result, Err(AgentError::ConnectionClosed)));

        drop(bridge);
    }

    #[tokio::test]
    async fn connection_read_invalid_json_returns_error() {
        let path = temp_socket_path("badjson");
        let bridge = AgentBridge::start(&path, "room-1").expect("start");

        let client_path = path.clone();
        let client_handle = tokio::spawn(async move {
            let stream = UnixStream::connect(&client_path).await.expect("connect");
            let (_read_half, write_half) = stream.into_split();
            let mut writer = BufWriter::new(write_half);
            writer.write_all(b"not valid json\n").await.expect("write");
            writer.flush().await.expect("flush");
        });

        let mut conn = bridge.accept_connection().await.expect("accept");
        client_handle.await.expect("join");

        let result = conn.read_message().await;
        assert!(matches!(result, Err(AgentError::JsonError(_))));

        drop(bridge);
    }

    #[tokio::test]
    async fn connection_room_id_accessor() {
        let path = temp_socket_path("conn-room");
        let bridge = AgentBridge::start(&path, "my-room").expect("start");

        let client_path = path.clone();
        tokio::spawn(async move {
            let _stream = UnixStream::connect(&client_path).await.expect("connect");
        });

        let conn = bridge.accept_connection().await.expect("accept");
        assert_eq!(conn.room_id(), "my-room");

        drop(bridge);
    }

    // --- Timeout tests ---

    #[tokio::test]
    async fn accept_with_timeout_fires_on_no_connection() {
        let path = temp_socket_path("timeout");
        let bridge = AgentBridge::start(&path, "room-1").expect("start");

        let result = bridge
            .accept_connection_with_timeout(Duration::from_millis(50))
            .await;
        assert!(matches!(result, Err(AgentError::Timeout)));

        // Socket should be cleaned up after timeout
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn accept_with_timeout_succeeds_before_deadline() {
        let path = temp_socket_path("timeout-ok");
        let bridge = AgentBridge::start(&path, "room-1").expect("start");

        let client_path = path.clone();
        tokio::spawn(async move {
            // Small delay to simulate a real agent connecting
            tokio::time::sleep(Duration::from_millis(10)).await;
            let _stream = UnixStream::connect(&client_path).await.expect("connect");
            // Keep the stream alive briefly
            tokio::time::sleep(Duration::from_millis(100)).await;
        });

        let result = bridge
            .accept_connection_with_timeout(Duration::from_secs(5))
            .await;
        assert!(result.is_ok());

        drop(bridge);
    }

    // --- Single connection (reject subsequent) tests ---

    #[tokio::test]
    async fn accept_single_connection_rejects_second() {
        let path = temp_socket_path("single");
        let mut bridge = AgentBridge::start(&path, "room-1").expect("start");

        let client_path = path.clone();
        // First client connects
        let first_client = tokio::spawn(async move {
            let _stream = UnixStream::connect(&client_path).await.expect("connect");
            // Keep alive while second client tries
            tokio::time::sleep(Duration::from_millis(300)).await;
        });

        let (conn, reject_handle) = bridge.accept_single_connection().await.expect("accept");

        // Second client connects and should receive an error
        let client_path2 = path.clone();
        let second_result = tokio::spawn(async move {
            // Small delay to let the reject loop start
            tokio::time::sleep(Duration::from_millis(50)).await;
            let stream = UnixStream::connect(&client_path2).await.expect("connect2");
            let (read_half, _write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);
            let mut line = String::new();
            let n = reader.read_line(&mut line).await.expect("read");
            if n == 0 {
                return None;
            }
            let msg: BridgeMessage = decode_line(&line).expect("decode");
            Some(msg)
        });

        let second_msg = second_result.await.expect("join");
        assert!(matches!(
            second_msg,
            Some(BridgeMessage::Error { ref code, .. }) if code == "already_connected"
        ));

        reject_handle.abort();
        first_client.abort();
        drop(conn);
    }

    // --- Shutdown test ---

    #[tokio::test]
    async fn shutdown_removes_socket_file() {
        let path = temp_socket_path("shutdown");
        let mut bridge = AgentBridge::start(&path, "room-1").expect("start");
        assert!(path.exists());

        bridge.shutdown();
        assert!(!path.exists());
    }

    // --- Handshake tests ---

    /// Helper: spawn a client that sends a Hello and returns the response.
    async fn spawn_hello_client(
        path: PathBuf,
        hello: AgentMessage,
    ) -> tokio::task::JoinHandle<Option<BridgeMessage>> {
        tokio::spawn(async move {
            let stream = UnixStream::connect(&path).await.expect("connect");
            let (read_half, write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);
            let mut writer = BufWriter::new(write_half);

            let line = encode_line(&hello).expect("encode");
            writer.write_all(line.as_bytes()).await.expect("write");
            writer.flush().await.expect("flush");

            let mut resp_line = String::new();
            let n = reader.read_line(&mut resp_line).await.expect("read");
            if n == 0 {
                return None;
            }
            Some(decode_line(&resp_line).expect("decode"))
        })
    }

    #[tokio::test]
    async fn handshake_success() {
        let path = temp_socket_path("hs-ok");
        let bridge = AgentBridge::start(&path, "room-abc").expect("start");

        let members = vec![BridgeMemberInfo {
            peer_id: "peer-alice".to_string(),
            display_name: "Alice".to_string(),
            is_admin: true,
            is_agent: false,
        }];
        let history = vec![BridgeHistoryEntry {
            sender_id: "peer-alice".to_string(),
            sender_name: "Alice".to_string(),
            content: "hello".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
        }];

        let hello = AgentMessage::Hello {
            protocol_version: 1,
            agent_id: "claude-bot".to_string(),
            display_name: "Claude".to_string(),
            capabilities: vec!["chat".to_string()],
        };
        let client = spawn_hello_client(path.clone(), hello).await;

        let mut conn = bridge.accept_connection().await.expect("accept");
        let result = conn
            .perform_handshake(
                "General",
                &members,
                &history,
                &["peer-alice".to_string()],
                256,
            )
            .await;

        let hs = result.expect("handshake");
        assert_eq!(hs.agent_id, "claude-bot");
        assert_eq!(hs.display_name, "Claude");
        assert_eq!(hs.peer_id, "agent:claude-bot");
        assert_eq!(hs.capabilities, vec!["chat".to_string()]);

        let resp = client.await.expect("join").expect("response");
        match resp {
            BridgeMessage::Welcome {
                room_id,
                room_name,
                members: m,
                history: h,
            } => {
                assert_eq!(room_id, "room-abc");
                assert_eq!(room_name, "General");
                assert_eq!(m.len(), 1);
                assert_eq!(h.len(), 1);
            }
            other => panic!("expected Welcome, got {other:?}"),
        }

        drop(bridge);
    }

    #[tokio::test]
    async fn handshake_bad_version() {
        let path = temp_socket_path("hs-ver");
        let bridge = AgentBridge::start(&path, "room-1").expect("start");

        let hello = AgentMessage::Hello {
            protocol_version: 999,
            agent_id: "bot".to_string(),
            display_name: "Bot".to_string(),
            capabilities: vec![],
        };
        let client = spawn_hello_client(path.clone(), hello).await;

        let mut conn = bridge.accept_connection().await.expect("accept");
        let result = conn.perform_handshake("Room", &[], &[], &[], 256).await;

        assert!(matches!(result, Err(AgentError::ProtocolError(_))));

        let resp = client.await.expect("join").expect("response");
        match resp {
            BridgeMessage::Error { code, .. } => assert_eq!(code, "unsupported_version"),
            other => panic!("expected Error, got {other:?}"),
        }

        drop(bridge);
    }

    #[tokio::test]
    async fn handshake_invalid_agent_id() {
        let path = temp_socket_path("hs-id");
        let bridge = AgentBridge::start(&path, "room-1").expect("start");

        let hello = AgentMessage::Hello {
            protocol_version: 1,
            agent_id: "   ".to_string(), // whitespace only
            display_name: "Bot".to_string(),
            capabilities: vec![],
        };
        let client = spawn_hello_client(path.clone(), hello).await;

        let mut conn = bridge.accept_connection().await.expect("accept");
        let result = conn.perform_handshake("Room", &[], &[], &[], 256).await;

        assert!(matches!(result, Err(AgentError::InvalidAgentId(_))));

        let resp = client.await.expect("join").expect("response");
        match resp {
            BridgeMessage::Error { code, .. } => assert_eq!(code, "invalid_agent_id"),
            other => panic!("expected Error, got {other:?}"),
        }

        drop(bridge);
    }

    #[tokio::test]
    async fn handshake_malformed_json() {
        let path = temp_socket_path("hs-json");
        let bridge = AgentBridge::start(&path, "room-1").expect("start");

        // Send raw garbage instead of a Hello
        let client_path = path.clone();
        let client = tokio::spawn(async move {
            let stream = UnixStream::connect(&client_path).await.expect("connect");
            let (read_half, write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);
            let mut writer = BufWriter::new(write_half);

            writer.write_all(b"NOT JSON\n").await.expect("write");
            writer.flush().await.expect("flush");

            let mut resp_line = String::new();
            let n = reader.read_line(&mut resp_line).await.expect("read");
            if n == 0 {
                return None;
            }
            let msg: BridgeMessage = decode_line(&resp_line).expect("decode");
            Some(msg)
        });

        let mut conn = bridge.accept_connection().await.expect("accept");
        let result = conn.perform_handshake("Room", &[], &[], &[], 256).await;

        assert!(matches!(result, Err(AgentError::ProtocolError(_))));

        let resp = client.await.expect("join").expect("response");
        match resp {
            BridgeMessage::Error { code, .. } => assert_eq!(code, "invalid_hello"),
            other => panic!("expected Error, got {other:?}"),
        }

        drop(bridge);
    }

    #[tokio::test]
    async fn handshake_not_hello_message() {
        let path = temp_socket_path("hs-nothello");
        let bridge = AgentBridge::start(&path, "room-1").expect("start");

        // Send a Pong instead of Hello
        let hello = AgentMessage::Pong;
        let client = spawn_hello_client(path.clone(), hello).await;

        let mut conn = bridge.accept_connection().await.expect("accept");
        let result = conn.perform_handshake("Room", &[], &[], &[], 256).await;

        assert!(matches!(result, Err(AgentError::ProtocolError(_))));

        let resp = client.await.expect("join").expect("response");
        match resp {
            BridgeMessage::Error { code, .. } => assert_eq!(code, "invalid_hello"),
            other => panic!("expected Error, got {other:?}"),
        }

        drop(bridge);
    }

    #[tokio::test]
    async fn handshake_room_full() {
        let path = temp_socket_path("hs-full");
        let bridge = AgentBridge::start(&path, "room-1").expect("start");

        // Create members at capacity
        let members: Vec<BridgeMemberInfo> = (0..256)
            .map(|i| BridgeMemberInfo {
                peer_id: format!("peer-{i}"),
                display_name: format!("User {i}"),
                is_admin: i == 0,
                is_agent: false,
            })
            .collect();

        let hello = AgentMessage::Hello {
            protocol_version: 1,
            agent_id: "bot".to_string(),
            display_name: "Bot".to_string(),
            capabilities: vec![],
        };
        let client = spawn_hello_client(path.clone(), hello).await;

        let mut conn = bridge.accept_connection().await.expect("accept");
        let result = conn
            .perform_handshake("Room", &members, &[], &[], 256)
            .await;

        assert!(matches!(result, Err(AgentError::RoomFull)));

        let resp = client.await.expect("join").expect("response");
        match resp {
            BridgeMessage::Error { code, .. } => assert_eq!(code, "room_full"),
            other => panic!("expected Error, got {other:?}"),
        }

        drop(bridge);
    }

    #[tokio::test]
    async fn handshake_empty_history() {
        let path = temp_socket_path("hs-nohist");
        let bridge = AgentBridge::start(&path, "room-1").expect("start");

        let hello = AgentMessage::Hello {
            protocol_version: 1,
            agent_id: "bot".to_string(),
            display_name: "Bot".to_string(),
            capabilities: vec![],
        };
        let client = spawn_hello_client(path.clone(), hello).await;

        let mut conn = bridge.accept_connection().await.expect("accept");
        let result = conn
            .perform_handshake("Empty Room", &[], &[], &[], 256)
            .await;

        assert!(result.is_ok());

        let resp = client.await.expect("join").expect("response");
        match resp {
            BridgeMessage::Welcome { history, .. } => {
                assert!(history.is_empty());
            }
            other => panic!("expected Welcome, got {other:?}"),
        }

        drop(bridge);
    }

    #[tokio::test]
    async fn handshake_generates_unique_peer_id() {
        let path = temp_socket_path("hs-uniq");
        let bridge = AgentBridge::start(&path, "room-1").expect("start");

        let existing = vec!["agent:bot".to_string()];

        let hello = AgentMessage::Hello {
            protocol_version: 1,
            agent_id: "bot".to_string(),
            display_name: "Bot".to_string(),
            capabilities: vec![],
        };
        let _client = spawn_hello_client(path.clone(), hello).await;

        let mut conn = bridge.accept_connection().await.expect("accept");
        let result = conn
            .perform_handshake("Room", &[], &[], &existing, 256)
            .await;

        let hs = result.expect("handshake");
        assert_eq!(hs.peer_id, "agent:bot-2");

        drop(bridge);
    }

    // --- Heartbeat tests ---

    #[tokio::test]
    async fn heartbeat_pong_received_resets_timer() {
        let (ping_tx, mut ping_rx) = tokio::sync::mpsc::channel::<BridgeMessage>(8);
        let (pong_tx, pong_rx) = tokio::sync::mpsc::channel::<()>(8);

        let config = HeartbeatConfig {
            ping_interval: Duration::from_millis(50),
            pong_timeout: Duration::from_millis(200),
        };

        let hb_handle = tokio::spawn(heartbeat_loop(ping_tx, pong_rx, config));

        // Wait for first Ping
        let msg = ping_rx.recv().await.expect("ping 1");
        assert!(matches!(msg, BridgeMessage::Ping));

        // Send Pong
        pong_tx.send(()).await.expect("pong 1");

        // Wait for second Ping (timer reset)
        let msg = ping_rx.recv().await.expect("ping 2");
        assert!(matches!(msg, BridgeMessage::Ping));

        // Send Pong
        pong_tx.send(()).await.expect("pong 2");

        // Clean shutdown by dropping pong sender
        drop(pong_tx);

        // Wait for third Ping
        let _msg = ping_rx.recv().await;

        let result = hb_handle.await.expect("join");
        // Should exit cleanly (pong channel closed)
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn heartbeat_missing_pong_triggers_timeout() {
        let (ping_tx, mut ping_rx) = tokio::sync::mpsc::channel::<BridgeMessage>(8);
        let (_pong_tx, pong_rx) = tokio::sync::mpsc::channel::<()>(8);

        let config = HeartbeatConfig {
            ping_interval: Duration::from_millis(50),
            pong_timeout: Duration::from_millis(100),
        };

        let hb_handle = tokio::spawn(heartbeat_loop(ping_tx, pong_rx, config));

        // Wait for Ping but don't send Pong
        let msg = ping_rx.recv().await.expect("ping");
        assert!(matches!(msg, BridgeMessage::Ping));

        // Heartbeat should timeout
        let result = hb_handle.await.expect("join");
        assert!(matches!(result, Err(AgentError::Timeout)));
    }

    #[tokio::test]
    async fn heartbeat_stops_when_ping_channel_closes() {
        let (ping_tx, ping_rx) = tokio::sync::mpsc::channel::<BridgeMessage>(8);
        let (_pong_tx, pong_rx) = tokio::sync::mpsc::channel::<()>(8);

        let config = HeartbeatConfig {
            ping_interval: Duration::from_millis(50),
            pong_timeout: Duration::from_millis(200),
        };

        let hb_handle = tokio::spawn(heartbeat_loop(ping_tx, pong_rx, config));

        // Drop the ping receiver — simulates connection close
        drop(ping_rx);

        let result = hb_handle.await.expect("join");
        assert!(matches!(result, Err(AgentError::ConnectionClosed)));
    }

    #[tokio::test]
    async fn heartbeat_stops_when_pong_channel_closes() {
        let (ping_tx, mut ping_rx) = tokio::sync::mpsc::channel::<BridgeMessage>(8);
        let (pong_tx, pong_rx) = tokio::sync::mpsc::channel::<()>(8);

        let config = HeartbeatConfig {
            ping_interval: Duration::from_millis(50),
            pong_timeout: Duration::from_millis(500),
        };

        let hb_handle = tokio::spawn(heartbeat_loop(ping_tx, pong_rx, config));

        // Wait for first Ping
        let _ = ping_rx.recv().await;

        // Drop pong sender to close channel
        drop(pong_tx);

        let result = hb_handle.await.expect("join");
        // Should exit cleanly
        assert!(result.is_ok());
    }
}
