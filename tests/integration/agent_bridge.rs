//! Integration tests for UC-007: Join Room as Agent Participant.
//!
//! Tests the agent bridge lifecycle, handshake protocol, message
//! fan-out, room event forwarding, and disconnect cleanup.
//!
//! Verification command: `cargo test --test agent_bridge`

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

use termchat::agent::AgentError;
use termchat::agent::bridge::{AgentBridge, AgentConnection, HeartbeatConfig, heartbeat_loop};
use termchat::agent::participant::{AgentParticipant, DisconnectReason, RoomEvent};
use termchat::agent::protocol::{
    AgentMessage, BridgeHistoryEntry, BridgeMemberInfo, BridgeMessage, PROTOCOL_VERSION,
    decode_line, encode_line,
};

// =============================================================================
// Test helpers
// =============================================================================

/// Monotonic counter to avoid socket path collisions across parallel tests.
static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Creates a unique temporary socket path for each test, avoiding collisions
/// when tests run in parallel.
fn temp_socket_path(name: &str) -> PathBuf {
    let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join("termchat-integ-agent");
    dir.join(format!("{name}-{}-{n}.sock", std::process::id()))
}

/// Sets up an `AgentBridge` listening on a unique socket, then connects a
/// mock agent client and returns both sides. The bridge also returns so
/// the caller controls its lifetime (drop cleans up the socket).
async fn setup_agent_bridge(
    test_name: &str,
    room_id: &str,
) -> (
    AgentBridge,
    AgentConnection,
    BufReader<tokio::net::unix::OwnedReadHalf>,
    BufWriter<tokio::net::unix::OwnedWriteHalf>,
) {
    let path = temp_socket_path(test_name);
    let bridge = AgentBridge::start(&path, room_id).expect("start bridge");

    let client_path = path.clone();
    let client_handle = tokio::spawn(async move {
        let stream = UnixStream::connect(&client_path)
            .await
            .expect("connect to bridge");
        let (read_half, write_half) = stream.into_split();
        (BufReader::new(read_half), BufWriter::new(write_half))
    });

    let conn = bridge.accept_connection().await.expect("accept connection");
    let (reader, writer) = client_handle.await.expect("client connected");
    (bridge, conn, reader, writer)
}

/// Connects a second mock agent to an existing bridge socket path.
async fn connect_mock_agent(
    path: &std::path::Path,
) -> (
    BufReader<tokio::net::unix::OwnedReadHalf>,
    BufWriter<tokio::net::unix::OwnedWriteHalf>,
) {
    let stream = UnixStream::connect(path).await.expect("connect mock agent");
    let (read_half, write_half) = stream.into_split();
    (BufReader::new(read_half), BufWriter::new(write_half))
}

/// Writes a JSON line message from the mock agent side.
async fn send_json_line(
    writer: &mut BufWriter<tokio::net::unix::OwnedWriteHalf>,
    msg: &AgentMessage,
) {
    let line = encode_line(msg).expect("encode agent message");
    writer
        .write_all(line.as_bytes())
        .await
        .expect("write to bridge");
    writer.flush().await.expect("flush to bridge");
}

/// Reads a single JSON line message on the mock agent side.
async fn read_json_line(reader: &mut BufReader<tokio::net::unix::OwnedReadHalf>) -> BridgeMessage {
    let mut line = String::new();
    reader.read_line(&mut line).await.expect("read from bridge");
    decode_line(&line).expect("decode bridge message")
}

/// Reads a JSON line with a timeout. Returns `None` if the timeout fires or EOF.
async fn read_json_line_timeout(
    reader: &mut BufReader<tokio::net::unix::OwnedReadHalf>,
    timeout: Duration,
) -> Option<BridgeMessage> {
    let mut line = String::new();
    match tokio::time::timeout(timeout, reader.read_line(&mut line)).await {
        Ok(Ok(n)) if n > 0 => Some(decode_line(&line).expect("decode")),
        _ => None,
    }
}

/// Performs a complete Hello/Welcome handshake from the mock agent side,
/// then returns the bridge-side `AgentConnection` (with handshake done).
/// The mock agent reader will have the Welcome message already consumed.
async fn do_handshake_on_conn(
    conn: &mut AgentConnection,
    writer: &mut BufWriter<tokio::net::unix::OwnedWriteHalf>,
    reader: &mut BufReader<tokio::net::unix::OwnedReadHalf>,
    agent_id: &str,
    display_name: &str,
) -> BridgeMessage {
    let hello = AgentMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
        agent_id: agent_id.to_string(),
        display_name: display_name.to_string(),
        capabilities: vec!["chat".to_string()],
    };
    send_json_line(writer, &hello).await;

    let _hs = conn
        .perform_handshake("General", &[], &[], &[], 256)
        .await
        .expect("handshake");

    read_json_line(reader).await
}

