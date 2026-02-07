//! WebSocket relay transport for TermChat (UC-004).
//!
//! Implements the [`Transport`] trait over a WebSocket connection to a
//! relay server. Used as the fallback transport in [`super::hybrid::HybridTransport`]
//! when P2P (QUIC) connections are unavailable.
//!
//! The relay server never sees plaintext — only opaque encrypted payloads
//! are forwarded, identified by PeerId for routing.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{Mutex, mpsc};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use termchat_proto::relay::{self, RelayMessage};

use super::{PeerId, Transport, TransportError, TransportType};

/// Type alias for the write half of a WebSocket connection.
type WsSender = futures_util::stream::SplitSink<
    WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;

/// Type alias for the read half of a WebSocket connection.
type WsReader =
    futures_util::stream::SplitStream<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>>;

/// Default timeout for connecting to the relay server.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Timeout for waiting for a `Registered` acknowledgment from the server.
const REGISTER_TIMEOUT: Duration = Duration::from_secs(5);

/// WebSocket relay transport implementing the [`Transport`] trait.
///
/// Connects to a relay server over WebSocket and sends/receives encrypted
/// payloads routed by [`PeerId`]. The relay server never inspects payloads —
/// it only uses the PeerId metadata for routing.
///
/// Created via [`RelayTransport::connect`], which establishes the WebSocket
/// connection, registers with the relay, and spawns a background reader task.
pub struct RelayTransport {
    /// This client's peer identity.
    local_id: PeerId,
    /// The relay server URL (ws:// or wss://).
    relay_url: String,
    /// Write half of the WebSocket connection (shared for concurrent sends).
    ws_sender: Arc<Mutex<WsSender>>,
    /// Channel for messages received from the background reader task.
    incoming: Mutex<mpsc::Receiver<(PeerId, Vec<u8>)>>,
    /// Whether the WebSocket connection to the relay is active.
    connected: Arc<AtomicBool>,
    /// Handle to the background reader task (kept alive for the transport's lifetime).
    _reader_handle: tokio::task::JoinHandle<()>,
}

impl RelayTransport {
    /// Connect to a relay server and register this peer.
    ///
    /// Performs the following steps:
    /// 1. Establishes a WebSocket connection to `relay_url` (10s timeout)
    /// 2. Sends a `Register` message with the local `PeerId`
    /// 3. Waits for a `Registered` acknowledgment (5s timeout)
    /// 4. Spawns a background task to read incoming messages
    ///
    /// # Errors
    ///
    /// - [`TransportError::Timeout`] if connection or registration times out.
    /// - [`TransportError::Unreachable`] if the relay URL cannot be resolved or connected.
    /// - [`TransportError::Io`] for TLS failures or registration rejection.
    pub async fn connect(relay_url: &str, local_id: PeerId) -> Result<Self, TransportError> {
        // Step 1: Connect to the relay WebSocket URL with a timeout.
        let (ws_stream, _response) =
            tokio::time::timeout(CONNECT_TIMEOUT, connect_async(relay_url))
                .await
                .map_err(|_| {
                    tracing::warn!(url = relay_url, "relay WebSocket connect timed out");
                    TransportError::Timeout
                })?
                .map_err(|e| {
                    tracing::warn!(url = relay_url, err = %e, "relay WebSocket connect failed");
                    map_ws_connect_error(e)
                })?;

        // Step 2: Split into sender and receiver halves.
        let (mut ws_sender, mut ws_reader) = ws_stream.split();

        // Step 3: Send Register message.
        let register = RelayMessage::Register {
            peer_id: local_id.as_str().to_string(),
        };
        let register_bytes =
            relay::encode(&register).map_err(|e| TransportError::Io(std::io::Error::other(e)))?;
        ws_sender
            .send(Message::Binary(register_bytes.into()))
            .await
            .map_err(|e| {
                tracing::warn!(err = %e, "failed to send Register message");
                TransportError::Io(std::io::Error::other(format!(
                    "failed to send Register: {e}"
                )))
            })?;

        // Step 4: Wait for Registered acknowledgment with timeout.
        let ack = tokio::time::timeout(REGISTER_TIMEOUT, ws_reader.next())
            .await
            .map_err(|_| {
                tracing::warn!(
                    url = relay_url,
                    "relay registration acknowledgment timed out"
                );
                TransportError::Timeout
            })?;

        match ack {
            Some(Ok(Message::Binary(data))) => match relay::decode(&data) {
                Ok(RelayMessage::Registered { peer_id }) => {
                    tracing::info!(
                        peer_id = %peer_id,
                        url = relay_url,
                        "registered with relay server"
                    );
                }
                Ok(RelayMessage::Error { reason }) => {
                    tracing::warn!(reason = %reason, "relay registration rejected");
                    return Err(TransportError::Io(std::io::Error::other(format!(
                        "relay registration rejected: {reason}"
                    ))));
                }
                Ok(other) => {
                    tracing::warn!(?other, "unexpected relay response during registration");
                    return Err(TransportError::Io(std::io::Error::other(
                        "unexpected response during registration",
                    )));
                }
                Err(e) => {
                    tracing::warn!(err = %e, "malformed relay registration response");
                    return Err(TransportError::Io(std::io::Error::other(format!(
                        "malformed registration response: {e}"
                    ))));
                }
            },
            Some(Ok(Message::Close(_))) => {
                tracing::warn!("relay server closed connection during registration");
                return Err(TransportError::ConnectionClosed);
            }
            Some(Ok(_)) => {
                tracing::warn!("unexpected non-binary frame during registration");
                return Err(TransportError::Io(std::io::Error::other(
                    "unexpected non-binary frame during registration",
                )));
            }
            Some(Err(e)) => {
                tracing::warn!(err = %e, "WebSocket error during registration");
                return Err(TransportError::Io(std::io::Error::other(format!(
                    "WebSocket error during registration: {e}"
                ))));
            }
            None => {
                tracing::warn!("relay WebSocket stream ended during registration");
                return Err(TransportError::ConnectionClosed);
            }
        }

        // Step 5: Spawn background reader task.
        let (tx, rx) = mpsc::channel(256);
        let connected = Arc::new(AtomicBool::new(true));
        let reader_connected = Arc::clone(&connected);

        let reader_handle = tokio::spawn(reader_loop(ws_reader, tx, reader_connected));

        Ok(Self {
            local_id,
            relay_url: relay_url.to_string(),
            ws_sender: Arc::new(Mutex::new(ws_sender)),
            incoming: Mutex::new(rx),
            connected,
            _reader_handle: reader_handle,
        })
    }

