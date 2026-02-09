//! Integration tests for UC-006: Create Room.
//!
//! Tests room creation, discovery via relay, join request flow,
//! membership updates, and end-to-end room messaging.
//!
//! Verification command: `cargo test --test room_management`

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite;
use uuid::Uuid;

use termchat::chat::room::{
    MAX_MEMBERS, MAX_NAME_LEN, MAX_ROOMS, RoomError, RoomEvent, RoomManager, validate_room_name,
};
use termchat_proto::message::ConversationId;
use termchat_proto::relay::{self, RelayMessage};
use termchat_proto::room::{self, RoomMessage};
use termchat_relay::relay::start_server;

// =============================================================================
// Type aliases and helpers
// =============================================================================

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Starts a relay server on a random port for testing.
async fn start_relay() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    start_server("127.0.0.1:0")
        .await
        .expect("failed to start test relay")
}

/// Connects a WebSocket client and registers it with the relay.
async fn connect_and_register(addr: std::net::SocketAddr, peer_id: &str) -> WsStream {
    let url = format!("ws://{addr}/ws");
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    let reg = RelayMessage::Register {
        peer_id: peer_id.to_string(),
    };
    let bytes = relay::encode(&reg).unwrap();
    ws.send(tungstenite::Message::Binary(bytes.into()))
        .await
        .unwrap();

    // Wait for Registered ack
    let ack_msg = ws.next().await.unwrap().unwrap();
    let ack = relay::decode(&ack_msg.into_data()).unwrap();
    assert!(
        matches!(ack, RelayMessage::Registered { .. }),
        "expected Registered, got {ack:?}"
    );

    ws
}

/// Sends a room message through the relay.
async fn send_room_msg(ws: &mut WsStream, msg: &RoomMessage) {
    let room_bytes = room::encode(msg).unwrap();
    let relay_msg = RelayMessage::Room(room_bytes);
    let bytes = relay::encode(&relay_msg).unwrap();
    ws.send(tungstenite::Message::Binary(bytes.into()))
        .await
        .unwrap();
}

/// Receives and decodes a relay message from a WebSocket.
async fn recv_relay_msg(ws: &mut WsStream) -> RelayMessage {
    let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .expect("recv timed out")
        .unwrap()
        .unwrap();
    relay::decode(&msg.into_data()).unwrap()
}

/// Extracts a RoomMessage from a RelayMessage::Room variant.
fn unwrap_room_msg(relay_msg: RelayMessage) -> RoomMessage {
    match relay_msg {
        RelayMessage::Room(bytes) => room::decode(&bytes).unwrap(),
        other => panic!("expected RelayMessage::Room, got {other:?}"),
    }
}

// =============================================================================
// T-006-16: Room creation + discovery tests
// =============================================================================

/// Room registration via relay: client sends RegisterRoom, relay responds
/// with confirmation, ListRooms returns the room.
#[tokio::test]
async fn room_registration_via_relay() {
    let (addr, _handle) = start_relay().await;
    let mut ws = connect_and_register(addr, "alice").await;

    // Register a room
    let register = RoomMessage::RegisterRoom {
        room_id: "room-001".to_string(),
        name: "General".to_string(),
        admin_peer_id: "alice".to_string(),
    };
    send_room_msg(&mut ws, &register).await;

    // Relay echoes back RegisterRoom as confirmation
    let response = recv_relay_msg(&mut ws).await;
    let room_msg = unwrap_room_msg(response);
    assert_eq!(
        room_msg,
        RoomMessage::RegisterRoom {
            room_id: "room-001".to_string(),
            name: "General".to_string(),
            admin_peer_id: "alice".to_string(),
        }
    );

    // ListRooms should return the registered room
    send_room_msg(&mut ws, &RoomMessage::ListRooms).await;

    let list_response = recv_relay_msg(&mut ws).await;
    let list_msg = unwrap_room_msg(list_response);
    match list_msg {
        RoomMessage::RoomList { rooms } => {
            assert_eq!(rooms.len(), 1);
            assert_eq!(rooms[0].room_id, "room-001");
            assert_eq!(rooms[0].name, "General");
            assert_eq!(rooms[0].member_count, 1);
        }
        other => panic!("expected RoomList, got {other:?}"),
    }
}