// =============================================================================
// Bridge lifecycle tests
// =============================================================================

#[tokio::test]
async fn bridge_socket_created_and_cleaned_on_drop() {
    let path = temp_socket_path("lifecycle-create");
    {
        let _bridge = AgentBridge::start(&path, "room-1").expect("start");
        assert!(path.exists(), "socket file should exist after start");
    }
    assert!(!path.exists(), "socket file should be removed after drop");
}

#[tokio::test]
async fn bridge_stale_socket_replaced() {
    let path = temp_socket_path("lifecycle-stale");
    // Create first bridge and leak it (simulates stale socket)
    let bridge1 = AgentBridge::start(&path, "room-1").expect("start");
    std::mem::forget(bridge1);

    // Second start should succeed by cleaning up stale socket
    let bridge2 = AgentBridge::start(&path, "room-1").expect("second start");
    assert!(path.exists());
    drop(bridge2);
}

#[tokio::test]
async fn bridge_accept_timeout_fires() {
    let path = temp_socket_path("lifecycle-timeout");
    let bridge = AgentBridge::start(&path, "room-1").expect("start");

    let result = bridge
        .accept_connection_with_timeout(Duration::from_millis(100))
        .await;
    assert!(
        matches!(result, Err(AgentError::Timeout)),
        "expected Timeout error"
    );
    // Socket cleaned up after timeout
    assert!(!path.exists(), "socket should be removed after timeout");
}

#[tokio::test]
async fn bridge_accept_single_rejects_second_client() {
    let path = temp_socket_path("lifecycle-multi");
    let mut bridge = AgentBridge::start(&path, "room-1").expect("start");

    // First client connects
    let client_path = path.clone();
    let first = tokio::spawn(async move {
        let _stream = UnixStream::connect(&client_path)
            .await
            .expect("first connect");
        tokio::time::sleep(Duration::from_millis(300)).await;
    });

    let (_conn, reject_handle) = bridge
        .accept_single_connection()
        .await
        .expect("accept single");

    // Second client connects — should get an error
    tokio::time::sleep(Duration::from_millis(50)).await;
    let (mut reader, _writer) = connect_mock_agent(&path).await;
    let msg = read_json_line(&mut reader).await;
    match msg {
        BridgeMessage::Error { code, .. } => {
            assert_eq!(code, "already_connected");
        }
        other => panic!("expected already_connected Error, got {other:?}"),
    }

    reject_handle.abort();
    first.abort();
}

#[tokio::test]
async fn bridge_shutdown_removes_socket() {
    let path = temp_socket_path("lifecycle-shutdown");
    let mut bridge = AgentBridge::start(&path, "room-1").expect("start");
    assert!(path.exists());
    bridge.shutdown();
    assert!(!path.exists());
}

#[tokio::test]
async fn bridge_room_id_and_socket_path_accessors() {
    let path = temp_socket_path("lifecycle-accessors");
    let bridge = AgentBridge::start(&path, "my-room-42").expect("start");
    assert_eq!(bridge.room_id(), "my-room-42");
    assert_eq!(bridge.socket_path(), path);
    drop(bridge);
}

// =============================================================================
// Handshake protocol tests
// =============================================================================

#[tokio::test]
async fn handshake_success_returns_welcome_with_members_and_history() {
    let (_bridge, mut conn, mut reader, mut writer) = setup_agent_bridge("hs-ok", "room-abc").await;

    let members = vec![BridgeMemberInfo {
        peer_id: "peer-alice".to_string(),
        display_name: "Alice".to_string(),
        is_admin: true,
        is_agent: false,
    }];
    let history = vec![BridgeHistoryEntry {
        sender_id: "peer-alice".to_string(),
        sender_name: "Alice".to_string(),
        content: "hello".to_string(),
        timestamp: "2025-01-15T00:00:00Z".to_string(),
    }];

    let hello = AgentMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
        agent_id: "claude-42".to_string(),
        display_name: "Claude".to_string(),
        capabilities: vec!["chat".to_string(), "code_review".to_string()],
    };
    send_json_line(&mut writer, &hello).await;

    let hs = conn
        .perform_handshake(
            "General",
            &members,
            &history,
            &["peer-alice".to_string()],
            256,
        )
        .await
        .expect("handshake");

    // Verify handshake result
    assert_eq!(hs.agent_id, "claude-42");
    assert_eq!(hs.display_name, "Claude");
    assert_eq!(hs.peer_id, "agent:claude-42");
    assert_eq!(hs.capabilities, vec!["chat", "code_review"]);

    // Verify client received Welcome with correct data
    let welcome = read_json_line(&mut reader).await;
    match welcome {
        BridgeMessage::Welcome {
            room_id,
            room_name,
            members: m,
            history: h,
        } => {
            assert_eq!(room_id, "room-abc");
            assert_eq!(room_name, "General");
            assert_eq!(m.len(), 1);
            assert_eq!(m[0].peer_id, "peer-alice");
            assert!(m[0].is_admin);
            assert!(!m[0].is_agent);
            assert_eq!(h.len(), 1);
            assert_eq!(h[0].content, "hello");
        }
        other => panic!("expected Welcome, got {other:?}"),
    }
}