    /// Return the relay server URL this transport is connected to.
    pub fn relay_url(&self) -> &str {
        &self.relay_url
    }

    /// Return the local peer ID.
    pub fn local_id(&self) -> &PeerId {
        &self.local_id
    }
}

impl Transport for RelayTransport {
    /// Send an encrypted payload to a peer via the relay server.
    ///
    /// Encodes the payload as a [`RelayMessage::RelayPayload`] and sends it
    /// as a WebSocket binary frame. The relay server routes by the `to` field.
    ///
    /// # Errors
    ///
    /// - [`TransportError::ConnectionClosed`] if the relay connection is down.
    /// - [`TransportError::Io`] for encoding or WebSocket send failures.
    async fn send(&self, peer: &PeerId, payload: &[u8]) -> Result<(), TransportError> {
        if !self.connected.load(Ordering::Relaxed) {
            return Err(TransportError::ConnectionClosed);
        }

        let msg = RelayMessage::RelayPayload {
            from: self.local_id.as_str().to_string(),
            to: peer.as_str().to_string(),
            payload: payload.to_vec(),
        };
        let bytes =
            relay::encode(&msg).map_err(|e| TransportError::Io(std::io::Error::other(e)))?;

        let mut sender = self.ws_sender.lock().await;
        sender
            .send(Message::Binary(bytes.into()))
            .await
            .map_err(|e| {
                tracing::warn!(err = %e, "relay send failed");
                self.connected.store(false, Ordering::Relaxed);
                TransportError::ConnectionClosed
            })?;

        Ok(())
    }

    /// Receive the next message from any peer via the relay.
    ///
    /// Blocks until a message arrives from the background reader task.
    /// Returns the sender's [`PeerId`] (as attested by the relay server)
    /// and the encrypted payload bytes.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::ConnectionClosed`] if the relay connection
    /// has been lost (the background reader task has exited).
    async fn recv(&self) -> Result<(PeerId, Vec<u8>), TransportError> {
        let mut rx = self.incoming.lock().await;
        rx.recv().await.ok_or(TransportError::ConnectionClosed)
    }