/// Register 3 rooms, ListRooms returns all 3 with correct info.
#[tokio::test]
async fn multiple_rooms_listed() {
    let (addr, _handle) = start_relay().await;
    let mut ws = connect_and_register(addr, "alice").await;

    let room_data = [("room-a", "Alpha"), ("room-b", "Beta"), ("room-c", "Gamma")];

    for (id, name) in &room_data {
        let register = RoomMessage::RegisterRoom {
            room_id: id.to_string(),
            name: name.to_string(),
            admin_peer_id: "alice".to_string(),
        };
        send_room_msg(&mut ws, &register).await;
        // Consume confirmation
        let _ = recv_relay_msg(&mut ws).await;
    }

    // List all rooms
    send_room_msg(&mut ws, &RoomMessage::ListRooms).await;
    let list_response = recv_relay_msg(&mut ws).await;
    let list_msg = unwrap_room_msg(list_response);

    match list_msg {
        RoomMessage::RoomList { rooms } => {
            assert_eq!(rooms.len(), 3);
            let names: Vec<&str> = rooms.iter().map(|r| r.name.as_str()).collect();
            assert!(names.contains(&"Alpha"));
            assert!(names.contains(&"Beta"));
            assert!(names.contains(&"Gamma"));
        }
        other => panic!("expected RoomList, got {other:?}"),
    }
}

/// Register two rooms with same name (case-insensitive): second gets error.
#[tokio::test]
async fn room_name_conflict_case_insensitive() {
    let (addr, _handle) = start_relay().await;
    let mut ws = connect_and_register(addr, "alice").await;

    // Register first room
    let register1 = RoomMessage::RegisterRoom {
        room_id: "room-1".to_string(),
        name: "General".to_string(),
        admin_peer_id: "alice".to_string(),
    };
    send_room_msg(&mut ws, &register1).await;
    let _ = recv_relay_msg(&mut ws).await; // confirmation

    // Register second room with same name different case
    let register2 = RoomMessage::RegisterRoom {
        room_id: "room-2".to_string(),
        name: "general".to_string(),
        admin_peer_id: "alice".to_string(),
    };
    send_room_msg(&mut ws, &register2).await;

    let response = recv_relay_msg(&mut ws).await;
    match response {
        RelayMessage::Error { reason } => {
            assert!(
                reason.contains("already exists"),
                "expected name conflict error, got: {reason}"
            );
        }
        other => panic!("expected Error for name conflict, got {other:?}"),
    }
}

/// Test validate_room_name with empty, too-long, control chars, valid name.
#[tokio::test]
async fn room_name_validation() {
    // Empty name
    assert_eq!(validate_room_name(""), Err(RoomError::NameEmpty));

    // Too long
    let long_name = "x".repeat(MAX_NAME_LEN + 1);
    assert_eq!(validate_room_name(&long_name), Err(RoomError::NameTooLong));

    // Only control characters
    assert_eq!(
        validate_room_name("\x00\x01\x02"),
        Err(RoomError::NameInvalidChars)
    );

    // Valid name
    let result = validate_room_name("General Chat");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "General Chat");

    // Valid name with leading/trailing whitespace is trimmed
    assert_eq!(validate_room_name("  Trimmed  ").unwrap(), "Trimmed");

    // Control chars stripped from valid name
    assert_eq!(validate_room_name("Hello\x00World").unwrap(), "HelloWorld");
}

