//! Serialization and deserialization for the `TermChat` wire protocol.
//!
//! Provides encode/decode functions using postcard, along with
//! length-prefix framing variants for stream-based transports.

use crate::message::Envelope;

/// Error type for codec encode/decode operations.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    /// Serialization or deserialization failed.
    #[error("serialization error: {0}")]
    Serialization(String),
    /// Frame is incomplete or has an invalid length prefix.
    #[error("invalid frame: {0}")]
    InvalidFrame(String),
}

/// Encodes an [`Envelope`] into a byte vector using postcard.
///
/// # Errors
///
/// Returns `CodecError::Serialization` if the envelope cannot be serialized.
pub fn encode(envelope: &Envelope) -> Result<Vec<u8>, CodecError> {
    postcard::to_allocvec(envelope).map_err(|e| CodecError::Serialization(e.to_string()))
}

/// Decodes an [`Envelope`] from a byte slice using postcard.
///
/// # Errors
///
/// Returns `CodecError::Serialization` if the bytes cannot be deserialized.
pub fn decode(bytes: &[u8]) -> Result<Envelope, CodecError> {
    postcard::from_bytes(bytes).map_err(|e| CodecError::Serialization(e.to_string()))
}

/// Encodes an [`Envelope`] with a 4-byte little-endian length prefix.
///
/// Wire format: `[u32 length (LE)][payload bytes]`
///
/// This is suitable for stream-based transports (TCP, WebSocket) where
/// message boundaries are not preserved by the transport layer.
///
/// # Errors
///
/// Returns `CodecError::Serialization` if the envelope cannot be serialized,
/// or `CodecError::InvalidFrame` if the payload exceeds `u32::MAX` bytes.
pub fn encode_framed(envelope: &Envelope) -> Result<Vec<u8>, CodecError> {
    let payload = encode(envelope)?;
    let len = u32::try_from(payload.len()).map_err(|_| {
        CodecError::InvalidFrame(format!(
            "payload too large for framing: {} bytes",
            payload.len()
        ))
    })?;
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

/// Decodes a length-prefixed frame back into an [`Envelope`].
///
/// Expects the wire format: `[u32 length (LE)][payload bytes]`
///
/// Returns the decoded envelope and the total number of bytes consumed
/// from the input (including the 4-byte length prefix).
///
/// # Errors
///
/// Returns `CodecError::InvalidFrame` if the input is too short or the
/// length prefix indicates more data than available, or
/// `CodecError::Serialization` if the payload cannot be deserialized.
pub fn decode_framed(bytes: &[u8]) -> Result<(Envelope, usize), CodecError> {
    if bytes.len() < 4 {
        return Err(CodecError::InvalidFrame(format!(
            "need at least 4 bytes for length prefix, got {}",
            bytes.len()
        )));
    }
    let len_bytes: [u8; 4] = bytes[..4]
        .try_into()
        .map_err(|_| CodecError::InvalidFrame("failed to read length prefix".into()))?;
    let payload_len = u32::from_le_bytes(len_bytes) as usize;

    let total_len = 4 + payload_len;
    if bytes.len() < total_len {
        return Err(CodecError::InvalidFrame(format!(
            "frame indicates {} bytes but only {} available",
            payload_len,
            bytes.len() - 4
        )));
    }

    let envelope = decode(&bytes[4..total_len])?;
    Ok((envelope, total_len))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::*;

    /// Helper to create a test envelope with a chat message.
    fn make_chat_envelope(text: &str) -> Envelope {
        Envelope::Chat(ChatMessage {
            metadata: MessageMetadata {
                message_id: MessageId::new(),
                timestamp: Timestamp::now(),
                sender_id: SenderId::new(vec![0xaa, 0xbb]),
                conversation_id: ConversationId::new(),
            },
            content: MessageContent::Text(text.to_string()),
        })
    }

    #[test]
    fn encode_decode_round_trip_chat() {
        let original = make_chat_envelope("hello, world!");
        let bytes = encode(&original).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn encode_decode_round_trip_ack() {
        let original = Envelope::Ack(DeliveryAck {
            message_id: MessageId::new(),
            timestamp: Timestamp::now(),
        });
        let bytes = encode(&original).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn encode_decode_round_trip_handshake() {
        let original = Envelope::Handshake(vec![0x01, 0x02, 0x03, 0x04]);
        let bytes = encode(&original).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn framed_encode_decode_round_trip() {
        let original = make_chat_envelope("framed message");
        let frame = encode_framed(&original).unwrap();

        // First 4 bytes are the length prefix
        let payload_len = u32::from_le_bytes(frame[..4].try_into().unwrap()) as usize;
        assert_eq!(payload_len, frame.len() - 4);

        let (decoded, consumed) = decode_framed(&frame).unwrap();
        assert_eq!(original, decoded);
        assert_eq!(consumed, frame.len());
    }

    #[test]
    fn decode_corrupted_bytes_returns_error() {
        let garbage = vec![0xff, 0xfe, 0xfd, 0xfc, 0xfb];
        let result = decode(&garbage);
        assert!(result.is_err());
    }

    #[test]
    fn decode_truncated_bytes_returns_error() {
        let original = make_chat_envelope("truncation test");
        let bytes = encode(&original).unwrap();
        // Take only the first half
        let truncated = &bytes[..bytes.len() / 2];
        let result = decode(truncated);
        assert!(result.is_err());
    }

    #[test]
    fn decode_empty_bytes_returns_error() {
        let result = decode(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn decode_framed_too_short_returns_error() {
        // Less than 4 bytes for the length prefix
        let result = decode_framed(&[0x01, 0x02]);
        assert!(result.is_err());
    }

    #[test]
    fn decode_framed_incomplete_payload_returns_error() {
        // Length prefix says 100 bytes but we only have 2
        let mut frame = Vec::new();
        frame.extend_from_slice(&100u32.to_le_bytes());
        frame.extend_from_slice(&[0x01, 0x02]);
        let result = decode_framed(&frame);
        assert!(result.is_err());
    }

    #[test]
    fn framed_multiple_messages_in_buffer() {
        let msg1 = make_chat_envelope("first");
        let msg2 = make_chat_envelope("second");

        let mut buffer = encode_framed(&msg1).unwrap();
        buffer.extend_from_slice(&encode_framed(&msg2).unwrap());

        let (decoded1, consumed1) = decode_framed(&buffer).unwrap();
        assert_eq!(msg1, decoded1);

        let (decoded2, consumed2) = decode_framed(&buffer[consumed1..]).unwrap();
        assert_eq!(msg2, decoded2);
        assert_eq!(consumed1 + consumed2, buffer.len());
    }
}
