//! Wire format message types for the `TermChat` protocol.
//!
//! All types in this module represent the on-the-wire format for messages
//! exchanged between `TermChat` peers. They are designed to be serialized with
//! postcard and encrypted before transmission.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Maximum allowed message payload size in bytes (64 KB).
pub const MAX_MESSAGE_SIZE: usize = 64 * 1024;

/// Unique identifier for a message, based on UUID v7 for time-ordering.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(Uuid);

impl MessageId {
    /// Creates a new time-ordered message identifier (UUID v7).
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Creates a `MessageId` from an existing UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the inner UUID value.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifies a message sender by their public key fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SenderId(Vec<u8>);

impl SenderId {
    /// Creates a new sender identity from a public key fingerprint.
    #[must_use]
    pub const fn new(fingerprint: Vec<u8>) -> Self {
        Self(fingerprint)
    }

    /// Returns the raw fingerprint bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Display for SenderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Identifies a message recipient by their public key fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecipientId(Vec<u8>);

impl RecipientId {
    /// Creates a new recipient identity from a public key fingerprint.
    #[must_use]
    pub const fn new(fingerprint: Vec<u8>) -> Self {
        Self(fingerprint)
    }

    /// Returns the raw fingerprint bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Display for RecipientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Identifies a conversation (direct message thread or room).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConversationId(Uuid);

impl ConversationId {
    /// Creates a new conversation identifier (UUID v7).
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Creates a `ConversationId` from an existing UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the inner UUID value.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for ConversationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ConversationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Millisecond-precision UTC timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Timestamp(u64);

impl Timestamp {
    /// Creates a timestamp for the current instant.
    #[must_use]
    pub fn now() -> Self {
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        Self(u64::try_from(millis).unwrap_or(u64::MAX))
    }

    /// Creates a timestamp from milliseconds since the UNIX epoch.
    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        Self(millis)
    }

    /// Returns the timestamp as milliseconds since the UNIX epoch.
    #[must_use]
    pub const fn as_millis(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}ms", self.0)
    }
}

/// Content of a chat message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageContent {
    /// Plain text message content.
    Text(String),
    // Future variants: File, Reaction, etc.
}

/// Metadata attached to every chat message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// Unique identifier for this message.
    pub message_id: MessageId,
    /// When the message was created.
    pub timestamp: Timestamp,
    /// Who sent this message.
    pub sender_id: SenderId,
    /// Which conversation this message belongs to.
    pub conversation_id: ConversationId,
}

/// A complete chat message with metadata and content, ready for serialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Message metadata (id, timestamp, sender, conversation).
    pub metadata: MessageMetadata,
    /// The message content (text, etc.).
    pub content: MessageContent,
}

/// Error returned when a message fails validation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    /// Message content is empty.
    #[error("message content is empty")]
    Empty,
    /// Message content exceeds the maximum allowed size.
    #[error("message too large ({size} bytes, max {max} bytes)")]
    TooLarge {
        /// Actual size of the content in bytes.
        size: usize,
        /// Maximum allowed size in bytes.
        max: usize,
    },
}