#[tokio::test]
async fn handshake_bad_version_sends_error_to_agent() {
    let (_bridge, mut conn, mut reader, mut writer) =
        setup_agent_bridge("hs-badver", "room-1").await;

    let hello = AgentMessage::Hello {
        protocol_version: 999,
        agent_id: "bot".to_string(),
        display_name: "Bot".to_string(),
        capabilities: vec![],
    };
    send_json_line(&mut writer, &hello).await;

    let result = conn.perform_handshake("Room", &[], &[], &[], 256).await;
    assert!(matches!(result, Err(AgentError::ProtocolError(_))));

    let resp = read_json_line(&mut reader).await;
    match resp {
        BridgeMessage::Error { code, .. } => assert_eq!(code, "unsupported_version"),
        other => panic!("expected unsupported_version Error, got {other:?}"),
    }
}

#[tokio::test]
async fn handshake_invalid_agent_id_sends_error_to_agent() {
    let (_bridge, mut conn, mut reader, mut writer) =
        setup_agent_bridge("hs-badid", "room-1").await;

    let hello = AgentMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
        agent_id: "   ".to_string(), // whitespace only -> invalid
        display_name: "Bot".to_string(),
        capabilities: vec![],
    };
    send_json_line(&mut writer, &hello).await;

    let result = conn.perform_handshake("Room", &[], &[], &[], 256).await;
    assert!(matches!(result, Err(AgentError::InvalidAgentId(_))));

    let resp = read_json_line(&mut reader).await;
    match resp {
        BridgeMessage::Error { code, .. } => assert_eq!(code, "invalid_agent_id"),
        other => panic!("expected invalid_agent_id Error, got {other:?}"),
    }
}

#[tokio::test]
async fn handshake_malformed_json_sends_error_to_agent() {
    let path = temp_socket_path("hs-malformed");
    let bridge = AgentBridge::start(&path, "room-1").expect("start");

    let client_path = path.clone();
    let client = tokio::spawn(async move {
        let stream = UnixStream::connect(&client_path).await.expect("connect");
        let (read_half, write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        let mut writer = BufWriter::new(write_half);

        writer.write_all(b"NOT JSON AT ALL\n").await.expect("write");
        writer.flush().await.expect("flush");

        let mut resp_line = String::new();
        let n = reader.read_line(&mut resp_line).await.expect("read");
        if n == 0 {
            return None;
        }
        Some(decode_line::<BridgeMessage>(&resp_line).expect("decode"))
    });

    let mut conn = bridge.accept_connection().await.expect("accept");
    let result = conn.perform_handshake("Room", &[], &[], &[], 256).await;
    assert!(matches!(result, Err(AgentError::ProtocolError(_))));

    let resp = client.await.expect("join").expect("response");
    match resp {
        BridgeMessage::Error { code, .. } => assert_eq!(code, "invalid_hello"),
        other => panic!("expected invalid_hello Error, got {other:?}"),
    }
    drop(bridge);
}

#[tokio::test]
async fn handshake_not_hello_first_message_sends_error() {
    let (_bridge, mut conn, mut reader, mut writer) =
        setup_agent_bridge("hs-nothello", "room-1").await;

    // Send Pong instead of Hello
    send_json_line(&mut writer, &AgentMessage::Pong).await;

    let result = conn.perform_handshake("Room", &[], &[], &[], 256).await;
    assert!(matches!(result, Err(AgentError::ProtocolError(_))));

    let resp = read_json_line(&mut reader).await;
    match resp {
        BridgeMessage::Error { code, .. } => assert_eq!(code, "invalid_hello"),
        other => panic!("expected invalid_hello Error, got {other:?}"),
    }
}

#[tokio::test]
async fn handshake_room_full_sends_error_to_agent() {
    let (_bridge, mut conn, mut reader, _writer) = setup_agent_bridge("hs-full", "room-1").await;

    // Create members at capacity (256)
    let members: Vec<BridgeMemberInfo> = (0..256)
        .map(|i| BridgeMemberInfo {
            peer_id: format!("peer-{i}"),
            display_name: format!("User {i}"),
            is_admin: i == 0,
            is_agent: false,
        })
        .collect();

    // Room is full — the handshake rejects before reading Hello
    let result = conn
        .perform_handshake("Room", &members, &[], &[], 256)
        .await;
    assert!(matches!(result, Err(AgentError::RoomFull)));

    let resp = read_json_line(&mut reader).await;
    match resp {
        BridgeMessage::Error { code, .. } => assert_eq!(code, "room_full"),
        other => panic!("expected room_full Error, got {other:?}"),
    }
}

#[tokio::test]
async fn handshake_peer_id_collision_appends_suffix() {
    let (_bridge, mut conn, mut reader, mut writer) =
        setup_agent_bridge("hs-collision", "room-1").await;

    let existing = vec!["agent:claude".to_string()];

    let hello = AgentMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
        agent_id: "claude".to_string(),
        display_name: "Claude".to_string(),
        capabilities: vec![],
    };
    send_json_line(&mut writer, &hello).await;

    let hs = conn
        .perform_handshake("Room", &[], &[], &existing, 256)
        .await
        .expect("handshake");
    assert_eq!(hs.peer_id, "agent:claude-2");

    let _welcome = read_json_line(&mut reader).await;
}

