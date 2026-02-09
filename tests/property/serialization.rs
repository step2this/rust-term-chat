//! Property-based serialization round-trip tests (T-001-18).
//!
//! Uses proptest to verify:
//! 1. Any valid `ChatMessage` survives encode -> decode round-trip.
//! 2. Any valid `Envelope` survives encode -> decode round-trip.
//! 3. Random bytes never cause a panic in `decode` (returns `Err` gracefully).
//! 4. Framed encode -> decode round-trips correctly for any valid envelope.
//! 5. Any valid `Task` and `TaskSyncMessage` survive encode -> decode round-trip.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use proptest::prelude::*;
use termchat_proto::agent::{AgentCapability, AgentInfo};
use termchat_proto::codec;
use termchat_proto::message::*;
use termchat_proto::presence::{PresenceMessage, PresenceStatus};
use termchat_proto::relay::{self, RelayMessage};
use termchat_proto::task::{
    self, LwwRegister, Task, TaskFieldUpdate, TaskId, TaskStatus, TaskSyncMessage,
};
use termchat_proto::typing::TypingMessage;
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

// ==========================================================================
// Task sync protocol property tests (UC-008)
// ==========================================================================

/// Strategy for generating arbitrary `TaskId` values.
fn arb_task_id() -> impl Strategy<Value = TaskId> {
    any::<u128>().prop_map(|n| TaskId::from_uuid(Uuid::from_u128(n)))
}

/// Strategy for generating arbitrary `TaskStatus` values.
fn arb_task_status() -> impl Strategy<Value = TaskStatus> {
    prop_oneof![
        Just(TaskStatus::Open),
        Just(TaskStatus::InProgress),
        Just(TaskStatus::Completed),
        Just(TaskStatus::Deleted),
    ]
}

/// Strategy for generating arbitrary LWW registers with string values.
fn arb_lww_string() -> impl Strategy<Value = LwwRegister<String>> {
    ("[^\x00]{0,128}", any::<u64>(), "[a-z]{1,16}")
        .prop_map(|(value, timestamp, author)| LwwRegister::new(value, timestamp, author))
}

/// Strategy for generating arbitrary LWW registers with `TaskStatus` values.
fn arb_lww_status() -> impl Strategy<Value = LwwRegister<TaskStatus>> {
    (arb_task_status(), any::<u64>(), "[a-z]{1,16}")
        .prop_map(|(value, timestamp, author)| LwwRegister::new(value, timestamp, author))
}

/// Strategy for generating arbitrary LWW registers with optional string values.
fn arb_lww_assignee() -> impl Strategy<Value = LwwRegister<Option<String>>> {
    (
        prop::option::of("[a-z]{1,16}".prop_map(String::from)),
        any::<u64>(),
        "[a-z]{1,16}",
    )
        .prop_map(|(value, timestamp, author)| LwwRegister::new(value, timestamp, author))
}

/// Strategy for generating arbitrary `Task` values.
fn arb_task() -> impl Strategy<Value = Task> {
    (
        arb_task_id(),
        "[a-z]{1,32}",
        arb_lww_string(),
        arb_lww_status(),
        arb_lww_assignee(),
        any::<u64>(),
        "[a-z]{1,16}",
    )
        .prop_map(
            |(id, room_id, title, status, assignee, created_at, created_by)| Task {
                id,
                room_id,
                title,
                status,
                assignee,
                created_at,
                created_by,
            },
        )
}

/// Strategy for generating arbitrary `TaskFieldUpdate` values.
fn arb_task_field_update() -> impl Strategy<Value = TaskFieldUpdate> {
    prop_oneof![
        arb_lww_string().prop_map(TaskFieldUpdate::Title),
        arb_lww_status().prop_map(TaskFieldUpdate::Status),
        arb_lww_assignee().prop_map(TaskFieldUpdate::Assignee),
    ]
}

/// Strategy for generating arbitrary `TaskSyncMessage` values.
fn arb_task_sync_message() -> impl Strategy<Value = TaskSyncMessage> {
    prop_oneof![
        (arb_task_id(), "[a-z]{1,32}", arb_task_field_update()).prop_map(
            |(task_id, room_id, field)| TaskSyncMessage::FieldUpdate {
                task_id,
                room_id,
                field,
            }
        ),
        ("[a-z]{1,32}", prop::collection::vec(arb_task(), 0..8))
            .prop_map(|(room_id, tasks)| TaskSyncMessage::FullState { room_id, tasks }),
        "[a-z]{1,32}".prop_map(|room_id| TaskSyncMessage::RequestFullState { room_id }),
    ]
}

proptest! {
    /// Any valid `Task` survives a postcard encode -> decode round-trip.
    #[test]
    fn task_postcard_round_trip(task in arb_task()) {
        let bytes = postcard::to_allocvec(&task).expect("encode should succeed");
        let decoded: Task = postcard::from_bytes(&bytes).expect("decode should succeed");
        prop_assert_eq!(task, decoded);
    }

    /// Any valid `TaskSyncMessage` survives encode -> decode round-trip
    /// through the task module's encode/decode functions.
    #[test]
    fn task_sync_message_round_trip(msg in arb_task_sync_message()) {
        let bytes = task::encode(&msg).expect("encode should succeed");
        let decoded = task::decode(&bytes).expect("decode should succeed");
        prop_assert_eq!(msg, decoded);
    }

    /// Random bytes never cause a panic when decoded as a `TaskSyncMessage`.
    #[test]
    fn random_bytes_task_decode_no_panic(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        let _ = task::decode(&bytes);
    }
}

// ==========================================================================
// Agent, Presence, Typing, and Relay property tests
// ==========================================================================