    /// Check whether this transport has an active connection to the relay server.
    ///
    /// Note: this indicates relay server connectivity, not whether the specific
    /// peer is registered at the relay. The relay may queue messages for offline
    /// peers, so `send()` can succeed even when the target peer is not online.
    fn is_connected(&self, _peer: &PeerId) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    /// Return [`TransportType::Relay`].
    fn transport_type(&self) -> TransportType {
        TransportType::Relay
    }
}

/// Background task that reads WebSocket messages and dispatches them.
///
/// Parses incoming binary frames as [`RelayMessage`] variants and pushes
/// received payloads into the `tx` channel. Handles protocol messages
/// (`Queued`, `Error`) by logging. Malformed frames are logged and skipped
/// (ext 10a) — the task does not disconnect on bad data.
///
/// Sets `connected` to `false` when the WebSocket closes or errors out.
async fn reader_loop(
    mut ws_reader: WsReader,
    tx: mpsc::Sender<(PeerId, Vec<u8>)>,
    connected: Arc<AtomicBool>,
) {
    while let Some(msg_result) = ws_reader.next().await {
        match msg_result {
            Ok(Message::Binary(data)) => {
                match relay::decode(&data) {
                    Ok(RelayMessage::RelayPayload { from, payload, .. }) => {
                        if tx.send((PeerId::new(from), payload)).await.is_err() {
                            // Receiver dropped — transport was dropped, exit.
                            break;
                        }
                    }
                    Ok(RelayMessage::Queued { to, count }) => {
                        tracing::debug!(
                            to = %to,
                            count = count,
                            "relay queued message for offline peer"
                        );
                    }
                    Ok(RelayMessage::Error { reason }) => {
                        tracing::warn!(reason = %reason, "relay server error");
                    }
                    Ok(other) => {
                        tracing::debug!(?other, "unexpected relay message type");
                    }
                    Err(e) => {
                        // Ext 10a: malformed frame — log and skip, don't disconnect.
                        tracing::warn!(err = %e, "malformed relay frame, skipping");
                    }
                }
            }
            Ok(Message::Close(_)) => {
                tracing::info!("relay WebSocket closed by server");
                break;
            }
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) | Ok(Message::Text(_)) => {
                // Ignore ping/pong/text frames.
            }
            Ok(Message::Frame(_)) => {
                // Raw frame — ignore.
            }
            Err(e) => {
                tracing::warn!(err = %e, "relay WebSocket read error");
                break;
            }
        }
    }
    connected.store(false, Ordering::Relaxed);
    tracing::info!("relay reader task exiting");
}

/// Start a relay server in-process for testing.
///
/// Binds to `127.0.0.1:0` (OS-assigned port) and returns the bound address
/// and a [`tokio::task::JoinHandle`] for cleanup.
#[cfg(test)]
async fn start_test_relay() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    termchat_relay::relay::start_server("127.0.0.1:0")
        .await
        .expect("failed to start test relay server")
}

