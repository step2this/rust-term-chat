//! Integration tests for UC-001: Send Direct Message (T-001-17).
//!
//! Verifies the five postconditions and two invariants from the use case:
//!
//! **Success Postconditions:**
//! 1. Message is stored in Sender's local history.
//! 2. Message is delivered to Recipient (or queued at relay).
//! 3. Sender sees delivery confirmation (status Delivered).
//!
//! **Invariants:**
//! 1. Plaintext message never leaves the application boundary.
//! 2. Message ordering is preserved per-conversation.

use termchat::chat::history::InMemoryStore;
use termchat::chat::{ChatEvent, ChatManager, RetryConfig, SendError};
use termchat::crypto::CryptoSession;
use termchat::crypto::noise::StubNoiseSession;
use termchat::transport::loopback::LoopbackTransport;
use termchat::transport::{PeerId, Transport};

use termchat_proto::codec;
use termchat_proto::message::{
    ChatMessage, ConversationId, Envelope, MessageContent, MessageId, MessageMetadata,
    MessageStatus, SenderId, Timestamp,
};

use std::time::Duration;
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create Alice (with history) and Bob (without history) connected via loopback.
///
/// Returns (alice_manager, alice_events, alice_warnings, bob_manager, bob_events).
fn create_connected_pair() -> (
    ChatManager<StubNoiseSession, LoopbackTransport, InMemoryStore>,
    mpsc::Receiver<ChatEvent>,
    mpsc::Receiver<termchat::chat::history::HistoryWarning>,
    ChatManager<StubNoiseSession, LoopbackTransport, InMemoryStore>,
    mpsc::Receiver<ChatEvent>,
) {
    let (transport_a, transport_b) =
        LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 64);

    let (alice, alice_events, alice_warnings) = ChatManager::with_history(
        StubNoiseSession::new(true),
        transport_a,
        SenderId::new(vec![0xAA]),
        PeerId::new("bob"),
        64,
        InMemoryStore::new(),
        16,
    );

    let (bob, bob_events) = ChatManager::new(
        StubNoiseSession::new(true),
        transport_b,
        SenderId::new(vec![0xBB]),
        PeerId::new("alice"),
        64,
    );

    (alice, alice_events, alice_warnings, bob, bob_events)
}

/// Create Alice and Bob where both have history enabled.
fn create_pair_both_with_history() -> (
    ChatManager<StubNoiseSession, LoopbackTransport, InMemoryStore>,
    mpsc::Receiver<ChatEvent>,
    mpsc::Receiver<termchat::chat::history::HistoryWarning>,
    ChatManager<StubNoiseSession, LoopbackTransport, InMemoryStore>,
    mpsc::Receiver<ChatEvent>,
    mpsc::Receiver<termchat::chat::history::HistoryWarning>,
) {
    let (transport_a, transport_b) =
        LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 64);

    let (alice, alice_events, alice_warnings) = ChatManager::with_history(
        StubNoiseSession::new(true),
        transport_a,
        SenderId::new(vec![0xAA]),
        PeerId::new("bob"),
        64,
        InMemoryStore::new(),
        16,
    );

    let (bob, bob_events, bob_warnings) = ChatManager::with_history(
        StubNoiseSession::new(true),
        transport_b,
        SenderId::new(vec![0xBB]),
        PeerId::new("alice"),
        64,
        InMemoryStore::new(),
        16,
    );

    (
        alice,
        alice_events,
        alice_warnings,
        bob,
        bob_events,
        bob_warnings,
    )
}

// ===========================================================================
// Postcondition 1+2: Message arrives at recipient, decrypted and intact
// ===========================================================================

