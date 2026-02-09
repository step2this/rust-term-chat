//! Integration tests for UC-010: Connect to Relay and Exchange Live Messages.
//!
//! Tests that the `net` module correctly wires `ChatManager` + `RelayTransport`
//! + `StubNoiseSession` and exchanges messages through the relay server.
//!
//! These tests validate:
//! - `spawn_net` connects to a relay and returns working channel handles
//! - Messages sent via `NetCommand::SendMessage` arrive as `NetEvent::MessageReceived`
//! - Connection failure falls back gracefully (returns error, not panic)
//! - Delivery status transitions: Sent → Delivered
//! - Shutdown command terminates cleanly

use std::time::Duration;

use termchat::net::{self, NetCommand, NetConfig, NetEvent};

/// Start the relay server in-process and return a ws:// URL.
async fn start_relay() -> (String, tokio::task::JoinHandle<()>) {
    let (addr, handle) = termchat_relay::relay::start_server("127.0.0.1:0")
        .await
        .expect("failed to start relay server");
    let url = format!("ws://{addr}/ws");
    (url, handle)
}

/// Helper: create a `NetConfig` for a given relay URL and peer IDs.
fn make_config(relay_url: &str, local: &str, remote: &str) -> NetConfig {
    NetConfig::new(relay_url.to_string(), local.to_string(), remote.to_string())
}

// =============================================================================
// Postcondition 1: TUI connects to relay server on startup
// =============================================================================

#[tokio::test]
async fn spawn_net_connects_to_relay() {
    let (url, _handle) = start_relay().await;
    let config = make_config(&url, "alice", "bob");

    let result = net::spawn_net(config).await;
    assert!(
        result.is_ok(),
        "spawn_net should succeed: {:?}",
        result.err()
    );

    let (_cmd_tx, mut evt_rx) = result.unwrap();

    // Should receive a ConnectionStatus event indicating connected.
    let event = tokio::time::timeout(Duration::from_secs(5), evt_rx.recv())
        .await
        .expect("timeout waiting for connection status")
        .expect("channel closed unexpectedly");

    match event {
        NetEvent::ConnectionStatus {
            connected,
            transport_type,
        } => {
            assert!(connected, "should be connected");
            assert_eq!(transport_type, "Relay");
        }
        other => panic!("expected ConnectionStatus, got: {other:?}"),
    }
}

// =============================================================================
// Postcondition 2 & 3: Messages exchanged between two peers via relay
// =============================================================================

#[tokio::test]
async fn two_peers_exchange_messages_via_relay() {
    let (url, _handle) = start_relay().await;

    let alice_config = make_config(&url, "alice", "bob");
    let bob_config = make_config(&url, "bob", "alice");

    let (alice_cmd_tx, mut alice_evt_rx) = net::spawn_net(alice_config)
        .await
        .expect("alice spawn_net failed");
    let (_bob_cmd_tx, mut bob_evt_rx) = net::spawn_net(bob_config)
        .await
        .expect("bob spawn_net failed");

    // Drain initial ConnectionStatus events.
    drain_connection_events(&mut alice_evt_rx).await;
    drain_connection_events(&mut bob_evt_rx).await;

    // Small delay to let background tasks stabilize.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Alice sends a message to Bob.
    alice_cmd_tx
        .send(NetCommand::SendMessage {
            conversation_id: "@ test".to_string(),
            text: "Hello from Alice!".to_string(),
        })
        .await
        .expect("send command failed");

    // Bob should receive the message.
    let bob_event = wait_for_message_received(&mut bob_evt_rx).await;
    match bob_event {
        NetEvent::MessageReceived {
            sender, content, ..
        } => {
            assert_eq!(sender, "alice");
            assert_eq!(content, "Hello from Alice!");
        }
        other => panic!("expected MessageReceived on Bob's side, got: {other:?}"),
    }
}

// =============================================================================
// Postcondition 4: Delivery status transitions Sent → Delivered
// =============================================================================

#[tokio::test]
async fn delivery_ack_produces_status_changed_event() {
    let (url, _handle) = start_relay().await;

    let alice_config = make_config(&url, "alice-ack", "bob-ack");
    let bob_config = make_config(&url, "bob-ack", "alice-ack");

    let (alice_cmd_tx, mut alice_evt_rx) = net::spawn_net(alice_config)
        .await
        .expect("alice spawn_net failed");
    let (_bob_cmd_tx, mut _bob_evt_rx) = net::spawn_net(bob_config)
        .await
        .expect("bob spawn_net failed");

    // Drain initial ConnectionStatus events.
    drain_connection_events(&mut alice_evt_rx).await;
    drain_connection_events(&mut _bob_evt_rx).await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Alice sends a message.
    alice_cmd_tx
        .send(NetCommand::SendMessage {
            conversation_id: "@ test".to_string(),
            text: "Ack test message".to_string(),
        })
        .await
        .expect("send command failed");

    // Wait for the StatusChanged event on Alice's side (delivered = true).
    // Bob auto-acks via ChatManager, which sends the ack back through the relay.
    let status_event = wait_for_status_changed(&mut alice_evt_rx).await;
    match status_event {
        NetEvent::StatusChanged { delivered, .. } => {
            assert!(delivered, "message should be marked as delivered");
        }
        other => panic!("expected StatusChanged, got: {other:?}"),
    }
}

