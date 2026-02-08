//! Presence status types for peer online/away/offline tracking.

use serde::{Deserialize, Serialize};

/// Presence status of a peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PresenceStatus {
    /// Peer is actively using the client.
    Online,
    /// Peer is idle (no recent input).
    Away,
    /// Peer has disconnected or shut down.
    Offline,
}

impl std::fmt::Display for PresenceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Online => write!(f, "online"),
            Self::Away => write!(f, "away"),
            Self::Offline => write!(f, "offline"),
        }
    }
}

/// A presence update message sent between peers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresenceMessage {
    /// The peer whose presence changed.
    pub peer_id: String,
    /// The new presence status.
    pub status: PresenceStatus,
    /// UTC timestamp in milliseconds when the status changed.
    pub timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_status_display() {
        assert_eq!(PresenceStatus::Online.to_string(), "online");
        assert_eq!(PresenceStatus::Away.to_string(), "away");
        assert_eq!(PresenceStatus::Offline.to_string(), "offline");
    }

    #[test]
    fn presence_message_round_trip() {
        let msg = PresenceMessage {
            peer_id: "alice".into(),
            status: PresenceStatus::Online,
            timestamp: 1_700_000_000_000,
        };
        let bytes = postcard::to_allocvec(&msg).unwrap();
        let decoded: PresenceMessage = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn presence_status_equality() {
        assert_eq!(PresenceStatus::Online, PresenceStatus::Online);
        assert_ne!(PresenceStatus::Online, PresenceStatus::Away);
        assert_ne!(PresenceStatus::Away, PresenceStatus::Offline);
    }
}