/// MSS steps 1-6 + postcondition 2: Alice sends a message; Bob receives it
/// decrypted with matching content.
#[tokio::test]
async fn message_arrives_at_recipient_decrypted_and_intact() {
    let (alice, _alice_events, _warnings, bob, mut bob_events) = create_connected_pair();
    let conversation = ConversationId::new();
    let original_text = "Hello, Bob! This is a direct message.";

    // Alice sends
    alice
        .send_message(
            MessageContent::Text(original_text.to_string()),
            conversation,
        )
        .await
        .expect("send should succeed");

    // Bob receives — the ChatManager decrypts and deserializes internally
    let envelope = bob.receive_one().await.expect("receive should succeed");

    // Verify content matches
    match envelope {
        Envelope::Chat(msg) => {
            let MessageContent::Text(ref text) = msg.content;
            assert_eq!(
                text, original_text,
                "received message content must match what was sent"
            );
        }
        other => panic!("expected Chat envelope, got: {other:?}"),
    }

    // Also verify the UI event was emitted for Bob
    let event = bob_events
        .try_recv()
        .expect("bob should have a MessageReceived event");
    match event {
        ChatEvent::MessageReceived { message, from } => {
            let MessageContent::Text(ref text) = message.content;
            assert_eq!(text, original_text);
            assert_eq!(from, PeerId::new("alice"));
        }
        other => panic!("expected MessageReceived event, got: {other:?}"),
    }
}

// ===========================================================================
// Postcondition 3: Sender sees delivery confirmation (Sent → Delivered)
// ===========================================================================

/// MSS steps 7-8: After Bob auto-acks, Alice's status transitions from
/// Sent to Delivered, and a StatusChanged event is emitted.
#[tokio::test]
async fn sender_status_transitions_sent_to_delivered() {
    let (alice, mut alice_events, _warnings, bob, _bob_events) = create_connected_pair();
    let conversation = ConversationId::new();

    // Alice sends
    let (msg_id, initial_status) = alice
        .send_message(MessageContent::Text("ack me please".into()), conversation)
        .await
        .expect("send should succeed");

    // Initial status is Sent
    assert_eq!(
        initial_status,
        MessageStatus::Sent,
        "initial status should be Sent"
    );
    assert_eq!(
        alice.get_status(&msg_id).await,
        Some(MessageStatus::Sent),
        "tracked status should be Sent after send"
    );

    // Drain the Sent event
    let sent_event = alice_events.try_recv().expect("should have Sent event");
    assert_eq!(
        sent_event,
        ChatEvent::StatusChanged {
            message_id: msg_id.clone(),
            status: MessageStatus::Sent,
        }
    );

    // Bob receives (auto-sends ack back)
    bob.receive_one().await.expect("bob receive should succeed");

    // Alice receives the ack
    let ack_envelope = alice.receive_one().await.expect("alice should receive ack");
    assert!(
        matches!(ack_envelope, Envelope::Ack(_)),
        "alice should receive an Ack envelope"
    );

    // Status should now be Delivered
    assert_eq!(
        alice.get_status(&msg_id).await,
        Some(MessageStatus::Delivered),
        "status should be Delivered after ack"
    );

    // UI event for the status change
    let delivered_event = alice_events
        .try_recv()
        .expect("should have Delivered event");
    assert_eq!(
        delivered_event,
        ChatEvent::StatusChanged {
            message_id: msg_id,
            status: MessageStatus::Delivered,
        }
    );
}

// ===========================================================================
// Postcondition 1: Message is stored in Sender's local history
// ===========================================================================

/// After sending and receiving an ack, the message should be in Alice's
/// local history with Delivered status.
#[tokio::test]
async fn message_stored_in_sender_history_with_delivered_status() {
    let (alice, _alice_events, _warnings, bob, _bob_events) = create_connected_pair();
    let conversation = ConversationId::new();
    let original_text = "save me to history";

    // Alice sends
    let (msg_id, _) = alice
        .send_message(
            MessageContent::Text(original_text.to_string()),
            conversation.clone(),
        )
        .await
        .expect("send should succeed");

    // Immediately after send, history should have the message with Sent status
    let history = alice.history().expect("alice should have history enabled");
    let records = history
        .get_conversation(&conversation, 10)
        .await
        .expect("history read should succeed");
    assert_eq!(records.len(), 1, "history should have 1 message after send");
    let MessageContent::Text(ref saved_text) = records[0].0.content;
    assert_eq!(saved_text, original_text, "saved content should match");
    assert_eq!(
        records[0].1,
        MessageStatus::Sent,
        "status should be Sent initially"
    );

    // Bob receives (auto-acks) and Alice receives the ack
    bob.receive_one().await.expect("bob receive should succeed");
    alice
        .receive_one()
        .await
        .expect("alice receive ack should succeed");

    // Now history should show Delivered status
    let records = history
        .get_conversation(&conversation, 10)
        .await
        .expect("history read should succeed");
    assert_eq!(records.len(), 1, "history should still have 1 message");
    assert_eq!(
        records[0].0.metadata.message_id, msg_id,
        "message ID should match"
    );
    assert_eq!(
        records[0].1,
        MessageStatus::Delivered,
        "history status should be Delivered after ack"
    );
}

