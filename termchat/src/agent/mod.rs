//! Agent bridge for AI participant integration.
//!
//! This module enables external AI agents (e.g., Claude Code) to join
//! `TermChat` rooms as participants via a Unix domain socket bridge using
//! a JSON lines protocol.
//!
//! # Submodules
//!
//! - [`bridge`]: Unix socket listener and connection management
//! - [`protocol`]: JSON lines wire format types (`AgentMessage`, `BridgeMessage`)
//! - [`participant`]: Room-level adapter (fan-out send, receive forwarding)

pub mod bridge;
pub mod participant;
pub mod protocol;

/// Errors that can occur during agent bridge operations.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// Failed to create or bind the Unix socket.
    #[error("socket creation failed: {0}")]
    SocketCreationFailed(#[from] std::io::Error),

    /// The agent connection was closed unexpectedly.
    #[error("agent connection closed")]
    ConnectionClosed,

    /// Received an invalid or malformed message from the agent.
    #[error("invalid message: {0}")]
    InvalidMessage(String),

    /// The agent ID is empty or contains only invalid characters.
    #[error("invalid agent ID: {0}")]
    InvalidAgentId(String),

    /// A protocol-level error (wrong version, unexpected message, etc.).
    #[error("protocol error: {0}")]
    ProtocolError(String),

    /// The requested room does not exist.
    #[error("room not found: {0}")]
    RoomNotFound(String),

    /// The room has reached its maximum member capacity.
    #[error("room is full")]
    RoomFull,

    /// The connection attempt timed out.
    #[error("connection timed out")]
    Timeout,

    /// An agent is already connected to this bridge.
    #[error("agent already connected")]
    AlreadyConnected,

    /// JSON serialization or deserialization failed.
    #[error("json error: {0}")]
    JsonError(#[from] serde_json::Error),
}