#[tokio::test]
async fn handshake_empty_history_sends_empty_arrays() {
    let (_bridge, mut conn, mut reader, mut writer) =
        setup_agent_bridge("hs-empty", "room-1").await;

    let welcome = do_handshake_on_conn(&mut conn, &mut writer, &mut reader, "bot", "Bot").await;
    match welcome {
        BridgeMessage::Welcome {
            members, history, ..
        } => {
            assert!(members.is_empty());
            assert!(history.is_empty());
        }
        other => panic!("expected Welcome, got {other:?}"),
    }
}

#[tokio::test]
async fn graceful_disconnect_via_goodbye() {
    let (_bridge, mut conn, mut reader, mut writer) =
        setup_agent_bridge("disconnect-good", "room-1").await;

    let _ = do_handshake_on_conn(&mut conn, &mut writer, &mut reader, "bot", "Bot").await;

    // Agent sends Goodbye
    send_json_line(&mut writer, &AgentMessage::Goodbye).await;

    let msg = conn.read_message().await.expect("read goodbye");
    assert!(matches!(msg, AgentMessage::Goodbye));
}

#[tokio::test]
async fn ungraceful_disconnect_eof() {
    let (_bridge, mut conn, _reader, writer) = setup_agent_bridge("disconnect-eof", "room-1").await;

    // Drop the writer to simulate a broken pipe
    drop(writer);

    // Reading should return ConnectionClosed
    let result = conn.read_message().await;
    assert!(matches!(result, Err(AgentError::ConnectionClosed)));
}

// =============================================================================
// Heartbeat tests
// =============================================================================

#[tokio::test]
async fn heartbeat_pong_received_keeps_alive() {
    let (ping_tx, mut ping_rx) = mpsc::channel::<BridgeMessage>(8);
    let (pong_tx, pong_rx) = mpsc::channel::<()>(8);

    let config = HeartbeatConfig {
        ping_interval: Duration::from_millis(50),
        pong_timeout: Duration::from_millis(200),
    };

    let hb = tokio::spawn(heartbeat_loop(ping_tx, pong_rx, config));

    // Wait for first Ping
    let msg = ping_rx.recv().await.expect("ping 1");
    assert!(matches!(msg, BridgeMessage::Ping));

    // Send Pong
    pong_tx.send(()).await.expect("pong 1");

    // Wait for second Ping (timer reset — proves first pong was processed)
    let msg = ping_rx.recv().await.expect("ping 2");
    assert!(matches!(msg, BridgeMessage::Ping));

    // Clean shutdown by dropping pong sender
    drop(pong_tx);
    let _ = ping_rx.recv().await;

    let result = hb.await.expect("join");
    assert!(result.is_ok());
}

#[tokio::test]
async fn heartbeat_missing_pong_triggers_timeout() {
    let (ping_tx, mut ping_rx) = mpsc::channel::<BridgeMessage>(8);
    let (_pong_tx, pong_rx) = mpsc::channel::<()>(8);

    let config = HeartbeatConfig {
        ping_interval: Duration::from_millis(50),
        pong_timeout: Duration::from_millis(100),
    };

    let hb = tokio::spawn(heartbeat_loop(ping_tx, pong_rx, config));

    // Wait for Ping but do NOT send Pong
    let msg = ping_rx.recv().await.expect("ping");
    assert!(matches!(msg, BridgeMessage::Ping));

    let result = hb.await.expect("join");
    assert!(matches!(result, Err(AgentError::Timeout)));
}

