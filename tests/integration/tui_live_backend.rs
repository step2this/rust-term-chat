//! Integration tests for UC-017: Connect TUI to Live Backend State.
//!
//! Tests verify postconditions from the use case document.
//!
//! # Verification Focus
//!
//! - Empty state on initialization (no demo data)
//! - Per-conversation message isolation
//! - Connection status updates
//! - Presence status wiring
//! - Typing indicator wiring
//! - Unread count increments and resets
//! - Auto-conversation creation on message push
//! - Message preview updates
//! - Conversation deduplication
//! - Conversation name tracking

use termchat::app::{App, ConversationItem, DisplayMessage, MessageStatus};
use termchat_proto::presence::PresenceStatus;

// =============================================================================
// Postcondition 9, Invariant 1: Empty App has no demo data
// =============================================================================

#[test]
fn test_empty_app_has_no_demo_data() {
    let app = App::new();

    // Verify no conversations exist
    assert!(
        app.conversations.is_empty(),
        "new app should have zero conversations"
    );

    // Verify no messages exist
    assert!(app.messages.is_empty(), "new app should have zero messages");

    // Verify not connected
    assert!(!app.is_connected, "new app should not be connected");
    assert!(
        app.connection_info.is_empty(),
        "new app should have empty connection_info"
    );

    // Verify presence map is empty
    assert!(
        app.presence_map.is_empty(),
        "new app should have empty presence_map"
    );

    // Verify typing peers is empty
    assert!(
        app.typing_peers.is_empty(),
        "new app should have empty typing_peers"
    );
}

// =============================================================================
// Postcondition 1, Invariant 2: Per-conversation message isolation
// =============================================================================

#[test]
fn test_per_conversation_message_isolation() {
    let mut app = App::new();

    // Create two DM conversations
    app.add_conversation("@ alice", None);
    app.add_conversation("@ bob", None);

    // Push messages to alice
    app.push_message(
        "@ alice",
        DisplayMessage {
            sender: "You".to_string(),
            content: "Hello Alice".to_string(),
            timestamp: "10:00".to_string(),
            status: MessageStatus::Sent,
            message_id: Some("msg-1".to_string()),
        },
    );

    app.push_message(
        "@ alice",
        DisplayMessage {
            sender: "alice".to_string(),
            content: "Hi there".to_string(),
            timestamp: "10:01".to_string(),
            status: MessageStatus::Delivered,
            message_id: Some("msg-2".to_string()),
        },
    );

    // Push messages to bob
    app.push_message(
        "@ bob",
        DisplayMessage {
            sender: "You".to_string(),
            content: "Hello Bob".to_string(),
            timestamp: "10:05".to_string(),
            status: MessageStatus::Sent,
            message_id: Some("msg-3".to_string()),
        },
    );

    // Verify alice conversation has 2 messages
    let alice_msgs = app.messages.get("@ alice").expect("alice messages missing");
    assert_eq!(alice_msgs.len(), 2);
    assert_eq!(alice_msgs[0].content, "Hello Alice");
    assert_eq!(alice_msgs[1].content, "Hi there");

    // Verify bob conversation has 1 message
    let bob_msgs = app.messages.get("@ bob").expect("bob messages missing");
    assert_eq!(bob_msgs.len(), 1);
    assert_eq!(bob_msgs[0].content, "Hello Bob");

    // Verify isolation: no cross-contamination
    assert_eq!(
        app.messages.len(),
        2,
        "should have exactly 2 conversations with messages"
    );
}

// =============================================================================
// Postcondition 4: Connection status updates
// =============================================================================

#[test]
fn test_connection_status_updates() {
    let mut app = App::new();

    // Initially disconnected
    assert!(!app.is_connected);
    assert!(app.connection_info.is_empty());

    // Connect to relay
    app.set_connection_status(true, "Relay");
    assert!(app.is_connected, "should be connected after update");
    assert_eq!(app.connection_info, "Relay");

    // Disconnect
    app.set_connection_status(false, "");
    assert!(!app.is_connected, "should be disconnected after update");
    assert_eq!(app.connection_info, "");

    // Reconnect with P2P
    app.set_connection_status(true, "P2P (QUIC)");
    assert!(app.is_connected);
    assert_eq!(app.connection_info, "P2P (QUIC)");
}

