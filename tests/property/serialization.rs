//! Property-based serialization round-trip tests (T-001-18).
//!
//! Uses proptest to verify:
//! 1. Any valid `ChatMessage` survives encode → decode round-trip.
//! 2. Any valid `Envelope` survives encode → decode round-trip.
//! 3. Random bytes never cause a panic in `decode` (returns `Err` gracefully).
//! 4. Framed encode → decode round-trips correctly for any valid envelope.

use proptest::prelude::*;
use termchat_proto::codec;
use termchat_proto::message::*;
use uuid::Uuid;

// --- Arbitrary implementations for protocol types ---

/// Strategy for generating arbitrary `MessageId` values.
fn arb_message_id() -> impl Strategy<Value = MessageId> {
    any::<u128>().prop_map(|n| MessageId::from_uuid(Uuid::from_u128(n)))
}

/// Strategy for generating arbitrary `SenderId` values.
fn arb_sender_id() -> impl Strategy<Value = SenderId> {
    prop::collection::vec(any::<u8>(), 0..64).prop_map(SenderId::new)
}

/// Strategy for generating arbitrary `ConversationId` values.
fn arb_conversation_id() -> impl Strategy<Value = ConversationId> {
    any::<u128>().prop_map(|n| ConversationId::from_uuid(Uuid::from_u128(n)))
}

/// Strategy for generating arbitrary `Timestamp` values.
fn arb_timestamp() -> impl Strategy<Value = Timestamp> {
    any::<u64>().prop_map(Timestamp::from_millis)
}

/// Strategy for generating arbitrary `MessageContent` values.
/// Uses non-empty strings to avoid validation failures during round-trip.
fn arb_message_content() -> impl Strategy<Value = MessageContent> {
    "[^\x00]{1,1024}".prop_map(MessageContent::Text)
}

/// Strategy for generating arbitrary `MessageMetadata`.
fn arb_message_metadata() -> impl Strategy<Value = MessageMetadata> {
    (
        arb_message_id(),
        arb_timestamp(),
        arb_sender_id(),
        arb_conversation_id(),
    )
        .prop_map(
            |(message_id, timestamp, sender_id, conversation_id)| MessageMetadata {
                message_id,
                timestamp,
                sender_id,
                conversation_id,
            },
        )
}

/// Strategy for generating arbitrary `ChatMessage` values.
fn arb_chat_message() -> impl Strategy<Value = ChatMessage> {
    (arb_message_metadata(), arb_message_content())
        .prop_map(|(metadata, content)| ChatMessage { metadata, content })
}

/// Strategy for generating arbitrary `DeliveryAck` values.
fn arb_delivery_ack() -> impl Strategy<Value = DeliveryAck> {
    (arb_message_id(), arb_timestamp()).prop_map(|(message_id, timestamp)| DeliveryAck {
        message_id,
        timestamp,
    })
}

/// Strategy for generating arbitrary `MessageStatus` values.
fn arb_message_status() -> impl Strategy<Value = MessageStatus> {
    prop_oneof![
        Just(MessageStatus::Pending),
        Just(MessageStatus::Sent),
        Just(MessageStatus::Delivered),
        ".*".prop_map(MessageStatus::Failed),
    ]
}

/// Strategy for generating arbitrary `Envelope` values.
fn arb_envelope() -> impl Strategy<Value = Envelope> {
    prop_oneof![
        arb_chat_message().prop_map(Envelope::Chat),
        arb_delivery_ack().prop_map(Envelope::Ack),
        prop::collection::vec(any::<u8>(), 0..256).prop_map(Envelope::Handshake),
    ]
}

// --- Property tests ---

proptest! {
    /// Any valid ChatMessage survives an encode → decode round-trip.
    #[test]
    fn chat_message_round_trip(msg in arb_chat_message()) {
        let envelope = Envelope::Chat(msg);
        let bytes = codec::encode(&envelope).expect("encode should succeed");
        let decoded = codec::decode(&bytes).expect("decode should succeed");
        prop_assert_eq!(envelope, decoded);
    }

    /// Any valid DeliveryAck survives an encode → decode round-trip.
    #[test]
    fn delivery_ack_round_trip(ack in arb_delivery_ack()) {
        let envelope = Envelope::Ack(ack);
        let bytes = codec::encode(&envelope).expect("encode should succeed");
        let decoded = codec::decode(&bytes).expect("decode should succeed");
        prop_assert_eq!(envelope, decoded);
    }

    /// Any valid Handshake payload survives an encode → decode round-trip.
    #[test]
    fn handshake_round_trip(data in prop::collection::vec(any::<u8>(), 0..256)) {
        let envelope = Envelope::Handshake(data);
        let bytes = codec::encode(&envelope).expect("encode should succeed");
        let decoded = codec::decode(&bytes).expect("decode should succeed");
        prop_assert_eq!(envelope, decoded);
    }

    /// Any valid Envelope variant survives an encode → decode round-trip.
    #[test]
    fn envelope_round_trip(envelope in arb_envelope()) {
        let bytes = codec::encode(&envelope).expect("encode should succeed");
        let decoded = codec::decode(&bytes).expect("decode should succeed");
        prop_assert_eq!(envelope, decoded);
    }

    /// Any valid Envelope survives a framed encode → decode round-trip.
    #[test]
    fn framed_envelope_round_trip(envelope in arb_envelope()) {
        let frame = codec::encode_framed(&envelope).expect("encode_framed should succeed");
        let (decoded, consumed) = codec::decode_framed(&frame).expect("decode_framed should succeed");
        prop_assert_eq!(&envelope, &decoded);
        prop_assert_eq!(consumed, frame.len());
    }

    /// Random bytes never cause a panic when decoded — they return Err gracefully.
    #[test]
    fn random_bytes_decode_no_panic(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        // We don't care if it returns Ok or Err, just that it doesn't panic.
        let _ = codec::decode(&bytes);
    }

    /// Random bytes never cause a panic when decoded as a framed message.
    #[test]
    fn random_bytes_decode_framed_no_panic(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        let _ = codec::decode_framed(&bytes);
    }

    /// `MessageStatus` survives round-trip through postcard encoding.
    /// (`MessageStatus` is not in `Envelope` directly, but we test it can be
    /// encoded/decoded through postcard independently.)
    #[test]
    fn message_status_postcard_round_trip(status in arb_message_status()) {
        let bytes = postcard::to_allocvec(&status)
            .expect("encode should succeed");
        let decoded: MessageStatus =
            postcard::from_bytes(&bytes)
                .expect("decode should succeed");
        prop_assert_eq!(status, decoded);
    }
}