#[tokio::test]
async fn heartbeat_stops_when_ping_channel_closes() {
    let (ping_tx, ping_rx) = mpsc::channel::<BridgeMessage>(8);
    let (_pong_tx, pong_rx) = mpsc::channel::<()>(8);

    let config = HeartbeatConfig {
        ping_interval: Duration::from_millis(50),
        pong_timeout: Duration::from_millis(200),
    };

    let hb = tokio::spawn(heartbeat_loop(ping_tx, pong_rx, config));

    // Drop the ping receiver — simulates connection close
    drop(ping_rx);

    let result = hb.await.expect("join");
    assert!(matches!(result, Err(AgentError::ConnectionClosed)));
}

// =============================================================================
// AgentParticipant tests — message send / receive via run() event loop
// =============================================================================

#[tokio::test]
async fn participant_send_message_via_run_loop() {
    let (_bridge, mut conn, mut reader, mut writer) =
        setup_agent_bridge("part-send-ok", "room-1").await;

    // Handshake first, then create participant
    let welcome =
        do_handshake_on_conn(&mut conn, &mut writer, &mut reader, "claude", "Claude").await;
    assert!(matches!(welcome, BridgeMessage::Welcome { .. }));

    let (outbound_tx, mut outbound_rx) = mpsc::channel(64);
    let (_room_tx, room_rx) = mpsc::channel(64);

    let mut participant = AgentParticipant::new(
        conn,
        "room-1",
        "agent:claude",
        "Claude",
        outbound_tx,
        room_rx,
    );
    participant.mark_ready();

    let run_handle = tokio::spawn(async move { participant.run().await });

    // Agent sends a message
    let msg = AgentMessage::SendMessage {
        content: "Hello from agent!".to_string(),
    };
    send_json_line(&mut writer, &msg).await;

    // Verify it appeared on the outbound channel
    let outbound = tokio::time::timeout(Duration::from_millis(500), outbound_rx.recv())
        .await
        .expect("timeout")
        .expect("outbound");
    assert_eq!(outbound.room_id, "room-1");
    assert_eq!(outbound.sender_peer_id, "agent:claude");
    assert_eq!(outbound.sender_display_name, "Claude");
    assert_eq!(outbound.content, "Hello from agent!");

    // Graceful shutdown
    send_json_line(&mut writer, &AgentMessage::Goodbye).await;
    let cleanup = tokio::time::timeout(Duration::from_millis(500), run_handle)
        .await
        .expect("timeout")
        .expect("join");
    assert_eq!(cleanup.reason, DisconnectReason::Goodbye);
}

#[tokio::test]
async fn participant_not_ready_rejects_send() {
    let (_bridge, mut conn, mut reader, mut writer) =
        setup_agent_bridge("part-not-ready", "room-1").await;

    let welcome = do_handshake_on_conn(&mut conn, &mut writer, &mut reader, "bot", "Bot").await;
    assert!(matches!(welcome, BridgeMessage::Welcome { .. }));

    let (outbound_tx, _outbound_rx) = mpsc::channel(64);
    let (_room_tx, room_rx) = mpsc::channel(64);

    // NOT marked ready
    let participant =
        AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);

    let run_handle = tokio::spawn(async move { participant.run().await });

    // Agent tries to send a message
    let msg = AgentMessage::SendMessage {
        content: "hello".to_string(),
    };
    send_json_line(&mut writer, &msg).await;

    // Should get a not_ready error back
    let resp = read_json_line_timeout(&mut reader, Duration::from_millis(500))
        .await
        .expect("response");
    match resp {
        BridgeMessage::Error { code, .. } => assert_eq!(code, "not_ready"),
        other => panic!("expected not_ready error, got {other:?}"),
    }

    // Clean up
    send_json_line(&mut writer, &AgentMessage::Goodbye).await;
    let cleanup = tokio::time::timeout(Duration::from_millis(500), run_handle)
        .await
        .expect("timeout")
        .expect("join");
    assert_eq!(cleanup.reason, DisconnectReason::Goodbye);
}

#[tokio::test]
async fn participant_oversized_message_rejected() {
    let (_bridge, mut conn, mut reader, mut writer) =
        setup_agent_bridge("part-oversize", "room-1").await;

    let _ = do_handshake_on_conn(&mut conn, &mut writer, &mut reader, "bot", "Bot").await;

    let (outbound_tx, _outbound_rx) = mpsc::channel(64);
    let (_room_tx, room_rx) = mpsc::channel(64);

    let mut participant =
        AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);
    participant.mark_ready();

    let run_handle = tokio::spawn(async move { participant.run().await });

    // 64KB + 1 byte
    let big_content = "x".repeat(64 * 1024 + 1);
    let msg = AgentMessage::SendMessage {
        content: big_content,
    };
    send_json_line(&mut writer, &msg).await;

    let resp = read_json_line_timeout(&mut reader, Duration::from_millis(500))
        .await
        .expect("response");
    match resp {
        BridgeMessage::Error { code, .. } => assert_eq!(code, "message_too_large"),
        other => panic!("expected message_too_large error, got {other:?}"),
    }

    // Cleanup
    send_json_line(&mut writer, &AgentMessage::Goodbye).await;
    let _ = tokio::time::timeout(Duration::from_millis(500), run_handle).await;
}