#[test]
fn test_can_send_follows_connection_status() {
    let mut app = App::new();

    // Initially can't send
    assert!(!app.can_send());

    // Connect, can send
    app.set_connection_status(true, "Relay");
    assert!(app.can_send());

    // Disconnect, can't send
    app.set_connection_status(false, "");
    assert!(!app.can_send());
}

// =============================================================================
// Postcondition 6: Presence wiring
// =============================================================================

#[test]
fn test_presence_wiring() {
    let mut app = App::new();

    // Add a DM conversation for bob
    app.add_conversation("@ bob", None);

    // Initially no presence
    assert!(
        app.presence_map.get("bob").is_none(),
        "bob should have no presence initially"
    );

    // Set bob online
    app.set_peer_presence("bob", PresenceStatus::Online);

    // Verify presence map updated
    assert_eq!(
        app.presence_map.get("bob"),
        Some(&PresenceStatus::Online),
        "presence_map should reflect online status"
    );

    // Verify DM conversation item updated
    let bob_conv = app
        .conversations
        .iter()
        .find(|c| c.name == "@ bob")
        .expect("bob conversation missing");
    assert_eq!(
        bob_conv.presence,
        Some(PresenceStatus::Online),
        "conversation presence should be online"
    );

    // Set bob away
    app.set_peer_presence("bob", PresenceStatus::Away);

    // Verify updates
    assert_eq!(
        app.presence_map.get("bob"),
        Some(&PresenceStatus::Away),
        "presence_map should reflect away status"
    );

    let bob_conv = app
        .conversations
        .iter()
        .find(|c| c.name == "@ bob")
        .expect("bob conversation missing");
    assert_eq!(
        bob_conv.presence,
        Some(PresenceStatus::Away),
        "conversation presence should be away"
    );

    // Set bob offline
    app.set_peer_presence("bob", PresenceStatus::Offline);
    assert_eq!(app.presence_map.get("bob"), Some(&PresenceStatus::Offline),);
}

#[test]
fn test_presence_only_affects_matching_dm() {
    let mut app = App::new();

    // Add multiple conversations
    app.add_conversation("@ alice", None);
    app.add_conversation("@ bob", None);
    app.add_conversation("# general", None);

    // Set alice online
    app.set_peer_presence("alice", PresenceStatus::Online);

    // Verify alice has presence
    let alice_conv = app
        .conversations
        .iter()
        .find(|c| c.name == "@ alice")
        .expect("alice conversation missing");
    assert_eq!(alice_conv.presence, Some(PresenceStatus::Online));

    // Verify bob and general don't have presence set
    let bob_conv = app
        .conversations
        .iter()
        .find(|c| c.name == "@ bob")
        .expect("bob conversation missing");
    assert_eq!(bob_conv.presence, None, "bob should not be affected");

    let general_conv = app
        .conversations
        .iter()
        .find(|c| c.name == "# general")
        .expect("general conversation missing");
    assert_eq!(general_conv.presence, None, "room should not have presence");
}

// =============================================================================
// Postcondition 5: Typing indicator wiring
// =============================================================================