// ===========================================================================
// Invariant 1: Plaintext never leaves the application boundary
// ===========================================================================

/// Bytes on the wire must be encrypted — the plaintext content must NOT
/// appear anywhere in the raw transport payload.
#[tokio::test]
async fn plaintext_never_appears_on_wire() {
    // Use a raw transport pair so we can inspect the bytes on the wire
    let (transport_a, transport_b) =
        LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 64);

    let (alice, _events) = ChatManager::<StubNoiseSession, LoopbackTransport, InMemoryStore>::new(
        StubNoiseSession::new(true),
        transport_a,
        SenderId::new(vec![0xAA]),
        PeerId::new("bob"),
        64,
    );

    let plaintext_content = "this is a secret message that must be encrypted";

    alice
        .send_message(
            MessageContent::Text(plaintext_content.to_string()),
            ConversationId::new(),
        )
        .await
        .expect("send should succeed");

    // Read raw bytes from the transport (before any ChatManager decryption)
    let (_from, raw_wire_bytes) = transport_b.recv().await.expect("should receive raw bytes");

    // The raw wire bytes must NOT contain the plaintext
    let plaintext_bytes = plaintext_content.as_bytes();
    assert!(
        !raw_wire_bytes
            .windows(plaintext_bytes.len())
            .any(|window| window == plaintext_bytes),
        "plaintext content must NOT appear in the raw wire bytes (Invariant 1)"
    );

    // Also verify that the raw bytes cannot be decoded as a valid Envelope
    // without decryption (they should be garbled without the crypto key)
    let direct_decode_result = codec::decode(&raw_wire_bytes);
    // The decode may succeed or fail since XOR may produce valid-looking bytes,
    // but if it succeeds the content should NOT match the original plaintext.
    if let Ok(Envelope::Chat(msg)) = direct_decode_result {
        let MessageContent::Text(ref text) = msg.content;
        assert_ne!(
            text, plaintext_content,
            "even if decode succeeds on raw bytes, content should not match plaintext"
        );
    }
}

// ===========================================================================
// Invariant 2: Message ordering is preserved per-conversation
// ===========================================================================

/// Multiple messages sent in sequence must arrive in the same order.
#[tokio::test]
async fn message_ordering_preserved_per_conversation() {
    let (alice, _alice_events, _warnings, bob, _bob_events) = create_connected_pair();
    let conversation = ConversationId::new();
    let message_count = 10;

    // Alice sends N messages sequentially
    for i in 0..message_count {
        alice
            .send_message(
                MessageContent::Text(format!("ordered message #{i}")),
                conversation.clone(),
            )
            .await
            .expect("send should succeed");
    }

    // Bob receives all N messages in order
    for i in 0..message_count {
        let envelope = bob.receive_one().await.expect("receive should succeed");
        match envelope {
            Envelope::Chat(msg) => {
                let MessageContent::Text(ref text) = msg.content;
                assert_eq!(
                    text,
                    &format!("ordered message #{i}"),
                    "message at position {i} should have correct content (ordering preserved)"
                );
            }
            other => panic!("expected Chat envelope at position {i}, got: {other:?}"),
        }
    }
}

// ===========================================================================
// Full end-to-end scenario: send + receive + ack + history + ordering
// ===========================================================================

