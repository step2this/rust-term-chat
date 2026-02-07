//! Transport layer abstraction for `TermChat`.
//!
//! Defines the [`Transport`] trait that all transport implementations must satisfy.
//! Concrete implementations include:
//! - [`loopback::LoopbackTransport`] — in-process channel-based transport for testing
//! - [`quic::QuicTransport`] — QUIC-based P2P transport (UC-003)
//! - [`relay::RelayTransport`] — WebSocket relay fallback (UC-004)

pub mod hybrid;
pub mod loopback;
pub mod quic;
pub mod relay;

use std::fmt;

/// Unique identifier for a peer in the network.
///
/// Wraps a public key fingerprint (or equivalent identifier).
/// In the final system this will be derived from the peer's
/// Noise static public key. For now it is an opaque string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PeerId(String);

impl PeerId {
    /// Create a new peer identifier from a string representation.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Return the string representation of this peer ID.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Describes which kind of transport is in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    /// Direct peer-to-peer via QUIC (UC-003).
    P2p,
    /// Relay server via WebSocket (UC-004).
    Relay,
    /// In-process loopback for testing.
    Loopback,
}

impl fmt::Display for TransportType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::P2p => write!(f, "P2P"),
            Self::Relay => write!(f, "Relay"),
            Self::Loopback => write!(f, "Loopback"),
        }
    }
}

/// Errors that can occur during transport operations.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// The connection to the peer has been closed.
    #[error("connection closed")]
    ConnectionClosed,

    /// The operation timed out before completing.
    #[error("transport operation timed out")]
    Timeout,

    /// The specified peer is not reachable via this transport.
    #[error("peer {0} is unreachable")]
    Unreachable(PeerId),

    /// An underlying I/O error occurred.
    #[error("transport I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Async transport trait for sending and receiving encrypted payloads.
///
/// Implementations carry opaque byte slices between peers. The transport
/// layer never inspects or modifies the payload — encryption and
/// serialization happen at higher layers.
///
/// # Invariant
///
/// Payloads passed to [`Transport::send`] MUST already be encrypted.
/// The transport treats them as opaque bytes and never attempts to
/// interpret their contents.
pub trait Transport: Send + Sync {
    /// Send an encrypted payload to the specified peer.
    ///
    /// Returns `Ok(())` when the payload has been handed off to the
    /// underlying transport. This does NOT guarantee delivery — the
    /// caller must wait for an application-level acknowledgment.
    fn send(
        &self,
        peer: &PeerId,
        payload: &[u8],
    ) -> impl std::future::Future<Output = Result<(), TransportError>> + Send;

    /// Receive the next encrypted payload from any connected peer.
    ///
    /// Blocks asynchronously until a message arrives. Returns the
    /// sender's [`PeerId`] and the raw (encrypted) bytes.
    fn recv(
        &self,
    ) -> impl std::future::Future<Output = Result<(PeerId, Vec<u8>), TransportError>> + Send;

    /// Check whether this transport currently has a connection to the given peer.
    fn is_connected(&self, peer: &PeerId) -> bool;

    /// Return the type of this transport.
    fn transport_type(&self) -> TransportType;
}