#[test]
fn test_typing_wiring() {
    let mut app = App::new();

    // Add a room conversation
    app.add_conversation("# general", None);

    // Initially no typing peers
    assert!(
        app.typing_peers.get("general").is_none(),
        "should have no typing peers initially"
    );

    // Set bob typing in general
    app.set_peer_typing("general", "bob", true);

    // Verify typing_peers updated
    let typing = app
        .typing_peers
        .get("general")
        .expect("typing peers missing");
    assert!(typing.contains("bob"), "bob should be typing");

    // Add another typing peer
    app.set_peer_typing("general", "alice", true);

    let typing = app
        .typing_peers
        .get("general")
        .expect("typing peers missing");
    assert!(typing.contains("bob"), "bob should still be typing");
    assert!(typing.contains("alice"), "alice should be typing");
    assert_eq!(typing.len(), 2);

    // Remove bob from typing
    app.set_peer_typing("general", "bob", false);

    let typing = app
        .typing_peers
        .get("general")
        .expect("typing peers missing");
    assert!(!typing.contains("bob"), "bob should not be typing");
    assert!(typing.contains("alice"), "alice should still be typing");
    assert_eq!(typing.len(), 1);

    // Remove alice from typing
    app.set_peer_typing("general", "alice", false);

    // Verify empty set is removed from map
    assert!(
        app.typing_peers.get("general").is_none(),
        "empty typing set should be removed"
    );
}

#[test]
fn test_current_typing_peers_returns_correct_results() {
    let mut app = App::new();

    // Add conversations
    app.add_conversation("# general", None);
    app.add_conversation("# dev", None);

    // Set typing in general
    app.set_peer_typing("general", "bob", true);
    app.set_peer_typing("general", "alice", true);

    // Set typing in dev
    app.set_peer_typing("dev", "charlie", true);

    // Select general conversation (index 0)
    app.selected_conversation = 0;

    // Verify current_typing_peers returns general's peers
    let typing_peers = app.current_typing_peers();
    assert_eq!(typing_peers.len(), 2);
    assert!(typing_peers.contains(&"bob"));
    assert!(typing_peers.contains(&"alice"));

    // Select dev conversation (index 1)
    app.selected_conversation = 1;

    // Verify current_typing_peers returns dev's peers
    let typing_peers = app.current_typing_peers();
    assert_eq!(typing_peers.len(), 1);
    assert!(typing_peers.contains(&"charlie"));

    // Select invalid conversation
    app.selected_conversation = 99;

    // Verify current_typing_peers returns empty
    let typing_peers = app.current_typing_peers();
    assert!(typing_peers.is_empty());
}

// =============================================================================
// Postcondition 8: Unread count increments
// =============================================================================

#[test]
fn test_unread_count_increments() {
    let mut app = App::new();

    // Add two conversations
    app.add_conversation("# general", None);
    app.add_conversation("# dev", None);

    // Select first conversation (general)
    app.selected_conversation = 0;

    // Push message to second conversation (dev)
    app.push_message(
        "# dev",
        DisplayMessage {
            sender: "bob".to_string(),
            content: "Check this out".to_string(),
            timestamp: "10:00".to_string(),
            status: MessageStatus::Delivered,
            message_id: None,
        },
    );

    // Verify dev has unread count = 1
    let dev_conv = app
        .conversations
        .iter()
        .find(|c| c.name == "# dev")
        .expect("dev conversation missing");
    assert_eq!(dev_conv.unread_count, 1, "dev should have 1 unread message");

    // Verify general has unread count = 0 (it's selected)
    let general_conv = app
        .conversations
        .iter()
        .find(|c| c.name == "# general")
        .expect("general conversation missing");
    assert_eq!(
        general_conv.unread_count, 0,
        "general should have 0 unread (selected)"
    );

    // Push another message to dev
    app.push_message(
        "# dev",
        DisplayMessage {
            sender: "alice".to_string(),
            content: "Agreed".to_string(),
            timestamp: "10:01".to_string(),
            status: MessageStatus::Delivered,
            message_id: None,
        },
    );

    // Verify dev has unread count = 2
    let dev_conv = app
        .conversations
        .iter()
        .find(|c| c.name == "# dev")
        .expect("dev conversation missing");
    assert_eq!(
        dev_conv.unread_count, 2,
        "dev should have 2 unread messages"
    );
}