/// Complete end-to-end scenario exercising all postconditions and invariants
/// across multiple messages. This is the capstone test for UC-001.
#[tokio::test]
async fn full_end_to_end_scenario() {
    let (alice, mut alice_events, _warnings, bob, mut bob_events) = create_connected_pair();
    let conversation = ConversationId::new();
    let messages = vec![
        "First message from Alice",
        "Second message from Alice",
        "Third message from Alice",
    ];

    // --- Phase 1: Alice sends all messages ---
    let mut msg_ids = Vec::new();
    for text in &messages {
        let (id, status) = alice
            .send_message(MessageContent::Text(text.to_string()), conversation.clone())
            .await
            .expect("send should succeed");
        assert_eq!(status, MessageStatus::Sent);
        msg_ids.push(id);
    }

    // All should be Sent
    for id in &msg_ids {
        assert_eq!(alice.get_status(id).await, Some(MessageStatus::Sent));
    }

    // Drain Sent events
    for _ in 0..messages.len() {
        let event = alice_events.try_recv().expect("should have Sent event");
        assert!(matches!(
            event,
            ChatEvent::StatusChanged {
                status: MessageStatus::Sent,
                ..
            }
        ));
    }

    // --- Phase 2: Bob receives all messages (auto-acks each) ---
    for (i, expected_text) in messages.iter().enumerate() {
        let envelope = bob.receive_one().await.expect("bob receive should succeed");
        match envelope {
            Envelope::Chat(msg) => {
                let MessageContent::Text(ref text) = msg.content;
                assert_eq!(
                    text, expected_text,
                    "message {i} content should match (ordering)"
                );
            }
            other => panic!("expected Chat at position {i}, got: {other:?}"),
        }

        // Verify Bob gets a MessageReceived event
        let event = bob_events
            .try_recv()
            .expect("bob should have MessageReceived event");
        assert!(matches!(event, ChatEvent::MessageReceived { .. }));
    }

    // --- Phase 3: Alice receives all acks ---
    for _ in 0..messages.len() {
        let envelope = alice.receive_one().await.expect("alice should receive ack");
        assert!(matches!(envelope, Envelope::Ack(_)));
    }

    // --- Phase 4: Verify all statuses are Delivered ---
    for (i, id) in msg_ids.iter().enumerate() {
        assert_eq!(
            alice.get_status(id).await,
            Some(MessageStatus::Delivered),
            "message {i} should be Delivered after ack"
        );
    }

    // Verify Delivered events were emitted
    for _ in 0..messages.len() {
        let event = alice_events
            .try_recv()
            .expect("should have Delivered event");
        assert!(matches!(
            event,
            ChatEvent::StatusChanged {
                status: MessageStatus::Delivered,
                ..
            }
        ));
    }

    // --- Phase 5: Verify history ---
    let history = alice.history().expect("alice should have history");
    let records = history
        .get_conversation(&conversation, 10)
        .await
        .expect("history read should succeed");
    assert_eq!(
        records.len(),
        messages.len(),
        "history should have all messages"
    );

    // All should be Delivered in history
    for record in &records {
        assert_eq!(
            record.1,
            MessageStatus::Delivered,
            "all messages in history should be Delivered"
        );
    }
}

// ===========================================================================
// Extension 2a: Empty and oversized messages are rejected
// ===========================================================================

/// Empty messages fail validation at step 2 of the MSS.
#[tokio::test]
async fn empty_message_rejected() {
    let (alice, _events, _warnings, _bob, _bob_events) = create_connected_pair();

    let result = alice
        .send_message(MessageContent::Text(String::new()), ConversationId::new())
        .await;

    assert!(
        matches!(result, Err(SendError::Validation(_))),
        "empty message should fail validation"
    );
}

/// Oversized messages (> 64KB) fail validation at step 2 of the MSS.
#[tokio::test]
async fn oversized_message_rejected() {
    let (alice, _events, _warnings, _bob, _bob_events) = create_connected_pair();

    let big_text = "x".repeat(termchat_proto::message::MAX_MESSAGE_SIZE + 1);
    let result = alice
        .send_message(MessageContent::Text(big_text), ConversationId::new())
        .await;

    assert!(
        matches!(result, Err(SendError::Validation(_))),
        "oversized message should fail validation"
    );
}