#[tokio::test]
async fn participant_receives_room_message_via_event_channel() {
    let (_bridge, mut conn, mut reader, mut writer) =
        setup_agent_bridge("part-recv-msg", "room-1").await;

    let _ = do_handshake_on_conn(&mut conn, &mut writer, &mut reader, "bot", "Bot").await;

    let (outbound_tx, _outbound_rx) = mpsc::channel(64);
    let (room_tx, room_rx) = mpsc::channel(64);

    let mut participant =
        AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);
    participant.mark_ready();

    let run_handle = tokio::spawn(async move { participant.run().await });

    // Send a room message to the agent
    room_tx
        .send(RoomEvent::Message {
            sender_id: "peer-alice".to_string(),
            sender_name: "Alice".to_string(),
            content: "Hey bot!".to_string(),
            timestamp: "2025-01-15T12:00:00Z".to_string(),
        })
        .await
        .expect("send room event");

    let msg = read_json_line_timeout(&mut reader, Duration::from_millis(500))
        .await
        .expect("message");
    match msg {
        BridgeMessage::RoomMessage {
            sender_id,
            sender_name,
            content,
            timestamp,
        } => {
            assert_eq!(sender_id, "peer-alice");
            assert_eq!(sender_name, "Alice");
            assert_eq!(content, "Hey bot!");
            assert_eq!(timestamp, "2025-01-15T12:00:00Z");
        }
        other => panic!("expected RoomMessage, got {other:?}"),
    }

    drop(room_tx);
    let _ = tokio::time::timeout(Duration::from_millis(500), run_handle).await;
}

#[tokio::test]
async fn participant_receives_membership_update_via_event_channel() {
    let (_bridge, mut conn, mut reader, mut writer) =
        setup_agent_bridge("part-membership", "room-1").await;

    let _ = do_handshake_on_conn(&mut conn, &mut writer, &mut reader, "bot", "Bot").await;

    let (outbound_tx, _outbound_rx) = mpsc::channel(64);
    let (room_tx, room_rx) = mpsc::channel(64);

    let mut participant =
        AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);
    participant.mark_ready();

    let run_handle = tokio::spawn(async move { participant.run().await });

    room_tx
        .send(RoomEvent::MembershipChange {
            action: "joined".to_string(),
            peer_id: "peer-bob".to_string(),
            display_name: "Bob".to_string(),
            is_agent: false,
        })
        .await
        .expect("send");

    let msg = read_json_line_timeout(&mut reader, Duration::from_millis(500))
        .await
        .expect("message");
    match msg {
        BridgeMessage::MembershipUpdate {
            action,
            peer_id,
            display_name,
            is_agent,
        } => {
            assert_eq!(action, "joined");
            assert_eq!(peer_id, "peer-bob");
            assert_eq!(display_name, "Bob");
            assert!(!is_agent);
        }
        other => panic!("expected MembershipUpdate, got {other:?}"),
    }

    drop(room_tx);
    let _ = tokio::time::timeout(Duration::from_millis(500), run_handle).await;
}

#[tokio::test]
async fn participant_peer_id_has_agent_prefix() {
    let (_bridge, conn, _reader, _writer) = setup_agent_bridge("part-prefix", "room-1").await;

    let (outbound_tx, _outbound_rx) = mpsc::channel(64);
    let (_room_tx, room_rx) = mpsc::channel(64);

    let participant = AgentParticipant::new(
        conn,
        "room-1",
        "agent:test-bot",
        "TestBot",
        outbound_tx,
        room_rx,
    );

    assert!(participant.peer_id().starts_with("agent:"));
    assert_eq!(participant.peer_id(), "agent:test-bot");
    assert_eq!(participant.display_name(), "TestBot");
    assert_eq!(participant.room_id(), "room-1");
    assert!(!participant.is_ready());
}

// =============================================================================
// End-to-end lifecycle tests
// =============================================================================