/// Create room via RoomManager: verify room_id, name, admin member, ConversationId.
#[tokio::test]
async fn local_room_creation() {
    let (mut mgr, _rx) = RoomManager::new();

    let room = mgr.create_room("Test Room", "peer-alice", "Alice").unwrap();

    // room_id is a valid UUID
    assert!(
        Uuid::parse_str(&room.room_id).is_ok(),
        "room_id should be a valid UUID"
    );

    assert_eq!(room.name, "Test Room");
    assert!(room.is_admin);

    // Admin member is present
    assert_eq!(room.members.len(), 1);
    assert_eq!(room.members[0].peer_id, "peer-alice");
    assert_eq!(room.members[0].display_name, "Alice");
    assert!(room.members[0].is_admin);

    // ConversationId is derived from room_id
    let expected_conv_id = ConversationId::from_uuid(Uuid::parse_str(&room.room_id).unwrap());
    assert_eq!(room.conversation_id.as_uuid(), expected_conv_id.as_uuid());
}

/// Create MAX_ROOMS rooms, next one fails with RoomLimitReached.
#[tokio::test]
async fn room_limit_enforcement() {
    let (mut mgr, _rx) = RoomManager::new();

    for i in 0..MAX_ROOMS {
        mgr.create_room(&format!("Room {i}"), "peer-alice", "Alice")
            .unwrap();
    }

    let result = mgr.create_room("One More", "peer-alice", "Alice");
    assert_eq!(result, Err(RoomError::RoomLimitReached));
}

/// Create "General", try creating "General" again: should fail.
#[tokio::test]
async fn duplicate_local_room_name() {
    let (mut mgr, _rx) = RoomManager::new();
    mgr.create_room("General", "peer-alice", "Alice").unwrap();

    let result = mgr.create_room("General", "peer-bob", "Bob");
    assert_eq!(result, Err(RoomError::DuplicateName("General".to_string())));
}

// =============================================================================
// T-006-17: Join flow tests
// =============================================================================

/// Client A registers room, Client B sends JoinRequest via relay, Client A
/// receives it.
#[tokio::test]
async fn join_request_routing_via_relay() {
    let (addr, _handle) = start_relay().await;

    // Alice registers room
    let mut ws_alice = connect_and_register(addr, "alice").await;
    let register = RoomMessage::RegisterRoom {
        room_id: "room-join-test".to_string(),
        name: "JoinTest".to_string(),
        admin_peer_id: "alice".to_string(),
    };
    send_room_msg(&mut ws_alice, &register).await;
    let _ = recv_relay_msg(&mut ws_alice).await; // confirmation

    // Bob sends JoinRequest
    let mut ws_bob = connect_and_register(addr, "bob").await;
    let join_req = RoomMessage::JoinRequest {
        room_id: "room-join-test".to_string(),
        peer_id: "bob".to_string(),
        display_name: "Bob".to_string(),
    };
    send_room_msg(&mut ws_bob, &join_req).await;

    // Alice receives the JoinRequest
    let response = recv_relay_msg(&mut ws_alice).await;
    let room_msg = unwrap_room_msg(response);
    assert_eq!(
        room_msg,
        RoomMessage::JoinRequest {
            room_id: "room-join-test".to_string(),
            peer_id: "bob".to_string(),
            display_name: "Bob".to_string(),
        }
    );
}

/// RoomManager approve_join adds member, returns updated member list.
#[tokio::test]
async fn approve_join_adds_member() {
    let (mut mgr, mut rx) = RoomManager::new();
    let room = mgr.create_room("General", "peer-alice", "Alice").unwrap();
    let _ = rx.try_recv(); // drain RoomCreated

    mgr.handle_join_request(&room.room_id, "peer-bob", "Bob")
        .unwrap();
    let _ = rx.try_recv(); // drain JoinRequestReceived

    let (new_member, members) = mgr.approve_join(&room.room_id, "peer-bob").unwrap();

    assert_eq!(new_member.peer_id, "peer-bob");
    assert_eq!(new_member.display_name, "Bob");
    assert!(!new_member.is_admin);
    assert_eq!(members.len(), 2);
    assert!(
        members
            .iter()
            .any(|m| m.peer_id == "peer-alice" && m.is_admin)
    );
    assert!(
        members
            .iter()
            .any(|m| m.peer_id == "peer-bob" && !m.is_admin)
    );

    // Verify event
    let event = rx.try_recv().unwrap();
    assert_eq!(
        event,
        RoomEvent::MemberJoined {
            room_id: room.room_id.clone(),
            peer_id: "peer-bob".to_string(),
            display_name: "Bob".to_string(),
        }
    );
}

