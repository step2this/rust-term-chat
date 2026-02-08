//! Integration tests for UC-009: Typing Indicators & Presence Status.
//!
//! Verifies:
//! 1. Presence messages round-trip through the encrypted pipeline.
//! 2. Typing indicator messages round-trip through the encrypted pipeline.
//! 3. ChatEvent variants are emitted correctly for presence and typing.
//! 4. App state tracks presence and typing correctly.
//! 5. Fire-and-forget semantics: send failures don't propagate.

use termchat::app::{App, ConversationItem};
use termchat::chat::history::InMemoryStore;
use termchat::chat::{ChatEvent, ChatManager};
use termchat::crypto::noise::StubNoiseSession;
use termchat::transport::PeerId;
use termchat::transport::loopback::LoopbackTransport;

use termchat_proto::message::SenderId;
use termchat_proto::presence::{PresenceMessage, PresenceStatus};
use termchat_proto::typing::TypingMessage;

use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create Alice and Bob connected via loopback (no history).
fn create_connected_pair() -> (
    ChatManager<StubNoiseSession, LoopbackTransport, InMemoryStore>,
    mpsc::Receiver<ChatEvent>,
    ChatManager<StubNoiseSession, LoopbackTransport, InMemoryStore>,
    mpsc::Receiver<ChatEvent>,
) {
    let (transport_a, transport_b) =
        LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 64);

    let (alice, alice_events) = ChatManager::new(
        StubNoiseSession::new(true),
        transport_a,
        SenderId::new(vec![0xAA]),
        PeerId::new("bob"),
        64,
    );

    let (bob, bob_events) = ChatManager::new(
        StubNoiseSession::new(true),
        transport_b,
        SenderId::new(vec![0xBB]),
        PeerId::new("alice"),
        64,
    );

    (alice, alice_events, bob, bob_events)
}

// ---------------------------------------------------------------------------
// Presence tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn presence_update_round_trips_through_pipeline() {
    let (alice, _alice_events, bob, mut bob_events) = create_connected_pair();

    let presence = PresenceMessage {
        peer_id: "alice".into(),
        status: PresenceStatus::Online,
        timestamp: 1_700_000_000_000,
    };

    alice.send_presence(&presence).await;

    // Bob receives and decodes the presence update
    bob.receive_one().await.unwrap();

    let event = bob_events.try_recv().unwrap();
    match event {
        ChatEvent::PresenceChanged { peer_id, status } => {
            assert_eq!(peer_id, "alice");
            assert_eq!(status, PresenceStatus::Online);
        }
        other => panic!("expected PresenceChanged, got {other:?}"),
    }
}

#[tokio::test]
async fn presence_away_status_round_trips() {
    let (alice, _alice_events, bob, mut bob_events) = create_connected_pair();

    let presence = PresenceMessage {
        peer_id: "alice".into(),
        status: PresenceStatus::Away,
        timestamp: 1_700_000_000_000,
    };

    alice.send_presence(&presence).await;
    bob.receive_one().await.unwrap();

    let event = bob_events.try_recv().unwrap();
    match event {
        ChatEvent::PresenceChanged { status, .. } => {
            assert_eq!(status, PresenceStatus::Away);
        }
        other => panic!("expected PresenceChanged, got {other:?}"),
    }
}

