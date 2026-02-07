//! QUIC-based P2P transport for TermChat (UC-003).
//!
//! Provides [`QuicTransport`], a [`Transport`] implementation using QUIC via
//! the `quinn` crate, and [`QuicListener`] for accepting incoming connections.
//!
//! QUIC TLS provides transport encryption. Peer authentication uses the
//! Noise XX handshake (UC-005), not TLS certificates.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use super::{PeerId, Transport, TransportError, TransportType};

/// Maximum payload size accepted by recv (64 KB).
const MAX_PAYLOAD_SIZE: u32 = 65_536;

/// Default connection timeout for the initiator side.
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Stream initialization marker. Sent by the initiator after `open_bi()`
/// to flush the QUIC STREAM frame, ensuring the responder's `accept_bi()`
/// returns promptly.
const STREAM_INIT_MARKER: u8 = 0x01;

// ---------------------------------------------------------------------------
// TLS configuration (T-003-02)
// ---------------------------------------------------------------------------

/// Generate an ephemeral self-signed X.509 certificate and private key.
///
/// Used for QUIC transport encryption only. Peer authentication happens
/// via the Noise XX handshake (UC-005), not TLS certificates.
pub fn generate_self_signed_cert() -> Result<
    (
        rustls::pki_types::CertificateDer<'static>,
        rustls::pki_types::PrivatePkcs8KeyDer<'static>,
    ),
    TransportError,
> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).map_err(|e| {
        TransportError::Io(std::io::Error::other(format!(
            "certificate generation failed: {e}"
        )))
    })?;
    let cert_der = rustls::pki_types::CertificateDer::from(cert.cert);
    let key_der = rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());
    Ok((cert_der, key_der))
}

/// Build a [`quinn::ServerConfig`] using the provided self-signed certificate.
///
/// Uses the `ring` crypto provider (via quinn's built-in rustls integration)
/// and presents the given certificate to connecting clients.
pub fn make_server_config(
    cert_der: rustls::pki_types::CertificateDer<'static>,
    key_der: rustls::pki_types::PrivatePkcs8KeyDer<'static>,
) -> Result<quinn::ServerConfig, TransportError> {
    let mut server_config = quinn::ServerConfig::with_single_cert(vec![cert_der], key_der.into())
        .map_err(|e| {
        TransportError::Io(std::io::Error::other(format!(
            "QUIC server config error: {e}"
        )))
    })?;

    // Enable keep-alive so stale connections are detected.
    let transport = Arc::get_mut(&mut server_config.transport).expect("unique Arc");
    transport.keep_alive_interval(Some(Duration::from_secs(15)));

    Ok(server_config)
}

/// Build a [`quinn::ClientConfig`] that skips certificate verification.
///
/// QUIC TLS is for transport encryption only. Real peer authentication
/// happens via the Noise XX handshake (UC-005).
pub fn make_client_config() -> Result<quinn::ClientConfig, TransportError> {
    let client_crypto = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .map_err(|e| {
        TransportError::Io(std::io::Error::other(format!(
            "TLS client config error: {e}"
        )))
    })?
    .dangerous()
    .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
    .with_no_client_auth();

    let client_config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto).map_err(|e| {
            TransportError::Io(std::io::Error::other(format!(
                "QUIC client config error: {e}"
            )))
        })?,
    ));

    Ok(client_config)
}

/// A [`rustls::client::danger::ServerCertVerifier`] that accepts all certificates.
///
/// This is intentional — QUIC TLS provides transport encryption, but peer
/// authentication is handled by the Noise XX handshake (UC-005).
#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

// ---------------------------------------------------------------------------
// QuicListener (T-003-03)
// ---------------------------------------------------------------------------

/// Listens for incoming QUIC connections and produces [`QuicTransport`] instances.
///
/// The listener wraps a [`quinn::Endpoint`] configured as a server. Each call
/// to [`accept`](QuicListener::accept) awaits the next incoming connection,
/// accepts a bidirectional stream, and returns a ready-to-use transport.
pub struct QuicListener {
    /// The QUIC endpoint accepting connections.
    endpoint: quinn::Endpoint,
    /// Identity of the local peer.
    local_id: PeerId,
}