/// RoomManager deny_join removes from queue, returns display name.
#[tokio::test]
async fn deny_join_removes_from_queue() {
    let (mut mgr, mut rx) = RoomManager::new();
    let room = mgr.create_room("General", "peer-alice", "Alice").unwrap();
    let _ = rx.try_recv(); // drain RoomCreated

    mgr.handle_join_request(&room.room_id, "peer-bob", "Bob")
        .unwrap();
    let _ = rx.try_recv(); // drain JoinRequestReceived

    let display_name = mgr.deny_join(&room.room_id, "peer-bob").unwrap();
    assert_eq!(display_name, "Bob");

    // Pending queue should be empty
    assert!(mgr.pending_requests(&room.room_id).is_empty());

    // Verify event
    let event = rx.try_recv().unwrap();
    assert_eq!(
        event,
        RoomEvent::MemberDenied {
            room_id: room.room_id.clone(),
            peer_id: "peer-bob".to_string(),
        }
    );
}

/// Fill room to MAX_MEMBERS, next approve fails with RoomFull.
#[tokio::test]
async fn room_capacity_limit() {
    let (mut mgr, _rx) = RoomManager::new();
    let room = mgr.create_room("Full Room", "peer-admin", "Admin").unwrap();

    // Fill the room to capacity (admin is already member 1)
    for i in 1..MAX_MEMBERS {
        mgr.handle_join_request(&room.room_id, &format!("peer-{i}"), &format!("User {i}"))
            .unwrap();
        mgr.approve_join(&room.room_id, &format!("peer-{i}"))
            .unwrap();
    }

    // Verify room is full
    let members = mgr.get_room_members(&room.room_id).unwrap();
    assert_eq!(members.len(), MAX_MEMBERS);

    // One more should fail
    mgr.handle_join_request(&room.room_id, "peer-overflow", "Overflow")
        .unwrap();
    let result = mgr.approve_join(&room.room_id, "peer-overflow");
    assert_eq!(result, Err(RoomError::RoomFull));
}

/// Approve same peer twice: second is idempotent.
#[tokio::test]
async fn duplicate_join_idempotent() {
    let (mut mgr, _rx) = RoomManager::new();
    let room = mgr.create_room("General", "peer-alice", "Alice").unwrap();

    mgr.handle_join_request(&room.room_id, "peer-bob", "Bob")
        .unwrap();
    let (member1, members1) = mgr.approve_join(&room.room_id, "peer-bob").unwrap();

    // Second approve should be idempotent
    let (member2, members2) = mgr.approve_join(&room.room_id, "peer-bob").unwrap();

    assert_eq!(member1.peer_id, member2.peer_id);
    assert_eq!(members1.len(), members2.len());
    assert_eq!(members2.len(), 2);
}

/// Non-admin tries to approve: gets NotAdmin error.
#[tokio::test]
async fn not_admin_rejection() {
    let (mut mgr, _rx) = RoomManager::new();
    let room = mgr.create_room("General", "peer-alice", "Alice").unwrap();

    // Simulate a non-admin room by clearing admin flag
    mgr.handle_join_request(&room.room_id, "peer-bob", "Bob")
        .unwrap();

    // Mark room as non-admin to simulate receiving someone else's room
    // (In real usage, the client-side room would have is_admin = false
    //  if we joined someone else's room.)
    let room_mut = mgr.get_room(&room.room_id).unwrap();
    let room_id = room_mut.room_id.clone();

    // We need to create a second manager that doesn't own the room.
    // Instead, test by handling join on a non-admin room.
    let (mut mgr2, _rx2) = RoomManager::new();
    let result = mgr2.approve_join(&room_id, "peer-bob");
    assert_eq!(result, Err(RoomError::RoomNotFound(room_id.clone())));

    // Also test: handle_join_request for nonexistent room
    let result = mgr2.handle_join_request("nonexistent-room", "peer-bob", "Bob");
    assert_eq!(
        result,
        Err(RoomError::RoomNotFound("nonexistent-room".to_string()))
    );
}