#[tokio::test]
async fn e2e_complete_lifecycle_handshake_send_receive_goodbye() {
    let (_bridge, mut conn, mut reader, mut writer) =
        setup_agent_bridge("e2e-full", "room-abc").await;

    // 1. Handshake
    let hello = AgentMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
        agent_id: "claude-e2e".to_string(),
        display_name: "Claude".to_string(),
        capabilities: vec!["chat".to_string()],
    };
    send_json_line(&mut writer, &hello).await;

    let hs = conn
        .perform_handshake("General", &[], &[], &[], 256)
        .await
        .expect("handshake");
    assert_eq!(hs.peer_id, "agent:claude-e2e");
    assert_eq!(hs.display_name, "Claude");

    let welcome = read_json_line(&mut reader).await;
    assert!(matches!(welcome, BridgeMessage::Welcome { .. }));

    // 2. Create participant and run event loop
    let (outbound_tx, mut outbound_rx) = mpsc::channel(64);
    let (room_tx, room_rx) = mpsc::channel(64);

    let mut participant = AgentParticipant::new(
        conn,
        "room-abc",
        &hs.peer_id,
        &hs.display_name,
        outbound_tx,
        room_rx,
    );
    participant.mark_ready();

    let run_handle = tokio::spawn(async move { participant.run().await });

    // 3. Agent sends a message
    let send_msg = AgentMessage::SendMessage {
        content: "Hello from E2E!".to_string(),
    };
    send_json_line(&mut writer, &send_msg).await;

    let outbound = tokio::time::timeout(Duration::from_millis(500), outbound_rx.recv())
        .await
        .expect("timeout")
        .expect("outbound");
    assert_eq!(outbound.content, "Hello from E2E!");
    assert_eq!(outbound.sender_peer_id, "agent:claude-e2e");

    // 4. Room sends a message to agent
    room_tx
        .send(RoomEvent::Message {
            sender_id: "peer-alice".to_string(),
            sender_name: "Alice".to_string(),
            content: "Hey Claude!".to_string(),
            timestamp: "2025-01-15T12:00:00Z".to_string(),
        })
        .await
        .expect("send room event");

    let msg = read_json_line_timeout(&mut reader, Duration::from_millis(500))
        .await
        .expect("room message");
    match msg {
        BridgeMessage::RoomMessage { content, .. } => {
            assert_eq!(content, "Hey Claude!");
        }
        other => panic!("expected RoomMessage, got {other:?}"),
    }

    // 5. Agent disconnects gracefully
    send_json_line(&mut writer, &AgentMessage::Goodbye).await;

    let cleanup = tokio::time::timeout(Duration::from_millis(500), run_handle)
        .await
        .expect("timeout")
        .expect("join");
    assert_eq!(cleanup.reason, DisconnectReason::Goodbye);
    assert_eq!(cleanup.peer_id, "agent:claude-e2e");
    assert_eq!(cleanup.display_name, "Claude");
    assert_eq!(cleanup.room_id, "room-abc");
}

#[tokio::test]
async fn e2e_disconnect_mid_conversation_broken_pipe() {
    let (_bridge, mut conn, mut reader, mut writer) =
        setup_agent_bridge("e2e-broken", "room-1").await;

    let _ = do_handshake_on_conn(&mut conn, &mut writer, &mut reader, "bot", "Bot").await;

    let (outbound_tx, _outbound_rx) = mpsc::channel(64);
    let (_room_tx, room_rx) = mpsc::channel(64);

    let mut participant =
        AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);
    participant.mark_ready();

    let run_handle = tokio::spawn(async move { participant.run().await });

    // Agent sends a message, then drops
    let msg = AgentMessage::SendMessage {
        content: "mid-conversation".to_string(),
    };
    send_json_line(&mut writer, &msg).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Drop client to simulate broken pipe
    drop(writer);
    drop(reader);

    let cleanup = tokio::time::timeout(Duration::from_millis(500), run_handle)
        .await
        .expect("timeout")
        .expect("join");
    assert_eq!(cleanup.reason, DisconnectReason::BrokenPipe);
}

#[tokio::test]
async fn e2e_room_closed_terminates_participant() {
    let (_bridge, mut conn, mut reader, mut writer) =
        setup_agent_bridge("e2e-roomclose", "room-1").await;

    let _ = do_handshake_on_conn(&mut conn, &mut writer, &mut reader, "bot", "Bot").await;

    let (outbound_tx, _outbound_rx) = mpsc::channel(64);
    let (room_tx, room_rx) = mpsc::channel(64);

    let mut participant =
        AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);
    participant.mark_ready();

    let run_handle = tokio::spawn(async move { participant.run().await });

    // Drop the room event sender to simulate room deletion
    drop(room_tx);

    let cleanup = tokio::time::timeout(Duration::from_millis(500), run_handle)
        .await
        .expect("timeout")
        .expect("join");
    assert_eq!(cleanup.reason, DisconnectReason::RoomClosed);
}