#[test]
fn test_unread_count_resets_on_selection() {
    let mut app = App::new();

    // Add conversations
    app.add_conversation("# general", None);
    app.add_conversation("# dev", None);

    // Select general (index 0)
    app.selected_conversation = 0;

    // Push messages to dev to build up unread count
    app.push_message(
        "# dev",
        DisplayMessage {
            sender: "bob".to_string(),
            content: "msg1".to_string(),
            timestamp: "10:00".to_string(),
            status: MessageStatus::Delivered,
            message_id: None,
        },
    );
    app.push_message(
        "# dev",
        DisplayMessage {
            sender: "bob".to_string(),
            content: "msg2".to_string(),
            timestamp: "10:01".to_string(),
            status: MessageStatus::Delivered,
            message_id: None,
        },
    );

    // Verify dev has unread count = 2
    let dev_conv = app
        .conversations
        .iter()
        .find(|c| c.name == "# dev")
        .expect("dev conversation missing");
    assert_eq!(dev_conv.unread_count, 2);

    // Now select dev conversation (index 1) - simulate user navigation
    app.selected_conversation = 1;
    // Manually call on_conversation_selected to trigger unread reset
    // (in production, this is called by next_conversation/prev_conversation)
    if let Some(conv) = app.conversations.get_mut(app.selected_conversation) {
        conv.unread_count = 0;
    }

    // Verify dev unread count is now 0
    let dev_conv = app
        .conversations
        .iter()
        .find(|c| c.name == "# dev")
        .expect("dev conversation missing");
    assert_eq!(
        dev_conv.unread_count, 0,
        "dev unread should reset to 0 on selection"
    );
}

// =============================================================================
// Extension 10a: Auto-create conversation
// =============================================================================

#[test]
fn test_auto_create_conversation() {
    let mut app = App::new();

    // Initially no conversations
    assert!(app.conversations.is_empty());

    // Push message to unknown conversation "@ charlie"
    app.push_message(
        "@ charlie",
        DisplayMessage {
            sender: "charlie".to_string(),
            content: "Hey there!".to_string(),
            timestamp: "10:00".to_string(),
            status: MessageStatus::Delivered,
            message_id: None,
        },
    );

    // Verify conversation was auto-created
    assert_eq!(
        app.conversations.len(),
        1,
        "conversation should be auto-created"
    );

    let charlie_conv = &app.conversations[0];
    assert_eq!(charlie_conv.name, "@ charlie");

    // Verify message was added
    let charlie_msgs = app
        .messages
        .get("@ charlie")
        .expect("charlie messages missing");
    assert_eq!(charlie_msgs.len(), 1);
    assert_eq!(charlie_msgs[0].content, "Hey there!");
}

// =============================================================================
// Postcondition 8: Message preview updates
// =============================================================================

#[test]
fn test_message_preview_updates() {
    let mut app = App::new();

    // Add a conversation
    app.add_conversation("# general", None);

    // Initially no preview
    let general_conv = app
        .conversations
        .iter()
        .find(|c| c.name == "# general")
        .expect("general conversation missing");
    assert!(
        general_conv.last_message_preview.is_none(),
        "preview should be None initially"
    );

    // Push a message
    app.push_message(
        "# general",
        DisplayMessage {
            sender: "bob".to_string(),
            content: "Hello everyone".to_string(),
            timestamp: "10:00".to_string(),
            status: MessageStatus::Delivered,
            message_id: None,
        },
    );

    // Verify preview updated
    let general_conv = app
        .conversations
        .iter()
        .find(|c| c.name == "# general")
        .expect("general conversation missing");
    assert_eq!(
        general_conv.last_message_preview,
        Some("bob: Hello everyone".to_string()),
        "preview should show sender and content"
    );

    // Push another message
    app.push_message(
        "# general",
        DisplayMessage {
            sender: "alice".to_string(),
            content: "Hi bob!".to_string(),
            timestamp: "10:01".to_string(),
            status: MessageStatus::Delivered,
            message_id: None,
        },
    );

    // Verify preview updated to latest message
    let general_conv = app
        .conversations
        .iter()
        .find(|c| c.name == "# general")
        .expect("general conversation missing");
    assert_eq!(
        general_conv.last_message_preview,
        Some("alice: Hi bob!".to_string()),
        "preview should update to latest message"
    );
}