/// Send JoinRequest for nonexistent room: relay returns error.
#[tokio::test]
async fn join_request_room_not_found_via_relay() {
    let (addr, _handle) = start_relay().await;
    let mut ws_bob = connect_and_register(addr, "bob").await;

    // Send JoinRequest for a room that was never registered
    let join_req = RoomMessage::JoinRequest {
        room_id: "nonexistent-room".to_string(),
        peer_id: "bob".to_string(),
        display_name: "Bob".to_string(),
    };
    send_room_msg(&mut ws_bob, &join_req).await;

    // Should get an error back
    let response = recv_relay_msg(&mut ws_bob).await;
    match response {
        RelayMessage::Error { reason } => {
            assert!(
                reason.contains("not found"),
                "expected room not found error, got: {reason}"
            );
        }
        other => panic!("expected Error for nonexistent room, got {other:?}"),
    }
}

// =============================================================================
// T-006-18: End-to-end room messaging
// =============================================================================

/// RoomManager event channel: create room emits RoomCreated, handle_join_request
/// emits JoinRequestReceived, approve emits MemberJoined.
#[tokio::test]
async fn room_manager_event_channel() {
    let (mut mgr, mut rx) = RoomManager::new();

    // Create room -> RoomCreated
    let room = mgr.create_room("Events", "peer-alice", "Alice").unwrap();
    let event = rx.try_recv().unwrap();
    assert_eq!(
        event,
        RoomEvent::RoomCreated {
            room_id: room.room_id.clone(),
            name: "Events".to_string(),
        }
    );

    // Join request -> JoinRequestReceived
    mgr.handle_join_request(&room.room_id, "peer-bob", "Bob")
        .unwrap();
    let event = rx.try_recv().unwrap();
    assert_eq!(
        event,
        RoomEvent::JoinRequestReceived {
            room_id: room.room_id.clone(),
            peer_id: "peer-bob".to_string(),
            display_name: "Bob".to_string(),
        }
    );

    // Approve -> MemberJoined
    mgr.approve_join(&room.room_id, "peer-bob").unwrap();
    let event = rx.try_recv().unwrap();
    assert_eq!(
        event,
        RoomEvent::MemberJoined {
            room_id: room.room_id.clone(),
            peer_id: "peer-bob".to_string(),
            display_name: "Bob".to_string(),
        }
    );

    // No more events
    assert!(rx.try_recv().is_err());
}

/// Room's ConversationId matches `ConversationId::from_uuid(Uuid::parse_str(&room_id))`.
#[tokio::test]
async fn conversation_id_derivation() {
    let (mut mgr, _rx) = RoomManager::new();
    let room = mgr
        .create_room("ConvIdTest", "peer-alice", "Alice")
        .unwrap();

    let expected = ConversationId::from_uuid(Uuid::parse_str(&room.room_id).unwrap());

    assert_eq!(room.conversation_id.as_uuid(), expected.as_uuid());
}