#[tokio::test]
async fn e2e_agent_re_invite_after_disconnect() {
    // First session: connect, handshake, disconnect
    let (_bridge1, mut conn1, mut reader1, mut writer1) =
        setup_agent_bridge("e2e-reinvite1", "room-1").await;

    let welcome1 =
        do_handshake_on_conn(&mut conn1, &mut writer1, &mut reader1, "claude", "Claude").await;
    assert!(matches!(welcome1, BridgeMessage::Welcome { .. }));

    conn1.close().await;
    drop(reader1);
    drop(writer1);

    // Second session: same agent reconnects to new bridge
    let (_bridge2, mut conn2, mut reader2, mut writer2) =
        setup_agent_bridge("e2e-reinvite2", "room-1").await;

    let welcome2 =
        do_handshake_on_conn(&mut conn2, &mut writer2, &mut reader2, "claude", "Claude").await;
    assert!(matches!(welcome2, BridgeMessage::Welcome { .. }));

    conn2.close().await;
}

#[tokio::test]
async fn e2e_multiple_messages_round_trip() {
    let (_bridge, mut conn, mut reader, mut writer) =
        setup_agent_bridge("e2e-multi-msg", "room-1").await;

    let _ = do_handshake_on_conn(&mut conn, &mut writer, &mut reader, "bot", "Bot").await;

    let (outbound_tx, mut outbound_rx) = mpsc::channel(64);
    let (room_tx, room_rx) = mpsc::channel(64);

    let mut participant =
        AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);
    participant.mark_ready();

    let run_handle = tokio::spawn(async move { participant.run().await });

    // Agent sends 3 messages
    for i in 0..3 {
        let msg = AgentMessage::SendMessage {
            content: format!("message-{i}"),
        };
        send_json_line(&mut writer, &msg).await;
    }

    // All 3 should arrive on the outbound channel
    for i in 0..3 {
        let outbound = tokio::time::timeout(Duration::from_millis(500), outbound_rx.recv())
            .await
            .expect("timeout")
            .expect("outbound");
        assert_eq!(outbound.content, format!("message-{i}"));
    }

    // Room sends 3 messages to agent
    for i in 0..3 {
        room_tx
            .send(RoomEvent::Message {
                sender_id: "peer-alice".to_string(),
                sender_name: "Alice".to_string(),
                content: format!("reply-{i}"),
                timestamp: "2025-01-15T12:00:00Z".to_string(),
            })
            .await
            .expect("send room event");
    }

    // All 3 should arrive on the agent side
    for i in 0..3 {
        let msg = read_json_line_timeout(&mut reader, Duration::from_millis(500))
            .await
            .expect("message");
        match msg {
            BridgeMessage::RoomMessage { content, .. } => {
                assert_eq!(content, format!("reply-{i}"));
            }
            other => panic!("expected RoomMessage, got {other:?}"),
        }
    }

    // Graceful shutdown
    send_json_line(&mut writer, &AgentMessage::Goodbye).await;
    let cleanup = tokio::time::timeout(Duration::from_millis(500), run_handle)
        .await
        .expect("timeout")
        .expect("join");
    assert_eq!(cleanup.reason, DisconnectReason::Goodbye);
}

#[tokio::test]
async fn e2e_membership_update_forwarded_via_run_loop() {
    let (_bridge, mut conn, mut reader, mut writer) =
        setup_agent_bridge("e2e-member-fwd", "room-1").await;

    let _ = do_handshake_on_conn(&mut conn, &mut writer, &mut reader, "bot", "Bot").await;

    let (outbound_tx, _outbound_rx) = mpsc::channel(64);
    let (room_tx, room_rx) = mpsc::channel(64);

    let mut participant =
        AgentParticipant::new(conn, "room-1", "agent:bot", "Bot", outbound_tx, room_rx);
    participant.mark_ready();

    let run_handle = tokio::spawn(async move { participant.run().await });

    // Send a membership event
    room_tx
        .send(RoomEvent::MembershipChange {
            action: "left".to_string(),
            peer_id: "peer-charlie".to_string(),
            display_name: "Charlie".to_string(),
            is_agent: false,
        })
        .await
        .expect("send");

    let msg = read_json_line_timeout(&mut reader, Duration::from_millis(500))
        .await
        .expect("message");
    match msg {
        BridgeMessage::MembershipUpdate {
            action,
            peer_id,
            display_name,
            is_agent,
        } => {
            assert_eq!(action, "left");
            assert_eq!(peer_id, "peer-charlie");
            assert_eq!(display_name, "Charlie");
            assert!(!is_agent);
        }
        other => panic!("expected MembershipUpdate, got {other:?}"),
    }

    // Clean shutdown
    drop(room_tx);
    let cleanup = tokio::time::timeout(Duration::from_millis(500), run_handle)
        .await
        .expect("timeout")
        .expect("join");
    assert_eq!(cleanup.reason, DisconnectReason::RoomClosed);
}