/// Strategy for generating arbitrary `AgentCapability` values.
fn arb_agent_capability() -> impl Strategy<Value = AgentCapability> {
    prop_oneof![
        Just(AgentCapability::Chat),
        Just(AgentCapability::TaskManagement),
        Just(AgentCapability::CodeReview),
    ]
}

/// Strategy for generating arbitrary `AgentInfo` values.
fn arb_agent_info() -> impl Strategy<Value = AgentInfo> {
    (
        "[a-z]{1,16}",
        "[a-z]{1,16}",
        prop::collection::vec(arb_agent_capability(), 0..4),
    )
        .prop_map(|(agent_id, display_name, capabilities)| AgentInfo {
            agent_id,
            display_name,
            capabilities,
        })
}

/// Strategy for generating arbitrary `PresenceStatus` values.
fn arb_presence_status() -> impl Strategy<Value = PresenceStatus> {
    prop_oneof![
        Just(PresenceStatus::Online),
        Just(PresenceStatus::Away),
        Just(PresenceStatus::Offline),
    ]
}

/// Strategy for generating arbitrary `PresenceMessage` values.
fn arb_presence_message() -> impl Strategy<Value = PresenceMessage> {
    ("[a-z]{1,16}", arb_presence_status(), any::<u64>()).prop_map(|(peer_id, status, timestamp)| {
        PresenceMessage {
            peer_id,
            status,
            timestamp,
        }
    })
}

/// Strategy for generating arbitrary `TypingMessage` values.
fn arb_typing_message() -> impl Strategy<Value = TypingMessage> {
    ("[a-z]{1,16}", "[a-z]{1,16}", any::<bool>()).prop_map(|(peer_id, room_id, is_typing)| {
        TypingMessage {
            peer_id,
            room_id,
            is_typing,
        }
    })
}

/// Strategy for generating arbitrary `RelayMessage` values.
fn arb_relay_message() -> impl Strategy<Value = RelayMessage> {
    prop_oneof![
        "[a-z]{1,16}".prop_map(|peer_id| RelayMessage::Register { peer_id }),
        "[a-z]{1,16}".prop_map(|peer_id| RelayMessage::Registered { peer_id }),
        (
            "[a-z]{1,16}",
            "[a-z]{1,16}",
            prop::collection::vec(any::<u8>(), 0..256),
        )
            .prop_map(|(from, to, payload)| RelayMessage::RelayPayload {
                from,
                to,
                payload
            }),
        ("[a-z]{1,16}", any::<u32>()).prop_map(|(to, count)| RelayMessage::Queued { to, count }),
        "[a-z]{1,16}".prop_map(|reason| RelayMessage::Error { reason }),
        prop::collection::vec(any::<u8>(), 0..256).prop_map(RelayMessage::Room),
    ]
}

proptest! {
    /// Any valid `AgentInfo` survives a postcard encode -> decode round-trip.
    #[test]
    fn agent_info_postcard_round_trip(info in arb_agent_info()) {
        let bytes = postcard::to_allocvec(&info).expect("encode should succeed");
        let decoded: AgentInfo = postcard::from_bytes(&bytes).expect("decode should succeed");
        prop_assert_eq!(info, decoded);
    }

    /// Any valid `AgentCapability` survives a postcard encode -> decode round-trip.
    #[test]
    fn agent_capability_postcard_round_trip(cap in arb_agent_capability()) {
        let bytes = postcard::to_allocvec(&cap).expect("encode should succeed");
        let decoded: AgentCapability = postcard::from_bytes(&bytes).expect("decode should succeed");
        prop_assert_eq!(cap, decoded);
    }

    /// Any valid `PresenceStatus` survives a postcard encode -> decode round-trip.
    #[test]
    fn presence_status_postcard_round_trip(status in arb_presence_status()) {
        let bytes = postcard::to_allocvec(&status).expect("encode should succeed");
        let decoded: PresenceStatus = postcard::from_bytes(&bytes).expect("decode should succeed");
        prop_assert_eq!(status, decoded);
    }

    /// Any valid `PresenceMessage` survives a postcard encode -> decode round-trip.
    #[test]
    fn presence_message_postcard_round_trip(msg in arb_presence_message()) {
        let bytes = postcard::to_allocvec(&msg).expect("encode should succeed");
        let decoded: PresenceMessage = postcard::from_bytes(&bytes).expect("decode should succeed");
        prop_assert_eq!(msg, decoded);
    }

    /// Any valid `TypingMessage` survives a postcard encode -> decode round-trip.
    #[test]
    fn typing_message_postcard_round_trip(msg in arb_typing_message()) {
        let bytes = postcard::to_allocvec(&msg).expect("encode should succeed");
        let decoded: TypingMessage = postcard::from_bytes(&bytes).expect("decode should succeed");
        prop_assert_eq!(msg, decoded);
    }

    /// Any valid `RelayMessage` survives an encode -> decode round-trip
    /// through the relay module's encode/decode functions.
    #[test]
    fn relay_message_round_trip(msg in arb_relay_message()) {
        let bytes = relay::encode(&msg).expect("encode should succeed");
        let decoded = relay::decode(&bytes).expect("decode should succeed");
        prop_assert_eq!(msg, decoded);
    }

    /// Random bytes never cause a panic when decoded as a `RelayMessage`.
    #[test]
    fn random_bytes_relay_decode_no_panic(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        let _ = relay::decode(&bytes);
    }

    /// Random bytes never cause a panic when decoded as a `PresenceMessage`.
    #[test]
    fn random_bytes_presence_decode_no_panic(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        let _ = postcard::from_bytes::<PresenceMessage>(&bytes);
    }
}