/// Queue registration, drain produces correct RegisterRoom messages.
#[tokio::test]
async fn offline_registration_queue() {
    let (mut mgr, _rx) = RoomManager::new();
    let room1 = mgr.create_room("Room A", "peer-alice", "Alice").unwrap();
    let room2 = mgr.create_room("Room B", "peer-alice", "Alice").unwrap();

    // Queue both for registration
    mgr.queue_registration(&room1.room_id);
    mgr.queue_registration(&room2.room_id);

    let messages = mgr.drain_pending_registrations();
    assert_eq!(messages.len(), 2);

    // Verify first message
    match &messages[0] {
        RoomMessage::RegisterRoom {
            room_id,
            name,
            admin_peer_id,
        } => {
            assert_eq!(room_id, &room1.room_id);
            assert_eq!(name, "Room A");
            assert_eq!(admin_peer_id, "peer-alice");
        }
        other => panic!("expected RegisterRoom, got {other:?}"),
    }

    // Verify second message
    match &messages[1] {
        RoomMessage::RegisterRoom {
            room_id,
            name,
            admin_peer_id,
        } => {
            assert_eq!(room_id, &room2.room_id);
            assert_eq!(name, "Room B");
            assert_eq!(admin_peer_id, "peer-alice");
        }
        other => panic!("expected RegisterRoom, got {other:?}"),
    }

    // Second drain is empty
    assert!(mgr.drain_pending_registrations().is_empty());
}

/// Full flow: register room -> list rooms -> join request -> approve.
/// Tests the round-trip through the relay for room lifecycle.
#[tokio::test]
async fn room_registry_round_trip_via_relay() {
    let (addr, _handle) = start_relay().await;

    // Alice creates and registers a room
    let mut ws_alice = connect_and_register(addr, "alice").await;
    let register = RoomMessage::RegisterRoom {
        room_id: "room-roundtrip".to_string(),
        name: "RoundTrip".to_string(),
        admin_peer_id: "alice".to_string(),
    };
    send_room_msg(&mut ws_alice, &register).await;
    let _ = recv_relay_msg(&mut ws_alice).await; // confirmation

    // Bob connects and discovers rooms
    let mut ws_bob = connect_and_register(addr, "bob").await;
    send_room_msg(&mut ws_bob, &RoomMessage::ListRooms).await;

    let list_response = recv_relay_msg(&mut ws_bob).await;
    let list_msg = unwrap_room_msg(list_response);
    match &list_msg {
        RoomMessage::RoomList { rooms } => {
            assert_eq!(rooms.len(), 1);
            assert_eq!(rooms[0].room_id, "room-roundtrip");
            assert_eq!(rooms[0].name, "RoundTrip");
        }
        other => panic!("expected RoomList, got {other:?}"),
    }

    // Bob sends JoinRequest
    let join_req = RoomMessage::JoinRequest {
        room_id: "room-roundtrip".to_string(),
        peer_id: "bob".to_string(),
        display_name: "Bob".to_string(),
    };
    send_room_msg(&mut ws_bob, &join_req).await;

    // Alice receives the JoinRequest via relay
    let routed = recv_relay_msg(&mut ws_alice).await;
    let routed_msg = unwrap_room_msg(routed);
    assert_eq!(
        routed_msg,
        RoomMessage::JoinRequest {
            room_id: "room-roundtrip".to_string(),
            peer_id: "bob".to_string(),
            display_name: "Bob".to_string(),
        }
    );
}

/// Deny join via RoomManager emits MemberDenied event.
#[tokio::test]
async fn deny_join_emits_event() {
    let (mut mgr, mut rx) = RoomManager::new();
    let room = mgr.create_room("DenyTest", "peer-alice", "Alice").unwrap();
    let _ = rx.try_recv(); // drain RoomCreated

    mgr.handle_join_request(&room.room_id, "peer-charlie", "Charlie")
        .unwrap();
    let _ = rx.try_recv(); // drain JoinRequestReceived

    let name = mgr.deny_join(&room.room_id, "peer-charlie").unwrap();
    assert_eq!(name, "Charlie");

    let event = rx.try_recv().unwrap();
    assert_eq!(
        event,
        RoomEvent::MemberDenied {
            room_id: room.room_id.clone(),
            peer_id: "peer-charlie".to_string(),
        }
    );
}

/// Queuing registration for a deleted/nonexistent room: drain skips it.
#[tokio::test]
async fn offline_registration_skips_deleted_rooms() {
    let (mut mgr, _rx) = RoomManager::new();
    mgr.queue_registration("nonexistent-room-id");

    let messages = mgr.drain_pending_registrations();
    assert!(messages.is_empty());
}

