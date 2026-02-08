//! Typing indicator types for real-time keystroke status.

use serde::{Deserialize, Serialize};

/// A typing indicator message sent between peers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypingMessage {
    /// The peer who is typing (or stopped typing).
    pub peer_id: String,
    /// The room where typing is occurring.
    pub room_id: String,
    /// Whether the peer is currently typing (`true`) or stopped (`false`).
    pub is_typing: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_message_round_trip() {
        let msg = TypingMessage {
            peer_id: "alice".into(),
            room_id: "general".into(),
            is_typing: true,
        };
        let bytes = postcard::to_allocvec(&msg).unwrap();
        let decoded: TypingMessage = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn typing_stopped_round_trip() {
        let msg = TypingMessage {
            peer_id: "bob".into(),
            room_id: "dev".into(),
            is_typing: false,
        };
        let bytes = postcard::to_allocvec(&msg).unwrap();
        let decoded: TypingMessage = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }
}