impl QuicListener {
    /// Bind a QUIC listener to the given socket address.
    ///
    /// Generates an ephemeral self-signed certificate for QUIC transport
    /// encryption. Use `0.0.0.0:0` to let the OS assign a port.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::Io`] if the address cannot be bound or
    /// the TLS configuration fails.
    pub async fn bind(addr: SocketAddr, local_id: PeerId) -> Result<Self, TransportError> {
        let (cert_der, key_der) = generate_self_signed_cert()?;
        let server_config = make_server_config(cert_der, key_der)?;
        let endpoint = quinn::Endpoint::server(server_config, addr)?;
        Ok(Self { endpoint, local_id })
    }

    /// Return the local socket address this listener is bound to.
    ///
    /// Useful when binding to port 0 to discover the OS-assigned port.
    pub fn local_addr(&self) -> Result<SocketAddr, TransportError> {
        self.endpoint.local_addr().map_err(TransportError::Io)
    }

    /// Await the next incoming QUIC connection.
    ///
    /// Accepts the connection and its first bidirectional stream, wrapping
    /// both in a [`QuicTransport`]. The remote peer ID is derived from
    /// the remote socket address (placeholder until Noise identity is available).
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::ConnectionClosed`] if the endpoint is closed,
    /// or [`TransportError::Io`] if the connection or stream negotiation fails.
    pub async fn accept(&self) -> Result<QuicTransport, TransportError> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or(TransportError::ConnectionClosed)?;

        let connection = incoming.await.map_err(|e| {
            tracing::warn!(err = %e, "QUIC accept handshake failed");
            TransportError::Io(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                format!("QUIC accept failed: {e}"),
            ))
        })?;

        let remote_addr = connection.remote_address();
        let remote_id = PeerId::new(remote_addr.to_string());

        let (send_stream, mut recv_stream) = connection.accept_bi().await.map_err(|e| {
            tracing::warn!(err = %e, addr = %remote_addr, "QUIC stream accept failed");
            TransportError::Io(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                format!("QUIC stream accept failed: {e}"),
            ))
        })?;

        // Read and discard the stream-init marker written by the initiator.
        let mut marker = [0u8; 1];
        recv_stream.read_exact(&mut marker).await.map_err(|e| {
            tracing::warn!(err = %e, "QUIC stream init marker read failed");
            map_read_exact_error(e)
        })?;

        Ok(QuicTransport {
            local_id: self.local_id.clone(),
            remote_id,
            connection,
            send_stream: Mutex::new(send_stream),
            recv_stream: Mutex::new(recv_stream),
        })
    }

    /// Close the listener endpoint, rejecting any in-flight connections.
    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"shutdown");
    }
}

// ---------------------------------------------------------------------------
// QuicTransport (T-003-04 through T-003-07)
// ---------------------------------------------------------------------------

/// QUIC-based P2P transport implementing the [`Transport`] trait.
///
/// Wraps a single point-to-point QUIC connection with one bidirectional
/// stream. Messages are length-prefixed on the wire (4-byte LE prefix
/// followed by the payload).
///
/// Created either via [`QuicTransport::connect`] (initiator) or
/// [`QuicListener::accept`] (responder).
pub struct QuicTransport {
    /// Identity of the local peer.
    local_id: PeerId,
    /// Identity of the remote peer.
    remote_id: PeerId,
    /// The underlying QUIC connection (for status checks).
    connection: quinn::Connection,
    /// Write half of the bidirectional stream (mutex for thread safety).
    send_stream: Mutex<quinn::SendStream>,
    /// Read half of the bidirectional stream (mutex for exclusive read access).
    recv_stream: Mutex<quinn::RecvStream>,
}

impl QuicTransport {
    /// Connect to a remote peer as the initiator.
    ///
    /// Dials the responder at `addr`, performs the QUIC handshake with a
    /// configurable timeout (default 10 seconds), opens a bidirectional
    /// stream, and returns the ready-to-use transport.
    ///
    /// # Errors
    ///
    /// - [`TransportError::Timeout`] if the connection is not established
    ///   within the timeout period.
    /// - [`TransportError::Unreachable`] if the peer address cannot be reached.
    /// - [`TransportError::Io`] for other connection or stream errors.
    pub async fn connect(
        addr: SocketAddr,
        local_id: PeerId,
        remote_id: PeerId,
    ) -> Result<Self, TransportError> {
        Self::connect_with_timeout(addr, local_id, remote_id, DEFAULT_CONNECT_TIMEOUT).await
    }

    /// Connect to a remote peer with a custom timeout.
    ///
    /// Same as [`connect`](QuicTransport::connect) but with an explicit
    /// timeout duration.
    pub async fn connect_with_timeout(
        addr: SocketAddr,
        local_id: PeerId,
        remote_id: PeerId,
        timeout: Duration,
    ) -> Result<Self, TransportError> {
        let client_config = make_client_config()?;

        // Bind to an OS-assigned port for the client endpoint.
        let mut endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().expect("valid bind addr"))?;
        endpoint.set_default_client_config(client_config);

        let connecting = endpoint.connect(addr, "localhost").map_err(|e| {
            tracing::warn!(err = %e, addr = %addr, "QUIC connect initiation failed");
            TransportError::Unreachable(remote_id.clone())
        })?;

        let connection =
            tokio::time::timeout(timeout, connecting)
                .await
                .map_err(|_| {
                    tracing::warn!(addr = %addr, timeout_secs = timeout.as_secs(), "QUIC connect timed out");
                    TransportError::Timeout
                })?
                .map_err(|e| {
                    tracing::warn!(err = %e, addr = %addr, "QUIC handshake failed");
                    map_connection_error(e, &remote_id)
                })?;

        let (mut send_stream, recv_stream) = connection.open_bi().await.map_err(|e| {
            tracing::warn!(err = %e, addr = %addr, "QUIC stream open failed");
            TransportError::Io(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                format!("QUIC stream open failed: {e}"),
            ))
        })?;

        // Write the stream-init marker to flush the STREAM frame so the
        // responder's accept_bi() returns promptly.
        send_stream
            .write_all(&[STREAM_INIT_MARKER])
            .await
            .map_err(|e| {
                tracing::warn!(err = %e, "QUIC stream init marker write failed");
                map_write_error(e)
            })?;

        Ok(Self {
            local_id,
            remote_id,
            connection,
            send_stream: Mutex::new(send_stream),
            recv_stream: Mutex::new(recv_stream),
        })
    }

    /// Return the local peer ID.
    pub fn local_id(&self) -> &PeerId {
        &self.local_id
    }

    /// Return the remote peer ID.
    pub fn remote_id(&self) -> &PeerId {
        &self.remote_id
    }
}

impl Transport for QuicTransport {
    async fn send(&self, peer: &PeerId, payload: &[u8]) -> Result<(), TransportError> {
        if *peer != self.remote_id {
            return Err(TransportError::Unreachable(peer.clone()));
        }

        let len = payload.len() as u32;
        let len_bytes = len.to_le_bytes();

        let mut stream = self.send_stream.lock().await;
        stream.write_all(&len_bytes).await.map_err(|e| {
            tracing::error!(err = %e, "QUIC send: failed to write length prefix");
            map_write_error(e)
        })?;
        stream.write_all(payload).await.map_err(|e| {
            tracing::error!(err = %e, "QUIC send: failed to write payload");
            map_write_error(e)
        })?;

        Ok(())
    }

    async fn recv(&self) -> Result<(PeerId, Vec<u8>), TransportError> {
        let mut stream = self.recv_stream.lock().await;

        // Read the 4-byte length prefix.
        let mut len_buf = [0u8; 4];
        stream
            .read_exact(&mut len_buf)
            .await
            .map_err(map_read_exact_error)?;

        let len = u32::from_le_bytes(len_buf);
        if len > MAX_PAYLOAD_SIZE {
            tracing::error!(
                payload_size = len,
                max = MAX_PAYLOAD_SIZE,
                "QUIC recv: payload exceeds maximum size"
            );
            return Err(TransportError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("payload size {len} exceeds maximum {MAX_PAYLOAD_SIZE}"),
            )));
        }

        // Read the payload.
        let mut payload = vec![0u8; len as usize];
        stream
            .read_exact(&mut payload)
            .await
            .map_err(map_read_exact_error)?;

        Ok((self.remote_id.clone(), payload))
    }

    fn is_connected(&self, peer: &PeerId) -> bool {
        *peer == self.remote_id && self.connection.close_reason().is_none()
    }

    fn transport_type(&self) -> TransportType {
        TransportType::P2p
    }
}

// ---------------------------------------------------------------------------
// Error mapping helpers
// ---------------------------------------------------------------------------

/// Map a quinn `ConnectionError` to the appropriate `TransportError`.
fn map_connection_error(err: quinn::ConnectionError, peer: &PeerId) -> TransportError {
    match err {
        quinn::ConnectionError::TimedOut => TransportError::Timeout,
        quinn::ConnectionError::ConnectionClosed(_)
        | quinn::ConnectionError::ApplicationClosed(_)
        | quinn::ConnectionError::LocallyClosed
        | quinn::ConnectionError::Reset => TransportError::ConnectionClosed,
        quinn::ConnectionError::CidsExhausted
        | quinn::ConnectionError::VersionMismatch
        | quinn::ConnectionError::TransportError(_) => TransportError::Unreachable(peer.clone()),
    }
}

/// Map a quinn `WriteError` to a `TransportError`.
fn map_write_error(err: quinn::WriteError) -> TransportError {
    match err {
        quinn::WriteError::ConnectionLost(_)
        | quinn::WriteError::ClosedStream
        | quinn::WriteError::Stopped(_) => TransportError::ConnectionClosed,
        other => TransportError::Io(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            format!("QUIC write error: {other}"),
        )),
    }
}

/// Map a quinn `ReadExactError` to a `TransportError`.
fn map_read_exact_error(err: quinn::ReadExactError) -> TransportError {
    match err {
        quinn::ReadExactError::FinishedEarly(_) => TransportError::ConnectionClosed,
        quinn::ReadExactError::ReadError(read_err) => match read_err {
            quinn::ReadError::ConnectionLost(_)
            | quinn::ReadError::ClosedStream
            | quinn::ReadError::Reset(_) => TransportError::ConnectionClosed,
            other => TransportError::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                format!("QUIC read error: {other}"),
            )),
        },
    }
}

// ---------------------------------------------------------------------------
// Tests (T-003-08, T-003-09, T-003-10)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a listener and connect to it, returning both transports.
    ///
    /// Must be called from a multi-thread tokio runtime (the accept task
    /// runs on a separate thread while the initiator connects).
    async fn create_connected_pair() -> (QuicTransport, QuicTransport) {
        let listener = QuicListener::bind(
            "127.0.0.1:0".parse().expect("valid addr"),
            PeerId::new("responder"),
        )
        .await
        .expect("listener bind");

        let addr = listener.local_addr().expect("local addr");

        let accept_handle = tokio::spawn(async move { listener.accept().await });

        let initiator = QuicTransport::connect(
            addr,
            PeerId::new("initiator"),
            PeerId::new(addr.to_string()),
        )
        .await
        .expect("connect");

        let responder = accept_handle.await.expect("accept task").expect("accept");

        (initiator, responder)
    }

    // -- T-003-08: Listener tests --

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn listener_bind_succeeds() {
        let listener = QuicListener::bind(
            "127.0.0.1:0".parse().expect("valid addr"),
            PeerId::new("test"),
        )
        .await;
        assert!(listener.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn listener_local_addr_returns_valid_port() {
        let listener = QuicListener::bind(
            "127.0.0.1:0".parse().expect("valid addr"),
            PeerId::new("test"),
        )
        .await
        .expect("bind");

        let addr = listener.local_addr().expect("local addr");
        assert!(
            addr.port() > 0,
            "expected non-zero port, got {}",
            addr.port()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn listener_accept_returns_transport_on_connect() {
        let listener = QuicListener::bind(
            "127.0.0.1:0".parse().expect("valid addr"),
            PeerId::new("responder"),
        )
        .await
        .expect("bind");

        let addr = listener.local_addr().expect("local addr");

        let accept_handle = tokio::spawn(async move { listener.accept().await });

        let _initiator = QuicTransport::connect(
            addr,
            PeerId::new("initiator"),
            PeerId::new(addr.to_string()),
        )
        .await
        .expect("connect");

        let responder = accept_handle.await.expect("task").expect("accept");
        assert_eq!(responder.transport_type(), TransportType::P2p);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn listener_accept_after_close_returns_connection_closed() {
        let listener = QuicListener::bind(
            "127.0.0.1:0".parse().expect("valid addr"),
            PeerId::new("test"),
        )
        .await
        .expect("bind");

        listener.close();

        let result = listener.accept().await;
        assert!(result.is_err(), "accept on closed endpoint should fail");
    }

    // -- T-003-09: Connect error tests --

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn connect_to_listener_succeeds() {
        let (initiator, responder) = create_connected_pair().await;
        assert!(initiator.is_connected(initiator.remote_id()));
        assert!(responder.is_connected(responder.remote_id()));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn connect_timeout_to_unreachable_address() {
        // TEST-NET-1 address that should be unreachable.
        let addr: SocketAddr = "192.0.2.1:1".parse().expect("valid addr");
        let result = QuicTransport::connect_with_timeout(
            addr,
            PeerId::new("initiator"),
            PeerId::new("unreachable"),
            Duration::from_secs(1),
        )
        .await;

        match result {
            Err(TransportError::Timeout) => {}          // expected
            Err(TransportError::Unreachable(_)) => {}   // also acceptable
            Err(TransportError::Io(_)) => {}            // OS may reject immediately
            Err(TransportError::ConnectionClosed) => {} // quinn may report this
            Ok(_) => panic!("expected error, got Ok"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn connect_to_non_listening_port_returns_error() {
        // Bind a listener, get its port, then close it to ensure nothing listens there.
        let listener = QuicListener::bind(
            "127.0.0.1:0".parse().expect("valid addr"),
            PeerId::new("temp"),
        )
        .await
        .expect("bind");
        let addr = listener.local_addr().expect("addr");
        listener.close();
        drop(listener);

        // Give the OS a moment to release the port.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let result = QuicTransport::connect_with_timeout(
            addr,
            PeerId::new("initiator"),
            PeerId::new("nobody"),
            Duration::from_secs(2),
        )
        .await;

        assert!(result.is_err(), "connect to closed port should fail");
    }

    // -- T-003-10: Transport trait unit tests --

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn send_recv_round_trip() {
        let (initiator, responder) = create_connected_pair().await;

        let payload = b"hello over QUIC";
        initiator
            .send(initiator.remote_id(), payload)
            .await
            .expect("send");

        let (from, data) = responder.recv().await.expect("recv");
        assert_eq!(from, *responder.remote_id());
        assert_eq!(data, payload);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn bidirectional_communication() {
        let (initiator, responder) = create_connected_pair().await;

        // Initiator -> Responder
        initiator
            .send(initiator.remote_id(), b"from initiator")
            .await
            .expect("send");
        let (_, data) = responder.recv().await.expect("recv");
        assert_eq!(data, b"from initiator");

        // Responder -> Initiator
        responder
            .send(responder.remote_id(), b"from responder")
            .await
            .expect("send");
        let (_, data) = initiator.recv().await.expect("recv");
        assert_eq!(data, b"from responder");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn multiple_messages_preserve_order() {
        let (initiator, responder) = create_connected_pair().await;

        for i in 0u32..10 {
            initiator
                .send(initiator.remote_id(), &i.to_le_bytes())
                .await
                .expect("send");
        }

        for i in 0u32..10 {
            let (_, data) = responder.recv().await.expect("recv");
            let received = u32::from_le_bytes(data.try_into().expect("4 bytes"));
            assert_eq!(received, i, "message order mismatch at index {i}");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn large_payload_round_trip() {
        let (initiator, responder) = create_connected_pair().await;

        // 32 KB payload
        let payload: Vec<u8> = (0..32_768).map(|i| (i % 256) as u8).collect();
        initiator
            .send(initiator.remote_id(), &payload)
            .await
            .expect("send large");

        let (_, data) = responder.recv().await.expect("recv large");
        assert_eq!(data, payload);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn empty_payload_round_trip() {
        let (initiator, responder) = create_connected_pair().await;

        initiator
            .send(initiator.remote_id(), b"")
            .await
            .expect("send empty");

        let (_, data) = responder.recv().await.expect("recv empty");
        assert!(data.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn transport_type_is_p2p() {
        let (initiator, responder) = create_connected_pair().await;
        assert_eq!(initiator.transport_type(), TransportType::P2p);
        assert_eq!(responder.transport_type(), TransportType::P2p);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn is_connected_true_for_remote_peer() {
        let (initiator, responder) = create_connected_pair().await;
        assert!(initiator.is_connected(initiator.remote_id()));
        assert!(responder.is_connected(responder.remote_id()));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn is_connected_false_for_unknown_peer() {
        let (initiator, _responder) = create_connected_pair().await;
        assert!(!initiator.is_connected(&PeerId::new("unknown")));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn send_to_wrong_peer_returns_unreachable() {
        let (initiator, _responder) = create_connected_pair().await;

        let wrong_peer = PeerId::new("wrong-peer");
        let result = initiator.send(&wrong_peer, b"hi").await;
        assert!(
            matches!(result, Err(TransportError::Unreachable(_))),
            "expected Unreachable error"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn is_connected_false_after_remote_drop() {
        let (initiator, responder) = create_connected_pair().await;
        let remote_id = responder.remote_id().clone();

        // Close the responder's connection.
        responder.connection.close(0u32.into(), b"bye");
        drop(responder);

        // Give QUIC time to propagate the close.
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert!(
            !initiator.is_connected(&remote_id),
            "is_connected should be false after remote drops"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn send_after_remote_close_returns_error() {
        let (initiator, responder) = create_connected_pair().await;
        let remote_id = initiator.remote_id().clone();

        responder.connection.close(0u32.into(), b"bye");
        drop(responder);

        // Give QUIC time to propagate the close.
        tokio::time::sleep(Duration::from_millis(100)).await;

        let result = initiator.send(&remote_id, b"hello?").await;
        assert!(result.is_err(), "send after remote close should fail");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn recv_after_remote_close_returns_error() {
        let (initiator, responder) = create_connected_pair().await;

        responder.connection.close(0u32.into(), b"bye");
        drop(responder);

        let result = initiator.recv().await;
        assert!(result.is_err(), "recv after remote close should fail");
    }

    // -- T-003-10 reviewer: postcondition gap-filling tests --

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn max_payload_boundary_round_trip() {
        // Success Postcondition 2: both peers can send/recv opaque byte payloads.
        // Tests at exactly MAX_PAYLOAD_SIZE (64 KB), the boundary.
        let (initiator, responder) = create_connected_pair().await;

        let payload: Vec<u8> = (0..MAX_PAYLOAD_SIZE as usize)
            .map(|i| (i % 256) as u8)
            .collect();
        initiator
            .send(initiator.remote_id(), &payload)
            .await
            .expect("send max payload");

        let (_, data) = responder.recv().await.expect("recv max payload");
        assert_eq!(data.len(), MAX_PAYLOAD_SIZE as usize);
        assert_eq!(data, payload);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn opaque_bytes_invariant() {
        // Invariant 1: transport never inspects or modifies payload bytes.
        // Send a payload containing every possible byte value (0x00..0xFF).
        let (initiator, responder) = create_connected_pair().await;

        let payload: Vec<u8> = (0u16..256).map(|b| b as u8).collect();
        initiator
            .send(initiator.remote_id(), &payload)
            .await
            .expect("send all bytes");

        let (_, data) = responder.recv().await.expect("recv all bytes");
        assert_eq!(data, payload, "transport must not modify payload bytes");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn listener_accepts_multiple_connections_sequentially() {
        // Invariant 4: QUIC endpoint can handle multiple concurrent connections.
        let listener = QuicListener::bind(
            "127.0.0.1:0".parse().expect("valid addr"),
            PeerId::new("server"),
        )
        .await
        .expect("bind");

        let addr = listener.local_addr().expect("local addr");

        // Accept first connection.
        let accept1 = tokio::spawn({
            let addr = addr;
            async move {
                QuicTransport::connect(addr, PeerId::new("client-1"), PeerId::new(addr.to_string()))
                    .await
            }
        });
        let responder1 = listener.accept().await.expect("accept first");
        let client1 = accept1.await.expect("join").expect("connect first");

        // Accept second connection.
        let accept2 = tokio::spawn({
            let addr = addr;
            async move {
                QuicTransport::connect(addr, PeerId::new("client-2"), PeerId::new(addr.to_string()))
                    .await
            }
        });
        let responder2 = listener.accept().await.expect("accept second");
        let client2 = accept2.await.expect("join").expect("connect second");

        // Verify both connections work independently.
        client1
            .send(client1.remote_id(), b"from client 1")
            .await
            .expect("send 1");
        let (_, data1) = responder1.recv().await.expect("recv 1");
        assert_eq!(data1, b"from client 1");

        client2
            .send(client2.remote_id(), b"from client 2")
            .await
            .expect("send 2");
        let (_, data2) = responder2.recv().await.expect("recv 2");
        assert_eq!(data2, b"from client 2");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn local_and_remote_id_accessors() {
        // Verify local_id() and remote_id() return the correct values
        // passed during construction.
        let listener = QuicListener::bind(
            "127.0.0.1:0".parse().expect("valid addr"),
            PeerId::new("responder"),
        )
        .await
        .expect("bind");

        let addr = listener.local_addr().expect("local addr");

        let accept_handle = tokio::spawn(async move { listener.accept().await });

        let remote_id = PeerId::new(addr.to_string());
        let initiator = QuicTransport::connect(addr, PeerId::new("initiator"), remote_id.clone())
            .await
            .expect("connect");

        let responder = accept_handle.await.expect("task").expect("accept");

        // Initiator side: local_id is "initiator", remote_id is the address.
        assert_eq!(initiator.local_id(), &PeerId::new("initiator"));
        assert_eq!(initiator.remote_id(), &remote_id);

        // Responder side: local_id is "responder", remote_id is derived from
        // the connecting peer's address.
        assert_eq!(responder.local_id(), &PeerId::new("responder"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn recv_returns_remote_peer_id_both_directions() {
        // Verify recv() returns the correct sender PeerId in both directions.
        let (initiator, responder) = create_connected_pair().await;

        // Initiator -> Responder: recv should report the remote PeerId.
        initiator
            .send(initiator.remote_id(), b"ping")
            .await
            .expect("send");
        let (from, _) = responder.recv().await.expect("recv");
        assert_eq!(
            from,
            *responder.remote_id(),
            "recv on responder should report initiator as sender"
        );

        // Responder -> Initiator: recv should report the remote PeerId.
        responder
            .send(responder.remote_id(), b"pong")
            .await
            .expect("send");
        let (from, _) = initiator.recv().await.expect("recv");
        assert_eq!(
            from,
            *initiator.remote_id(),
            "recv on initiator should report responder as sender"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn generate_self_signed_cert_produces_valid_cert() {
        // Postcondition 6: connection uses QUIC transport encryption (TLS 1.3).
        // Verify the cert generation succeeds and produces non-empty data.
        let (cert_der, key_der) = generate_self_signed_cert().expect("cert generation");
        assert!(!cert_der.is_empty(), "certificate should be non-empty");
        assert!(
            !key_der.secret_pkcs8_der().is_empty(),
            "key should be non-empty"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn tls_configs_are_constructible() {
        // Verify server and client TLS configs can be built without error.
        let (cert_der, key_der) = generate_self_signed_cert().expect("cert gen");
        let server = make_server_config(cert_der, key_der);
        assert!(server.is_ok(), "server config should succeed");

        let client = make_client_config();
        assert!(client.is_ok(), "client config should succeed");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn connection_error_mapping_timeout() {
        let peer = PeerId::new("test");
        let err = map_connection_error(quinn::ConnectionError::TimedOut, &peer);
        assert!(
            matches!(err, TransportError::Timeout),
            "TimedOut should map to Timeout"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn connection_error_mapping_locally_closed() {
        let peer = PeerId::new("test");
        let err = map_connection_error(quinn::ConnectionError::LocallyClosed, &peer);
        assert!(
            matches!(err, TransportError::ConnectionClosed),
            "LocallyClosed should map to ConnectionClosed"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn connection_error_mapping_reset() {
        let peer = PeerId::new("test");
        let err = map_connection_error(quinn::ConnectionError::Reset, &peer);
        assert!(
            matches!(err, TransportError::ConnectionClosed),
            "Reset should map to ConnectionClosed"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn connection_error_mapping_version_mismatch() {
        let peer = PeerId::new("test");
        let err = map_connection_error(quinn::ConnectionError::VersionMismatch, &peer);
        assert!(
            matches!(err, TransportError::Unreachable(_)),
            "VersionMismatch should map to Unreachable"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn connection_error_mapping_cids_exhausted() {
        let peer = PeerId::new("test");
        let err = map_connection_error(quinn::ConnectionError::CidsExhausted, &peer);
        assert!(
            matches!(err, TransportError::Unreachable(_)),
            "CidsExhausted should map to Unreachable"
        );
    }
}