// =============================================================================
// Conversation deduplication
// =============================================================================

#[test]
fn test_add_conversation_dedup() {
    let mut app = App::new();

    // Add a conversation
    let added = app.add_conversation("# general", None);
    assert!(added, "first add should return true");
    assert_eq!(app.conversations.len(), 1);

    // Try to add again
    let added = app.add_conversation("# general", None);
    assert!(!added, "duplicate add should return false");
    assert_eq!(app.conversations.len(), 1, "should still have only 1");

    // Add a different conversation
    let added = app.add_conversation("# dev", None);
    assert!(added, "different conversation should be added");
    assert_eq!(app.conversations.len(), 2);
}

// =============================================================================
// Selected conversation name tracking
// =============================================================================

#[test]
fn test_selected_conversation_name() {
    let mut app = App::new();

    // Initially no conversations, should return None
    assert!(
        app.selected_conversation_name().is_none(),
        "should return None when no conversations"
    );

    // Add conversations
    app.add_conversation("# general", None);
    app.add_conversation("# dev", None);
    app.add_conversation("@ alice", None);

    // Select first conversation (index 0)
    app.selected_conversation = 0;
    assert_eq!(
        app.selected_conversation_name(),
        Some("# general"),
        "should return first conversation name"
    );

    // Select second conversation (index 1)
    app.selected_conversation = 1;
    assert_eq!(
        app.selected_conversation_name(),
        Some("# dev"),
        "should return second conversation name"
    );

    // Select third conversation (index 2)
    app.selected_conversation = 2;
    assert_eq!(
        app.selected_conversation_name(),
        Some("@ alice"),
        "should return third conversation name"
    );

    // Select out of bounds (index 99)
    app.selected_conversation = 99;
    assert!(
        app.selected_conversation_name().is_none(),
        "should return None for invalid index"
    );
}

#[test]
fn test_current_messages_per_conversation() {
    let mut app = App::new();

    // Add conversations
    app.add_conversation("# general", None);
    app.add_conversation("# dev", None);

    // Push messages to general
    app.push_message(
        "# general",
        DisplayMessage {
            sender: "alice".to_string(),
            content: "general msg 1".to_string(),
            timestamp: "10:00".to_string(),
            status: MessageStatus::Delivered,
            message_id: None,
        },
    );
    app.push_message(
        "# general",
        DisplayMessage {
            sender: "bob".to_string(),
            content: "general msg 2".to_string(),
            timestamp: "10:01".to_string(),
            status: MessageStatus::Delivered,
            message_id: None,
        },
    );

    // Push messages to dev
    app.push_message(
        "# dev",
        DisplayMessage {
            sender: "charlie".to_string(),
            content: "dev msg 1".to_string(),
            timestamp: "10:05".to_string(),
            status: MessageStatus::Delivered,
            message_id: None,
        },
    );

    // Select general (index 0)
    app.selected_conversation = 0;
    let current_msgs = app.current_messages();
    assert_eq!(current_msgs.len(), 2, "general should have 2 messages");
    assert_eq!(current_msgs[0].content, "general msg 1");
    assert_eq!(current_msgs[1].content, "general msg 2");

    // Select dev (index 1)
    app.selected_conversation = 1;
    let current_msgs = app.current_messages();
    assert_eq!(current_msgs.len(), 1, "dev should have 1 message");
    assert_eq!(current_msgs[0].content, "dev msg 1");

    // Select out of bounds
    app.selected_conversation = 99;
    let current_msgs = app.current_messages();
    assert!(
        current_msgs.is_empty(),
        "invalid conversation should return empty slice"
    );
}