impl ChatMessage {
    /// Validates this message for sending.
    ///
    /// Checks that the content is non-empty and within the size limit
    /// ([`MAX_MESSAGE_SIZE`] = 64 KB).
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::Empty`] if the message text is empty, or
    /// [`ValidationError::TooLarge`] if it exceeds `MAX_MESSAGE_SIZE`.
    pub const fn validate(&self) -> Result<(), ValidationError> {
        match &self.content {
            MessageContent::Text(text) => {
                if text.is_empty() {
                    return Err(ValidationError::Empty);
                }
                let size = text.len();
                if size > MAX_MESSAGE_SIZE {
                    return Err(ValidationError::TooLarge {
                        size,
                        max: MAX_MESSAGE_SIZE,
                    });
                }
            }
        }
        Ok(())
    }
}

/// Tracks the delivery lifecycle of a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageStatus {
    /// Message created but not yet sent.
    Pending,
    /// Message transmitted, awaiting delivery confirmation.
    Sent,
    /// Delivery confirmed by recipient.
    Delivered,
    /// Delivery failed with a reason.
    Failed(String),
}

/// Acknowledgment that a message was received by the recipient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryAck {
    /// The ID of the message being acknowledged.
    pub message_id: MessageId,
    /// When the acknowledgment was created.
    pub timestamp: Timestamp,
}

/// Negative acknowledgment indicating message processing failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nack {
    /// The ID of the message being negatively acknowledged.
    pub message_id: MessageId,
    /// Reason for the NACK.
    pub reason: NackReason,
}

/// Reason for a negative acknowledgment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NackReason {
    /// Deserialization failed (unknown format or version).
    DeserializationFailed,
    /// Sender ID in metadata does not match authenticated peer.
    SenderIdMismatch,
    /// Other reason (free-form string).
    Other(String),
}

/// Top-level envelope wrapping all wire-level protocol messages.
///
/// Every message on the wire is wrapped in an `Envelope`, which allows
/// the receiver to determine the message type before further processing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Envelope {
    /// A chat message from one peer to another.
    Chat(ChatMessage),
    /// A delivery acknowledgment.
    Ack(DeliveryAck),
    /// A negative acknowledgment indicating processing failure.
    Nack(Nack),
    /// A handshake message (opaque bytes, interpreted by the crypto layer).
    Handshake(Vec<u8>),
    /// A presence status update (opaque bytes, decoded by the application layer).
    PresenceUpdate(Vec<u8>),
    /// A typing indicator (opaque bytes, decoded by the application layer).
    TypingIndicator(Vec<u8>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_id_display_is_uuid() {
        let id = MessageId::new();
        let display = id.to_string();
        // UUID v7 format: 8-4-4-4-12 hex chars
        assert_eq!(display.len(), 36);
        assert!(display.contains('-'));
    }

    #[test]
    fn timestamp_round_trips_millis() {
        let ts = Timestamp::from_millis(1_700_000_000_000);
        assert_eq!(ts.as_millis(), 1_700_000_000_000);
    }

    #[test]
    fn timestamp_now_is_reasonable() {
        let ts = Timestamp::now();
        // Should be after 2020-01-01 and before 2100-01-01
        assert!(ts.as_millis() > 1_577_836_800_000);
        assert!(ts.as_millis() < 4_102_444_800_000);
    }

    #[test]
    fn sender_id_display_is_hex() {
        let id = SenderId::new(vec![0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(id.to_string(), "deadbeef");
    }

    #[test]
    fn chat_message_construction() {
        let msg = ChatMessage {
            metadata: MessageMetadata {
                message_id: MessageId::new(),
                timestamp: Timestamp::now(),
                sender_id: SenderId::new(vec![1, 2, 3]),
                conversation_id: ConversationId::new(),
            },
            content: MessageContent::Text("hello".into()),
        };

        let MessageContent::Text(ref text) = msg.content;
        assert_eq!(text, "hello");
    }

    #[test]
    fn envelope_chat_variant() {
        let msg = ChatMessage {
            metadata: MessageMetadata {
                message_id: MessageId::new(),
                timestamp: Timestamp::now(),
                sender_id: SenderId::new(vec![1]),
                conversation_id: ConversationId::new(),
            },
            content: MessageContent::Text("test".into()),
        };
        let envelope = Envelope::Chat(msg.clone());

        if let Envelope::Chat(inner) = envelope {
            assert_eq!(inner, msg);
        } else {
            panic!("expected Chat envelope");
        }
    }

    #[test]
    fn envelope_ack_variant() {
        let ack = DeliveryAck {
            message_id: MessageId::new(),
            timestamp: Timestamp::now(),
        };
        let envelope = Envelope::Ack(ack.clone());

        if let Envelope::Ack(inner) = envelope {
            assert_eq!(inner, ack);
        } else {
            panic!("expected Ack envelope");
        }
    }

    #[test]
    fn envelope_handshake_variant() {
        let data = vec![0x01, 0x02, 0x03];
        let envelope = Envelope::Handshake(data.clone());

        if let Envelope::Handshake(inner) = envelope {
            assert_eq!(inner, data);
        } else {
            panic!("expected Handshake envelope");
        }
    }

    #[test]
    fn envelope_presence_update_variant() {
        let data = vec![0x10, 0x20];
        let envelope = Envelope::PresenceUpdate(data.clone());

        if let Envelope::PresenceUpdate(inner) = envelope {
            assert_eq!(inner, data);
        } else {
            panic!("expected PresenceUpdate envelope");
        }
    }

    #[test]
    fn envelope_typing_indicator_variant() {
        let data = vec![0x30, 0x40];
        let envelope = Envelope::TypingIndicator(data.clone());

        if let Envelope::TypingIndicator(inner) = envelope {
            assert_eq!(inner, data);
        } else {
            panic!("expected TypingIndicator envelope");
        }
    }

    // --- Validation tests (T-001-14) ---

    /// Helper to create a `ChatMessage` with the given text content.
    fn make_message(text: &str) -> ChatMessage {
        ChatMessage {
            metadata: MessageMetadata {
                message_id: MessageId::new(),
                timestamp: Timestamp::now(),
                sender_id: SenderId::new(vec![1, 2, 3]),
                conversation_id: ConversationId::new(),
            },
            content: MessageContent::Text(text.to_string()),
        }
    }

    #[test]
    fn validate_empty_message_returns_error() {
        let msg = make_message("");
        let result = msg.validate();
        assert_eq!(result, Err(ValidationError::Empty));
    }

    #[test]
    fn validate_normal_message_ok() {
        let msg = make_message("hello, world!");
        assert!(msg.validate().is_ok());
    }

    #[test]
    fn validate_multiline_message_ok() {
        let msg = make_message("line one\nline two\nline three");
        assert!(msg.validate().is_ok());
    }

    #[test]
    fn validate_exactly_at_size_limit_ok() {
        let text = "a".repeat(MAX_MESSAGE_SIZE);
        let msg = make_message(&text);
        assert!(msg.validate().is_ok());
    }

    #[test]
    fn validate_one_byte_over_limit_returns_error() {
        let text = "a".repeat(MAX_MESSAGE_SIZE + 1);
        let msg = make_message(&text);
        let result = msg.validate();
        assert_eq!(
            result,
            Err(ValidationError::TooLarge {
                size: MAX_MESSAGE_SIZE + 1,
                max: MAX_MESSAGE_SIZE,
            })
        );
    }

    #[test]
    fn message_status_variants() {
        let pending = MessageStatus::Pending;
        let sent = MessageStatus::Sent;
        let delivered = MessageStatus::Delivered;
        let failed = MessageStatus::Failed("timeout".into());

        assert_eq!(pending, MessageStatus::Pending);
        assert_eq!(sent, MessageStatus::Sent);
        assert_eq!(delivered, MessageStatus::Delivered);
        assert_eq!(failed, MessageStatus::Failed("timeout".into()));
    }
}