// ===========================================================================
// Extension 4a: No Noise session → send fails with crypto error
// ===========================================================================

/// If no crypto session is established, sending should fail with a Crypto error.
#[tokio::test]
async fn no_crypto_session_returns_crypto_error() {
    let (transport_a, _transport_b) =
        LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 64);

    let (alice, _events) = ChatManager::<StubNoiseSession, LoopbackTransport, InMemoryStore>::new(
        StubNoiseSession::new(false), // NOT established
        transport_a,
        SenderId::new(vec![0xAA]),
        PeerId::new("bob"),
        64,
    );

    let result = alice
        .send_message(MessageContent::Text("hello".into()), ConversationId::new())
        .await;

    assert!(
        matches!(result, Err(SendError::Crypto(_))),
        "should fail with Crypto error when no session established"
    );
}

// ===========================================================================
// Extension 5b: Offline / disconnected transport → send fails
// ===========================================================================

/// If the transport is disconnected, sending should fail with a Transport error.
#[tokio::test]
async fn disconnected_transport_returns_transport_error() {
    let (transport_a, transport_b) =
        LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 64);
    drop(transport_b); // simulate offline

    let (alice, _events) = ChatManager::<StubNoiseSession, LoopbackTransport, InMemoryStore>::new(
        StubNoiseSession::new(true),
        transport_a,
        SenderId::new(vec![0xAA]),
        PeerId::new("bob"),
        64,
    );

    let result = alice
        .send_message(MessageContent::Text("hello".into()), ConversationId::new())
        .await;

    assert!(
        matches!(result, Err(SendError::Transport(_))),
        "should fail with Transport error when disconnected"
    );
}

// ===========================================================================
// Extension 7a: Ack timeout → status remains Sent
// ===========================================================================

/// If Bob never acks, await_ack should time out and return Sent status.
#[tokio::test]
async fn ack_timeout_leaves_status_as_sent() {
    let (alice, _events, _warnings, _bob, _bob_events) = create_connected_pair();
    let conversation = ConversationId::new();

    let (msg_id, _) = alice
        .send_message(MessageContent::Text("no ack coming".into()), conversation)
        .await
        .expect("send should succeed");

    // Do NOT have Bob receive — no ack will be sent back
    let config = RetryConfig {
        ack_timeout: Duration::from_millis(50),
        ack_retries: 0,
        ..Default::default()
    };

    let status = alice.await_ack(&msg_id, &config).await;
    assert_eq!(
        status,
        MessageStatus::Sent,
        "status should remain Sent when ack times out (Extension 7a)"
    );
}

// ===========================================================================
// Unique message IDs
// ===========================================================================

/// Each sent message must get a unique MessageId.
#[tokio::test]
async fn each_message_gets_unique_id() {
    let (alice, _events, _warnings, _bob, _bob_events) = create_connected_pair();
    let conversation = ConversationId::new();

    let mut ids = Vec::new();
    for i in 0..5 {
        let (id, _) = alice
            .send_message(
                MessageContent::Text(format!("msg {i}")),
                conversation.clone(),
            )
            .await
            .expect("send should succeed");
        ids.push(id);
    }

    // All IDs should be unique
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            assert_ne!(ids[i], ids[j], "message IDs {i} and {j} must differ");
        }
    }
}

// ===========================================================================
// Bidirectional: both Alice and Bob can send and receive
// ===========================================================================