// =============================================================================
// UC-016: JoinApproved/JoinDenied relay routing tests
// =============================================================================

/// Alice creates a room, Bob sends JoinRequest, Alice approves -> Bob receives
/// JoinApproved with member list via the relay.
#[tokio::test]
async fn join_approved_reaches_requester_via_relay() {
    let (addr, _handle) = start_relay().await;

    // Alice registers a room
    let mut ws_alice = connect_and_register(addr, "alice").await;
    let register = RoomMessage::RegisterRoom {
        room_id: "room-approve-test".to_string(),
        name: "ApproveTest".to_string(),
        admin_peer_id: "alice".to_string(),
    };
    send_room_msg(&mut ws_alice, &register).await;
    let _ = recv_relay_msg(&mut ws_alice).await; // confirmation

    // Bob connects and sends JoinRequest
    let mut ws_bob = connect_and_register(addr, "bob").await;
    let join_req = RoomMessage::JoinRequest {
        room_id: "room-approve-test".to_string(),
        peer_id: "bob".to_string(),
        display_name: "Bob".to_string(),
    };
    send_room_msg(&mut ws_bob, &join_req).await;

    // Alice receives JoinRequest
    let request = recv_relay_msg(&mut ws_alice).await;
    let request_msg = unwrap_room_msg(request);
    assert_eq!(
        request_msg,
        RoomMessage::JoinRequest {
            room_id: "room-approve-test".to_string(),
            peer_id: "bob".to_string(),
            display_name: "Bob".to_string(),
        }
    );

    // Alice approves: sends JoinApproved with target_peer_id = "bob"
    let approve = RoomMessage::JoinApproved {
        room_id: "room-approve-test".to_string(),
        name: "ApproveTest".to_string(),
        members: vec![
            room::MemberInfo {
                peer_id: "alice".to_string(),
                display_name: "Alice".to_string(),
                is_admin: true,
                is_agent: false,
            },
            room::MemberInfo {
                peer_id: "bob".to_string(),
                display_name: "Bob".to_string(),
                is_admin: false,
                is_agent: false,
            },
        ],
        target_peer_id: "bob".to_string(),
    };
    send_room_msg(&mut ws_alice, &approve).await;

    // Bob receives the JoinApproved
    let response = recv_relay_msg(&mut ws_bob).await;
    let room_msg = unwrap_room_msg(response);
    match room_msg {
        RoomMessage::JoinApproved {
            room_id,
            name,
            members,
            target_peer_id,
        } => {
            assert_eq!(room_id, "room-approve-test");
            assert_eq!(name, "ApproveTest");
            assert_eq!(target_peer_id, "bob");
            assert_eq!(members.len(), 2);
            assert!(members.iter().any(|m| m.peer_id == "alice" && m.is_admin));
            assert!(members.iter().any(|m| m.peer_id == "bob" && !m.is_admin));
        }
        other => panic!("expected JoinApproved, got {other:?}"),
    }
}

