//! Relay wire protocol types for the TermChat relay server.
//!
//! Defines the [`RelayMessage`] enum that is bincode-encoded and sent
//! over WebSocket binary frames between relay clients and the relay server.

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

/// Messages exchanged between relay clients and the relay server.
///
/// The relay protocol is simple: clients register with a PeerId, then
/// send/receive encrypted payloads routed by PeerId. The relay never
/// inspects payload contents — it only reads routing metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub enum RelayMessage {
    /// Client registers its PeerId with the relay server.
    ///
    /// Must be the first message sent after WebSocket connection.
    /// Server responds with [`RelayMessage::Registered`] on success.
    Register {
        /// The PeerId of the registering client.
        peer_id: String,
    },

    /// Server acknowledges successful registration.
    Registered {
        /// The PeerId that was registered (echoed back for confirmation).
        peer_id: String,
    },

    /// An encrypted payload to be relayed from one peer to another.
    ///
    /// The `from` field is overwritten by the relay server with the
    /// sender's registered PeerId (server-side enforcement against spoofing).
    RelayPayload {
        /// Sender's PeerId (server overwrites this with the registered PeerId).
        from: String,
        /// Recipient's PeerId (used by server for routing).
        to: String,
        /// Opaque encrypted payload bytes.
        payload: Vec<u8>,
    },

    /// Server acknowledges that a message was queued for an offline recipient.
    Queued {
        /// The PeerId of the offline recipient.
        to: String,
        /// Number of messages currently queued for this recipient.
        count: u32,
    },

    /// Server reports an error condition.
    Error {
        /// Human-readable error description.
        reason: String,
    },
}

/// Encodes a [`RelayMessage`] into bytes using bincode.
pub fn encode(msg: &RelayMessage) -> Result<Vec<u8>, String> {
    bincode::encode_to_vec(msg, bincode::config::standard())
        .map_err(|e| format!("relay encode error: {e}"))
}

/// Decodes a [`RelayMessage`] from bytes using bincode.
pub fn decode(bytes: &[u8]) -> Result<RelayMessage, String> {
    let (msg, _len) =
        bincode::decode_from_slice::<RelayMessage, _>(bytes, bincode::config::standard())
            .map_err(|e| format!("relay decode error: {e}"))?;
    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_register() {
        let msg = RelayMessage::Register {
            peer_id: "peer-abc".to_string(),
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_registered() {
        let msg = RelayMessage::Registered {
            peer_id: "peer-abc".to_string(),
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_relay_payload() {
        let msg = RelayMessage::RelayPayload {
            from: "sender-1".to_string(),
            to: "recipient-2".to_string(),
            payload: vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03],
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_relay_payload_empty() {
        let msg = RelayMessage::RelayPayload {
            from: "a".to_string(),
            to: "b".to_string(),
            payload: vec![],
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_queued() {
        let msg = RelayMessage::Queued {
            to: "peer-xyz".to_string(),
            count: 42,
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn round_trip_error() {
        let msg = RelayMessage::Error {
            reason: "rate limited".to_string(),
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn decode_corrupted_bytes_fails() {
        let result = decode(&[0xFF, 0xFE, 0xFD, 0xFC]);
        assert!(result.is_err());
    }

    #[test]
    fn decode_empty_bytes_fails() {
        let result = decode(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn round_trip_large_payload() {
        let msg = RelayMessage::RelayPayload {
            from: "sender".to_string(),
            to: "recipient".to_string(),
            payload: vec![0xAB; 60_000], // Just under 64KB limit
        };
        let bytes = encode(&msg).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }
}