/// Both parties can send and receive messages through the full pipeline.
#[tokio::test]
async fn bidirectional_message_exchange() {
    let (alice, mut alice_events, _a_warnings, bob, mut bob_events, _b_warnings) =
        create_pair_both_with_history();
    let conversation = ConversationId::new();

    // Alice sends to Bob
    let (alice_msg_id, _) = alice
        .send_message(
            MessageContent::Text("Hello from Alice".into()),
            conversation.clone(),
        )
        .await
        .expect("alice send should succeed");

    // Bob receives Alice's message
    let envelope = bob.receive_one().await.expect("bob should receive");
    match envelope {
        Envelope::Chat(msg) => {
            let MessageContent::Text(ref text) = msg.content;
            assert_eq!(text, "Hello from Alice");
        }
        other => panic!("expected Chat, got: {other:?}"),
    }

    // Alice receives the ack
    alice.receive_one().await.expect("alice should receive ack");
    assert_eq!(
        alice.get_status(&alice_msg_id).await,
        Some(MessageStatus::Delivered)
    );

    // Now Bob sends to Alice
    let (bob_msg_id, _) = bob
        .send_message(
            MessageContent::Text("Hello from Bob".into()),
            conversation.clone(),
        )
        .await
        .expect("bob send should succeed");

    // Alice receives Bob's message
    let envelope = alice.receive_one().await.expect("alice should receive");
    match envelope {
        Envelope::Chat(msg) => {
            let MessageContent::Text(ref text) = msg.content;
            assert_eq!(text, "Hello from Bob");
        }
        other => panic!("expected Chat, got: {other:?}"),
    }

    // Bob receives the ack
    bob.receive_one().await.expect("bob should receive ack");
    assert_eq!(
        bob.get_status(&bob_msg_id).await,
        Some(MessageStatus::Delivered)
    );

    // Drain events to verify they exist (Sent + Delivered for each side)
    // Alice: Sent for her message, Delivered for her message, MessageReceived for Bob's
    let _ = alice_events.try_recv().expect("alice Sent event");
    let _ = alice_events.try_recv().expect("alice Delivered event");
    // Bob: MessageReceived for Alice's, Sent for his message, Delivered for his message
    let _ = bob_events.try_recv().expect("bob MessageReceived event");
    let _ = bob_events.try_recv().expect("bob Sent event");
    let _ = bob_events.try_recv().expect("bob Delivered event");
}

// ===========================================================================
// UC-002: Receive Direct Message Tests
// ===========================================================================

/// UC-002: Bob receives a message and it is stored in his local history.
#[tokio::test]
async fn receive_stores_message_in_history() {
    let (alice, _alice_events, _a_warnings, bob, _bob_events, _b_warnings) =
        create_pair_both_with_history();
    let conversation = ConversationId::new();
    let original_text = "message for bob's history";

    // Alice sends
    alice
        .send_message(
            MessageContent::Text(original_text.to_string()),
            conversation.clone(),
        )
        .await
        .expect("send should succeed");

    // Bob receives
    bob.receive_one().await.expect("bob receive should succeed");

    // Verify Bob's history has the message with Delivered status
    let bob_history = bob.history().expect("bob should have history");
    let records = bob_history
        .get_conversation(&conversation, 10)
        .await
        .expect("bob history read should succeed");

    assert_eq!(records.len(), 1, "bob history should have 1 message");
    let MessageContent::Text(ref text) = records[0].0.content;
    assert_eq!(text, original_text, "content should match");
    assert_eq!(
        records[0].1,
        MessageStatus::Delivered,
        "status should be Delivered after receive"
    );
}

/// UC-002: Bob receives a message and emits a MessageReceived event.
#[tokio::test]
async fn receive_emits_message_received_event() {
    let (alice, _alice_events, _warnings, bob, mut bob_events) = create_connected_pair();
    let conversation = ConversationId::new();

    alice
        .send_message(MessageContent::Text("test event".into()), conversation)
        .await
        .expect("send should succeed");

    bob.receive_one().await.expect("bob receive should succeed");

    let event = bob_events
        .try_recv()
        .expect("bob should have MessageReceived event");
    match event {
        ChatEvent::MessageReceived { message, from } => {
            let MessageContent::Text(ref text) = message.content;
            assert_eq!(text, "test event");
            assert_eq!(from, PeerId::new("alice"));
        }
        other => panic!("expected MessageReceived, got: {other:?}"),
    }
}