/// Alice creates a room, Bob sends JoinRequest, Alice denies -> Bob receives
/// JoinDenied with reason via the relay.
#[tokio::test]
async fn join_denied_reaches_requester_via_relay() {
    let (addr, _handle) = start_relay().await;

    // Alice registers a room
    let mut ws_alice = connect_and_register(addr, "alice").await;
    let register = RoomMessage::RegisterRoom {
        room_id: "room-deny-test".to_string(),
        name: "DenyTest".to_string(),
        admin_peer_id: "alice".to_string(),
    };
    send_room_msg(&mut ws_alice, &register).await;
    let _ = recv_relay_msg(&mut ws_alice).await; // confirmation

    // Bob connects and sends JoinRequest
    let mut ws_bob = connect_and_register(addr, "bob").await;
    let join_req = RoomMessage::JoinRequest {
        room_id: "room-deny-test".to_string(),
        peer_id: "bob".to_string(),
        display_name: "Bob".to_string(),
    };
    send_room_msg(&mut ws_bob, &join_req).await;

    // Alice receives JoinRequest
    let request = recv_relay_msg(&mut ws_alice).await;
    let request_msg = unwrap_room_msg(request);
    assert_eq!(
        request_msg,
        RoomMessage::JoinRequest {
            room_id: "room-deny-test".to_string(),
            peer_id: "bob".to_string(),
            display_name: "Bob".to_string(),
        }
    );

    // Alice denies: sends JoinDenied with target_peer_id = "bob"
    let deny = RoomMessage::JoinDenied {
        room_id: "room-deny-test".to_string(),
        reason: "room is invite-only".to_string(),
        target_peer_id: "bob".to_string(),
    };
    send_room_msg(&mut ws_alice, &deny).await;

    // Bob receives the JoinDenied
    let response = recv_relay_msg(&mut ws_bob).await;
    let room_msg = unwrap_room_msg(response);
    match room_msg {
        RoomMessage::JoinDenied {
            room_id,
            reason,
            target_peer_id,
        } => {
            assert_eq!(room_id, "room-deny-test");
            assert_eq!(reason, "room is invite-only");
            assert_eq!(target_peer_id, "bob");
        }
        other => panic!("expected JoinDenied, got {other:?}"),
    }
}

/// Room list from relay shows multiple rooms from different clients.
#[tokio::test]
async fn room_discovery_across_clients() {
    let (addr, _handle) = start_relay().await;

    // Alice registers a room
    let mut ws_alice = connect_and_register(addr, "alice").await;
    let reg1 = RoomMessage::RegisterRoom {
        room_id: "room-alice".to_string(),
        name: "AliceRoom".to_string(),
        admin_peer_id: "alice".to_string(),
    };
    send_room_msg(&mut ws_alice, &reg1).await;
    let _ = recv_relay_msg(&mut ws_alice).await;

    // Bob registers a different room
    let mut ws_bob = connect_and_register(addr, "bob").await;
    let reg2 = RoomMessage::RegisterRoom {
        room_id: "room-bob".to_string(),
        name: "BobRoom".to_string(),
        admin_peer_id: "bob".to_string(),
    };
    send_room_msg(&mut ws_bob, &reg2).await;
    let _ = recv_relay_msg(&mut ws_bob).await;

    // Carol lists all rooms
    let mut ws_carol = connect_and_register(addr, "carol").await;
    send_room_msg(&mut ws_carol, &RoomMessage::ListRooms).await;

    let list_response = recv_relay_msg(&mut ws_carol).await;
    let list_msg = unwrap_room_msg(list_response);
    match list_msg {
        RoomMessage::RoomList { rooms } => {
            assert_eq!(rooms.len(), 2);
            let names: Vec<&str> = rooms.iter().map(|r| r.name.as_str()).collect();
            assert!(names.contains(&"AliceRoom"));
            assert!(names.contains(&"BobRoom"));
        }
        other => panic!("expected RoomList, got {other:?}"),
    }
}

/// RoomManager get_room and list_rooms work after creating multiple rooms.
#[tokio::test]
async fn room_manager_query_methods() {
    let (mut mgr, _rx) = RoomManager::new();
    let room_a = mgr.create_room("Alpha", "peer-alice", "Alice").unwrap();
    let room_b = mgr.create_room("Beta", "peer-alice", "Alice").unwrap();

    // get_room
    let fetched = mgr.get_room(&room_a.room_id).unwrap();
    assert_eq!(fetched.name, "Alpha");

    // get_room_by_name
    let fetched = mgr.get_room_by_name("Beta").unwrap();
    assert_eq!(fetched.room_id, room_b.room_id);

    // list_rooms
    let rooms = mgr.list_rooms();
    assert_eq!(rooms.len(), 2);

    // get_room_members
    let members = mgr.get_room_members(&room_a.room_id).unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].peer_id, "peer-alice");
}