// =============================================================================
// Failure Postcondition 1: Relay unreachable falls back gracefully
// =============================================================================

#[tokio::test]
async fn spawn_net_returns_error_when_relay_unreachable() {
    // Use a port that is almost certainly not listening.
    let config = make_config("ws://127.0.0.1:1/ws", "alice", "bob");

    let result = net::spawn_net(config).await;
    assert!(result.is_err(), "should fail when relay is unreachable");

    let err = result.unwrap_err();
    assert!(
        err.contains("relay connection failed"),
        "error should mention relay: {err}"
    );
}

// =============================================================================
// Shutdown command
// =============================================================================

#[tokio::test]
async fn shutdown_command_terminates_cleanly() {
    let (url, _handle) = start_relay().await;
    let config = make_config(&url, "alice-shutdown", "bob-shutdown");

    let (cmd_tx, _evt_rx) = net::spawn_net(config).await.expect("spawn_net failed");

    // Send shutdown and verify it doesn't panic.
    cmd_tx
        .send(NetCommand::Shutdown)
        .await
        .expect("shutdown send failed");

    // Brief pause to let the task process shutdown.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Sending another command should fail (channel closed).
    let result = cmd_tx
        .send(NetCommand::SendMessage {
            conversation_id: "@ test".to_string(),
            text: "after shutdown".to_string(),
        })
        .await;
    assert!(result.is_err(), "channel should be closed after shutdown");
}

// =============================================================================
// Bidirectional messaging
// =============================================================================

#[tokio::test]
async fn bidirectional_message_exchange() {
    let (url, _handle) = start_relay().await;

    let alice_config = make_config(&url, "alice-bidir", "bob-bidir");
    let bob_config = make_config(&url, "bob-bidir", "alice-bidir");

    let (alice_cmd_tx, mut alice_evt_rx) = net::spawn_net(alice_config)
        .await
        .expect("alice spawn_net failed");
    let (bob_cmd_tx, mut bob_evt_rx) = net::spawn_net(bob_config)
        .await
        .expect("bob spawn_net failed");

    drain_connection_events(&mut alice_evt_rx).await;
    drain_connection_events(&mut bob_evt_rx).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Alice → Bob
    alice_cmd_tx
        .send(NetCommand::SendMessage {
            conversation_id: "@ test".to_string(),
            text: "Hi Bob!".to_string(),
        })
        .await
        .unwrap();

    let bob_msg = wait_for_message_received(&mut bob_evt_rx).await;
    match bob_msg {
        NetEvent::MessageReceived {
            sender, content, ..
        } => {
            assert_eq!(sender, "alice-bidir");
            assert_eq!(content, "Hi Bob!");
        }
        other => panic!("expected MessageReceived, got: {other:?}"),
    }

    // Bob → Alice
    bob_cmd_tx
        .send(NetCommand::SendMessage {
            conversation_id: "@ test".to_string(),
            text: "Hi Alice!".to_string(),
        })
        .await
        .unwrap();

    let alice_msg = wait_for_message_received(&mut alice_evt_rx).await;
    match alice_msg {
        NetEvent::MessageReceived {
            sender, content, ..
        } => {
            assert_eq!(sender, "bob-bidir");
            assert_eq!(content, "Hi Alice!");
        }
        other => panic!("expected MessageReceived, got: {other:?}"),
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// Drain ConnectionStatus events that arrive at startup.
async fn drain_connection_events(rx: &mut tokio::sync::mpsc::Receiver<NetEvent>) {
    // Consume up to 5 events or until timeout, looking only for ConnectionStatus.
    for _ in 0..5 {
        match tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
            Ok(Some(NetEvent::ConnectionStatus { .. })) => continue,
            _ => break,
        }
    }
}

/// Wait for a `MessageReceived` event, skipping other events.
async fn wait_for_message_received(rx: &mut tokio::sync::mpsc::Receiver<NetEvent>) -> NetEvent {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_secs(5), rx.recv()).await {
            Ok(Some(evt @ NetEvent::MessageReceived { .. })) => return evt,
            Ok(Some(_)) => continue, // skip non-message events
            Ok(None) => panic!("channel closed while waiting for MessageReceived"),
            Err(_) => break, // timeout
        }
    }
    panic!("timeout waiting for MessageReceived event");
}

/// Wait for a `StatusChanged { delivered: true }` event, skipping other events.
///
/// The `ChatManager` emits `StatusChanged(Sent)` first, then `StatusChanged(Delivered)`
/// when the ack arrives. This helper skips the initial `Sent` event.
async fn wait_for_status_changed(rx: &mut tokio::sync::mpsc::Receiver<NetEvent>) -> NetEvent {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_secs(5), rx.recv()).await {
            Ok(Some(
                evt @ NetEvent::StatusChanged {
                    delivered: true, ..
                },
            )) => return evt,
            Ok(Some(_)) => continue, // skip Sent status and non-status events
            Ok(None) => panic!("channel closed while waiting for StatusChanged"),
            Err(_) => break,
        }
    }
    panic!("timeout waiting for StatusChanged(delivered=true) event");
}