/// UC-002 Invariant 3: Duplicate messages are deduplicated.
#[tokio::test]
async fn duplicate_message_deduplicated() {
    let (alice_transport, bob_transport) =
        LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 64);

    let (alice, _alice_events) =
        ChatManager::<StubNoiseSession, LoopbackTransport, InMemoryStore>::new(
            StubNoiseSession::new(true),
            alice_transport,
            SenderId::new(vec![0xAA]),
            PeerId::new("bob"),
            64,
        );

    let (bob, mut bob_events) =
        ChatManager::<StubNoiseSession, LoopbackTransport, InMemoryStore>::new(
            StubNoiseSession::new(true),
            bob_transport,
            SenderId::new(vec![0xBB]),
            PeerId::new("alice"),
            64,
        );

    let conversation = ConversationId::new();

    // Send the same message twice
    let (msg_id, _) = alice
        .send_message(
            MessageContent::Text("duplicate test".into()),
            conversation.clone(),
        )
        .await
        .expect("first send should succeed");

    // Bob receives first instance
    bob.receive_one()
        .await
        .expect("first receive should succeed");
    let event1 = bob_events.try_recv().expect("should have first event");
    assert!(matches!(event1, ChatEvent::MessageReceived { .. }));

    // Manually send the same encrypted payload again by re-encrypting the same message
    let same_message = ChatMessage {
        metadata: MessageMetadata {
            message_id: msg_id.clone(),
            timestamp: Timestamp::now(),
            sender_id: SenderId::new(vec![0xAA]),
            conversation_id: conversation,
        },
        content: MessageContent::Text("duplicate test".into()),
    };
    let envelope = Envelope::Chat(same_message);
    let serialized = codec::encode(&envelope).unwrap();
    let encrypted = alice.crypto().encrypt(&serialized).unwrap();
    alice
        .transport()
        .send(&PeerId::new("bob"), &encrypted)
        .await
        .unwrap();

    // Bob receives duplicate
    bob.receive_one()
        .await
        .expect("duplicate receive should succeed");

    // Bob should NOT emit a second MessageReceived event (message was deduplicated)
    assert!(
        bob_events.try_recv().is_err(),
        "duplicate message should not emit a second event"
    );
}

/// UC-002 Extension 1a: Oversized payload is dropped silently before decryption.
#[tokio::test]
async fn oversized_payload_dropped_silently() {
    let (alice_transport, bob_transport) =
        LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 128);

    let (bob, _bob_events) = ChatManager::<StubNoiseSession, LoopbackTransport, InMemoryStore>::new(
        StubNoiseSession::new(true),
        bob_transport,
        SenderId::new(vec![0xBB]),
        PeerId::new("alice"),
        64,
    );

    // Manually craft an oversized payload (> 64KB)
    let huge_payload = vec![0xFF; 65 * 1024];
    alice_transport
        .send(&PeerId::new("bob"), &huge_payload)
        .await
        .expect("transport send should succeed");

    // Bob should reject it with an OversizedPayload error
    let result = bob.receive_one().await;
    assert!(
        matches!(result, Err(SendError::OversizedPayload { .. })),
        "oversized payload should be rejected"
    );
}

/// UC-002 Extension 5a: Deserialization failure sends a NACK.
#[tokio::test]
async fn deserialization_failure_sends_nack() {
    let (alice_transport, bob_transport) =
        LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 64);

    let (alice, _alice_events) =
        ChatManager::<StubNoiseSession, LoopbackTransport, InMemoryStore>::new(
            StubNoiseSession::new(true),
            alice_transport,
            SenderId::new(vec![0xAA]),
            PeerId::new("bob"),
            64,
        );

    let (bob, _bob_events) = ChatManager::<StubNoiseSession, LoopbackTransport, InMemoryStore>::new(
        StubNoiseSession::new(true),
        bob_transport,
        SenderId::new(vec![0xBB]),
        PeerId::new("alice"),
        64,
    );

    // Alice sends corrupted data (encrypt garbage bytes)
    let garbage = vec![0xFF; 100];
    let encrypted = alice.crypto().encrypt(&garbage).unwrap();
    alice
        .transport()
        .send(&PeerId::new("bob"), &encrypted)
        .await
        .unwrap();

    // Bob tries to receive — should fail deserialization and send NACK
    let result = bob.receive_one().await;
    assert!(
        matches!(result, Err(SendError::Codec(_))),
        "corrupted payload should fail deserialization"
    );

    // Alice should receive the NACK
    let envelope = alice
        .receive_one()
        .await
        .expect("alice should receive NACK");
    match envelope {
        Envelope::Nack(nack) => {
            assert_eq!(
                nack.reason,
                termchat_proto::message::NackReason::DeserializationFailed
            );
        }
        other => panic!("expected Nack, got: {other:?}"),
    }
}