/// Map a `tokio_tungstenite` connection error to a [`TransportError`].
fn map_ws_connect_error(err: tokio_tungstenite::tungstenite::Error) -> TransportError {
    use tokio_tungstenite::tungstenite::Error as WsError;
    match err {
        WsError::Io(io_err) => {
            // DNS/network failures surface as io errors.
            if io_err.kind() == std::io::ErrorKind::ConnectionRefused
                || io_err.kind() == std::io::ErrorKind::AddrNotAvailable
            {
                TransportError::Unreachable(PeerId::new("relay"))
            } else {
                TransportError::Io(io_err)
            }
        }
        WsError::Tls(_) => {
            // Ext 4c: TLS handshake failure.
            TransportError::Io(std::io::Error::other(format!("TLS error: {err}")))
        }
        WsError::Http(response) => TransportError::Io(std::io::Error::other(format!(
            "relay HTTP error: status {}",
            response.status()
        ))),
        other => TransportError::Io(std::io::Error::other(format!(
            "relay connection error: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Helper: start a test relay server and return a ws:// URL for connecting.
    async fn test_relay_url() -> (String, tokio::task::JoinHandle<()>) {
        let (addr, handle) = start_test_relay().await;
        let url = format!("ws://{addr}/ws");
        (url, handle)
    }

    /// Start a minimal WebSocket server that accepts one connection, performs
    /// the relay handshake, then closes the connection. Used to test disconnect
    /// detection on the client side.
    async fn start_disconnect_server() -> (String, tokio::task::JoinHandle<()>) {
        use futures_util::SinkExt;
        use termchat_proto::relay;
        use tokio::net::TcpListener;
        use tokio_tungstenite::tungstenite as ws;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("ws://{addr}/ws");

        let handle = tokio::spawn(async move {
            // Accept exactly one connection.
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws_stream = tokio_tungstenite::accept_async(stream).await.unwrap();

            // Read Register message.
            if let Some(Ok(ws::Message::Binary(data))) = ws_stream.next().await {
                if let Ok(RelayMessage::Register { peer_id }) = relay::decode(&data) {
                    // Send Registered ack.
                    let ack = RelayMessage::Registered { peer_id };
                    let bytes = relay::encode(&ack).unwrap();
                    let _ = ws_stream.send(ws::Message::Binary(bytes.into())).await;
                }
            }

            // Brief delay then close the connection.
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _ = ws_stream.close(None).await;
            drop(ws_stream);
        });

        (url, handle)
    }

    #[tokio::test]
    async fn connect_and_register_successfully() {
        let (url, _handle) = test_relay_url().await;
        let transport = RelayTransport::connect(&url, PeerId::new("alice")).await;
        assert!(transport.is_ok(), "connect failed: {:?}", transport.err());
    }

    #[tokio::test]
    async fn transport_type_returns_relay() {
        let (url, _handle) = test_relay_url().await;
        let transport = RelayTransport::connect(&url, PeerId::new("alice"))
            .await
            .unwrap();
        assert_eq!(transport.transport_type(), TransportType::Relay);
    }

    #[tokio::test]
    async fn is_connected_true_after_connect() {
        let (url, _handle) = test_relay_url().await;
        let transport = RelayTransport::connect(&url, PeerId::new("alice"))
            .await
            .unwrap();
        assert!(transport.is_connected(&PeerId::new("anyone")));
    }

    #[tokio::test]
    async fn is_connected_false_after_server_shutdown() {
        let (url, _handle) = start_disconnect_server().await;
        let transport = RelayTransport::connect(&url, PeerId::new("alice"))
            .await
            .unwrap();
        assert!(transport.is_connected(&PeerId::new("anyone")));

        // The disconnect server closes the connection shortly after registration.
        // Poll until disconnection is detected (up to 5 seconds).
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::time::Instant::now() < deadline {
            if !transport.is_connected(&PeerId::new("anyone")) {
                return; // Success.
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        assert!(
            !transport.is_connected(&PeerId::new("anyone")),
            "should be disconnected after server closes connection"
        );
    }

    #[tokio::test]
    async fn send_recv_round_trip_through_relay() {
        let (url, _handle) = test_relay_url().await;

        let alice = RelayTransport::connect(&url, PeerId::new("alice"))
            .await
            .unwrap();
        let bob = RelayTransport::connect(&url, PeerId::new("bob"))
            .await
            .unwrap();

        // Alice sends to Bob.
        alice.send(&PeerId::new("bob"), b"hello bob").await.unwrap();

        // Bob receives it.
        let (from, data) = tokio::time::timeout(Duration::from_secs(5), bob.recv())
            .await
            .expect("recv timed out")
            .unwrap();

        assert_eq!(from, PeerId::new("alice"));
        assert_eq!(data, b"hello bob");
    }

    #[tokio::test]
    async fn bidirectional_send_recv() {
        let (url, _handle) = test_relay_url().await;

        let alice = RelayTransport::connect(&url, PeerId::new("alice"))
            .await
            .unwrap();
        let bob = RelayTransport::connect(&url, PeerId::new("bob"))
            .await
            .unwrap();

        // Alice → Bob.
        alice
            .send(&PeerId::new("bob"), b"from alice")
            .await
            .unwrap();
        let (from, data) = tokio::time::timeout(Duration::from_secs(5), bob.recv())
            .await
            .expect("recv timed out")
            .unwrap();
        assert_eq!(from, PeerId::new("alice"));
        assert_eq!(data, b"from alice");

        // Bob → Alice.
        bob.send(&PeerId::new("alice"), b"from bob").await.unwrap();
        let (from, data) = tokio::time::timeout(Duration::from_secs(5), alice.recv())
            .await
            .expect("recv timed out")
            .unwrap();
        assert_eq!(from, PeerId::new("bob"));
        assert_eq!(data, b"from bob");
    }

    #[tokio::test]
    async fn multiple_messages_preserve_fifo_order() {
        let (url, _handle) = test_relay_url().await;

        let alice = RelayTransport::connect(&url, PeerId::new("alice"))
            .await
            .unwrap();
        let bob = RelayTransport::connect(&url, PeerId::new("bob"))
            .await
            .unwrap();

        // Send 20 messages from Alice to Bob.
        for i in 0u32..20 {
            alice
                .send(&PeerId::new("bob"), &i.to_le_bytes())
                .await
                .unwrap();
        }

        // Receive all and verify order.
        for i in 0u32..20 {
            let (from, data) = tokio::time::timeout(Duration::from_secs(5), bob.recv())
                .await
                .expect("recv timed out")
                .unwrap();
            assert_eq!(from, PeerId::new("alice"));
            let received = u32::from_le_bytes(data.try_into().unwrap());
            assert_eq!(received, i, "FIFO order violated at message {i}");
        }
    }

    #[tokio::test]
    async fn send_after_disconnect_returns_connection_closed() {
        let (url, _handle) = start_disconnect_server().await;

        let transport = RelayTransport::connect(&url, PeerId::new("alice"))
            .await
            .unwrap();

        // Wait for the server to close the connection.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::time::Instant::now() < deadline {
            if !transport.is_connected(&PeerId::new("bob")) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let result = transport.send(&PeerId::new("bob"), b"hello").await;
        assert!(
            matches!(result, Err(TransportError::ConnectionClosed)),
            "expected ConnectionClosed, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn recv_after_disconnect_returns_connection_closed() {
        let (url, _handle) = start_disconnect_server().await;

        let transport = RelayTransport::connect(&url, PeerId::new("alice"))
            .await
            .unwrap();

        // The disconnect server will close the connection shortly after registration.
        // recv() should return ConnectionClosed once the reader task detects the close.
        let result = tokio::time::timeout(Duration::from_secs(5), transport.recv()).await;
        match result {
            Ok(Err(TransportError::ConnectionClosed)) => {} // expected
            Ok(other) => panic!("expected ConnectionClosed, got: {:?}", other),
            Err(_) => panic!("recv did not return within timeout after disconnect"),
        }
    }

    #[tokio::test]
    async fn connect_to_nonexistent_server_returns_error() {
        // Use a port that is almost certainly not listening.
        let result = RelayTransport::connect("ws://127.0.0.1:1", PeerId::new("alice")).await;

        assert!(
            result.is_err(),
            "connecting to nonexistent server should fail"
        );
    }

    #[tokio::test]
    async fn local_id_accessor() {
        let (url, _handle) = test_relay_url().await;
        let transport = RelayTransport::connect(&url, PeerId::new("test-peer"))
            .await
            .unwrap();
        assert_eq!(transport.local_id(), &PeerId::new("test-peer"));
    }

    #[tokio::test]
    async fn relay_url_accessor() {
        let (url, _handle) = test_relay_url().await;
        let transport = RelayTransport::connect(&url, PeerId::new("alice"))
            .await
            .unwrap();
        assert_eq!(transport.relay_url(), url);
    }

    #[tokio::test]
    async fn three_peers_exchange_messages() {
        let (url, _handle) = test_relay_url().await;

        let alice = RelayTransport::connect(&url, PeerId::new("alice"))
            .await
            .unwrap();
        let bob = RelayTransport::connect(&url, PeerId::new("bob"))
            .await
            .unwrap();
        let carol = RelayTransport::connect(&url, PeerId::new("carol"))
            .await
            .unwrap();

        // Alice → Carol.
        alice
            .send(&PeerId::new("carol"), b"hi carol")
            .await
            .unwrap();
        let (from, data) = tokio::time::timeout(Duration::from_secs(5), carol.recv())
            .await
            .expect("recv timed out")
            .unwrap();
        assert_eq!(from, PeerId::new("alice"));
        assert_eq!(data, b"hi carol");

        // Bob → Alice.
        bob.send(&PeerId::new("alice"), b"hi alice").await.unwrap();
        let (from, data) = tokio::time::timeout(Duration::from_secs(5), alice.recv())
            .await
            .expect("recv timed out")
            .unwrap();
        assert_eq!(from, PeerId::new("bob"));
        assert_eq!(data, b"hi alice");

        // Carol → Bob.
        carol.send(&PeerId::new("bob"), b"hi bob").await.unwrap();
        let (from, data) = tokio::time::timeout(Duration::from_secs(5), bob.recv())
            .await
            .expect("recv timed out")
            .unwrap();
        assert_eq!(from, PeerId::new("carol"));
        assert_eq!(data, b"hi bob");
    }
}