#[tokio::test]
async fn presence_offline_status_round_trips() {
    let (alice, _alice_events, bob, mut bob_events) = create_connected_pair();

    let presence = PresenceMessage {
        peer_id: "alice".into(),
        status: PresenceStatus::Offline,
        timestamp: 1_700_000_000_000,
    };

    alice.send_presence(&presence).await;
    bob.receive_one().await.unwrap();

    let event = bob_events.try_recv().unwrap();
    match event {
        ChatEvent::PresenceChanged { status, .. } => {
            assert_eq!(status, PresenceStatus::Offline);
        }
        other => panic!("expected PresenceChanged, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Typing indicator tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn typing_indicator_start_round_trips() {
    let (alice, _alice_events, bob, mut bob_events) = create_connected_pair();

    let typing = TypingMessage {
        peer_id: "alice".into(),
        room_id: "general".into(),
        is_typing: true,
    };

    alice.send_typing(&typing).await;
    bob.receive_one().await.unwrap();

    let event = bob_events.try_recv().unwrap();
    match event {
        ChatEvent::TypingChanged {
            peer_id,
            room_id,
            is_typing,
        } => {
            assert_eq!(peer_id, "alice");
            assert_eq!(room_id, "general");
            assert!(is_typing);
        }
        other => panic!("expected TypingChanged, got {other:?}"),
    }
}

#[tokio::test]
async fn typing_indicator_stop_round_trips() {
    let (alice, _alice_events, bob, mut bob_events) = create_connected_pair();

    let typing = TypingMessage {
        peer_id: "alice".into(),
        room_id: "general".into(),
        is_typing: false,
    };

    alice.send_typing(&typing).await;
    bob.receive_one().await.unwrap();

    let event = bob_events.try_recv().unwrap();
    match event {
        ChatEvent::TypingChanged { is_typing, .. } => {
            assert!(!is_typing);
        }
        other => panic!("expected TypingChanged, got {other:?}"),
    }
}

#[tokio::test]
async fn multiple_presence_updates_all_arrive() {
    let (alice, _alice_events, bob, mut bob_events) = create_connected_pair();

    let statuses = [
        PresenceStatus::Online,
        PresenceStatus::Away,
        PresenceStatus::Offline,
    ];

    for status in &statuses {
        let presence = PresenceMessage {
            peer_id: "alice".into(),
            status: *status,
            timestamp: 1_700_000_000_000,
        };
        alice.send_presence(&presence).await;
    }

    for expected_status in &statuses {
        bob.receive_one().await.unwrap();
        let event = bob_events.try_recv().unwrap();
        match event {
            ChatEvent::PresenceChanged { status, .. } => {
                assert_eq!(status, *expected_status);
            }
            other => panic!("expected PresenceChanged, got {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Fire-and-forget semantics
// ---------------------------------------------------------------------------

#[tokio::test]
async fn presence_send_on_disconnected_transport_does_not_panic() {
    let (transport_a, transport_b) =
        LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 64);
    drop(transport_b); // disconnect

    let (manager, _events): (
        ChatManager<StubNoiseSession, LoopbackTransport, InMemoryStore>,
        _,
    ) = ChatManager::new(
        StubNoiseSession::new(true),
        transport_a,
        SenderId::new(vec![0xAA]),
        PeerId::new("bob"),
        64,
    );

    let presence = PresenceMessage {
        peer_id: "alice".into(),
        status: PresenceStatus::Online,
        timestamp: 1_700_000_000_000,
    };

    // Should not panic or return an error
    manager.send_presence(&presence).await;
}

#[tokio::test]
async fn typing_send_on_disconnected_transport_does_not_panic() {
    let (transport_a, transport_b) =
        LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 64);
    drop(transport_b);

    let (manager, _events): (
        ChatManager<StubNoiseSession, LoopbackTransport, InMemoryStore>,
        _,
    ) = ChatManager::new(
        StubNoiseSession::new(true),
        transport_a,
        SenderId::new(vec![0xAA]),
        PeerId::new("bob"),
        64,
    );

    let typing = TypingMessage {
        peer_id: "alice".into(),
        room_id: "general".into(),
        is_typing: true,
    };

    manager.send_typing(&typing).await;
}

// ---------------------------------------------------------------------------
// App state tests
// ---------------------------------------------------------------------------

#[test]
fn app_set_peer_presence_updates_map() {
    let mut app = App::new();

    app.set_peer_presence("Charlie", PresenceStatus::Online);
    assert_eq!(
        app.presence_map.get("Charlie"),
        Some(&PresenceStatus::Online)
    );

    app.set_peer_presence("Charlie", PresenceStatus::Away);
    assert_eq!(app.presence_map.get("Charlie"), Some(&PresenceStatus::Away));
}

#[test]
fn app_set_peer_presence_updates_dm_conversation() {
    let mut app = App::new();
    // App::new() has a "@ Alice" conversation with Online presence

    app.set_peer_presence("Alice", PresenceStatus::Away);

    let alice_conv = app
        .conversations
        .iter()
        .find(|c| c.name == "@ Alice")
        .unwrap();
    assert_eq!(alice_conv.presence, Some(PresenceStatus::Away));
}

#[test]
fn app_set_peer_typing_adds_and_removes() {
    let mut app = App::new();

    // Use a room without pre-existing demo typing data
    app.set_peer_typing("test-room", "Bob", true);
    assert!(app.typing_peers.get("test-room").unwrap().contains("Bob"));

    app.set_peer_typing("test-room", "Bob", false);
    assert!(app.typing_peers.get("test-room").is_none());
}

#[test]
fn app_set_peer_typing_multiple_peers() {
    let mut app = App::new();

    app.set_peer_typing("dev", "Alice", true);
    app.set_peer_typing("dev", "Bob", true);

    let peers = app.typing_peers.get("dev").unwrap();
    assert!(peers.contains("Alice"));
    assert!(peers.contains("Bob"));
    assert_eq!(peers.len(), 2);

    app.set_peer_typing("dev", "Alice", false);
    let peers = app.typing_peers.get("dev").unwrap();
    assert!(!peers.contains("Alice"));
    assert!(peers.contains("Bob"));
}

#[test]
fn app_current_typing_peers_returns_correct_list() {
    let app = App::new();
    // App::new() starts with "# general" selected (index 0)

    // The demo data already has Alice typing in general
    let peers = app.current_typing_peers();
    assert_eq!(peers.len(), 1);
    assert!(peers.contains(&"Alice"));
}

#[test]
fn app_tick_typing_expires_after_timeout() {
    let mut app = App::new();
    app.typing_timer = Some(std::time::Instant::now() - std::time::Duration::from_secs(5));
    app.local_typing = true;

    app.tick_typing();

    assert!(!app.local_typing);
    assert!(app.typing_timer.is_none());
}

#[test]
fn app_tick_typing_does_not_expire_if_recent() {
    let mut app = App::new();
    app.typing_timer = Some(std::time::Instant::now());
    app.local_typing = true;

    app.tick_typing();

    assert!(app.local_typing);
    assert!(app.typing_timer.is_some());
}

#[test]
fn conversation_item_presence_defaults_to_none_for_rooms() {
    let conv = ConversationItem {
        name: "# general".to_string(),
        unread_count: 0,
        last_message_preview: None,
        is_agent: false,
        presence: None,
    };
    assert!(conv.presence.is_none());
}

#[test]
fn conversation_item_presence_set_for_dm() {
    let conv = ConversationItem {
        name: "@ Bob".to_string(),
        unread_count: 0,
        last_message_preview: None,
        is_agent: false,
        presence: Some(PresenceStatus::Online),
    };
    assert_eq!(conv.presence, Some(PresenceStatus::Online));
}