/// UC-002 Extension 6a: Message with clock skew is still displayed with a warning.
#[tokio::test]
async fn receive_with_clock_skew_still_displays() {
    let (alice_transport, bob_transport) =
        LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 64);

    let (alice, _alice_events) =
        ChatManager::<StubNoiseSession, LoopbackTransport, InMemoryStore>::new(
            StubNoiseSession::new(true),
            alice_transport,
            SenderId::new(vec![0xAA]),
            PeerId::new("bob"),
            64,
        );

    let (bob, mut bob_events) =
        ChatManager::<StubNoiseSession, LoopbackTransport, InMemoryStore>::new(
            StubNoiseSession::new(true),
            bob_transport,
            SenderId::new(vec![0xBB]),
            PeerId::new("alice"),
            64,
        );

    // Craft a message with an old timestamp (10 minutes in the past)
    let old_timestamp = Timestamp::from_millis(Timestamp::now().as_millis() - 10 * 60 * 1000);
    let message = ChatMessage {
        metadata: MessageMetadata {
            message_id: MessageId::new(),
            timestamp: old_timestamp,
            sender_id: SenderId::new(vec![0xAA]),
            conversation_id: ConversationId::new(),
        },
        content: MessageContent::Text("old message".into()),
    };
    let envelope = Envelope::Chat(message);
    let serialized = codec::encode(&envelope).unwrap();
    let encrypted = alice.crypto().encrypt(&serialized).unwrap();
    alice
        .transport()
        .send(&PeerId::new("bob"), &encrypted)
        .await
        .unwrap();

    // Bob receives — should still display but with clock skew warning
    bob.receive_one().await.expect("receive should succeed");

    let event = bob_events.try_recv().expect("should have event");
    match event {
        ChatEvent::MessageReceivedWithClockSkew {
            message,
            from,
            skew_description,
        } => {
            let MessageContent::Text(ref text) = message.content;
            assert_eq!(text, "old message");
            assert_eq!(from, PeerId::new("alice"));
            assert!(skew_description.contains("timestamp"));
        }
        other => panic!("expected MessageReceivedWithClockSkew, got: {other:?}"),
    }
}

/// UC-002 Extension 8a: Ack failure does not block message display.
#[tokio::test]
async fn ack_failure_does_not_block_display() {
    let (alice_transport, bob_transport) =
        LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 64);

    let (alice, _alice_events) =
        ChatManager::<StubNoiseSession, LoopbackTransport, InMemoryStore>::new(
            StubNoiseSession::new(true),
            alice_transport,
            SenderId::new(vec![0xAA]),
            PeerId::new("bob"),
            64,
        );

    let (bob, mut bob_events) =
        ChatManager::<StubNoiseSession, LoopbackTransport, InMemoryStore>::new(
            StubNoiseSession::new(true),
            bob_transport,
            SenderId::new(vec![0xBB]),
            PeerId::new("alice"),
            64,
        );

    let conversation = ConversationId::new();

    // Alice sends
    alice
        .send_message(MessageContent::Text("ack might fail".into()), conversation)
        .await
        .expect("send should succeed");

    // Drop Alice's transport before Bob receives (so ack send will fail)
    drop(alice);

    // Bob receives — message should still be emitted even if ack send fails
    bob.receive_one().await.expect("receive should succeed");

    let event = bob_events.try_recv().expect("should have event");
    assert!(
        matches!(event, ChatEvent::MessageReceived { .. }),
        "message should be emitted even if ack send fails"
    );
}

// Note: Extension 6c (sender ID mismatch) is tested via the sender_id_matches_peer
// validation logic, but with the current StubNoiseSession implementation, all peers
// are considered valid. A full test would require a real crypto implementation with
// proper identity verification (UC-005).
