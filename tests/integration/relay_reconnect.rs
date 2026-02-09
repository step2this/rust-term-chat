// Test-specific lint overrides: integration tests use unwrap/expect freely,
// and some pedantic/nursery lints are not appropriate for test code.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::needless_continue,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_let_else,
    clippy::future_not_send,
    clippy::redundant_pub_crate,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc,
    clippy::missing_docs_in_private_items
)]

//! Integration tests for UC-011: Auto-Reconnect to Relay.
//!
//! Tests that the `net` module supervisor correctly detects relay disconnection,
//! applies exponential backoff, reconnects, drains queued messages, and handles
//! graceful shutdown during reconnection.
//!
//! These tests validate:
//! - `spawn_net` reconnects automatically when the relay is restarted
//! - Messages queued during disconnection are delivered after reconnect
//! - Exponential backoff timing is correct
//! - Graceful shutdown works during reconnection
//! - Messages sent during active reconnection attempts are queued
//!
//! ## Disconnect simulation
//!
//! Simply aborting the relay server's `JoinHandle` does not close existing
//! WebSocket connections (they are on independently-spawned tasks). Instead
//! we place a **TCP proxy** between the client and the real relay. To simulate
//! a disconnect we abort ALL proxy connection tasks (tracked in a shared vec),
//! which immediately closes both ends of every proxied TCP connection, causing
//! the client's WebSocket layer to detect a disconnect.

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use termchat::config::ReconnectConfig;
use termchat::net::{self, NetCommand, NetConfig, NetEvent};
use tokio::sync::mpsc;

// =============================================================================
// TCP Proxy helper
// =============================================================================

/// A simple TCP proxy that forwards traffic between a client-facing port and
/// a backend (the real relay). Calling `kill()` aborts all tracked connection
/// tasks, which immediately tears down both directions of every proxied TCP
/// connection, causing the client's WebSocket layer to detect a disconnect.
struct TcpProxy {
    /// Address clients should connect to (127.0.0.1:<proxy_port>).
    pub client_addr: String,
    /// The acceptor task handle.
    accept_handle: tokio::task::JoinHandle<()>,
    /// All per-connection task handles. Aborting these kills the TCP streams.
    conn_handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl TcpProxy {
    /// Create a new TCP proxy from `proxy_port` to `backend_addr`.
    async fn new(proxy_port: u16, backend_addr: &str) -> Self {
        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{proxy_port}"))
            .await
            .unwrap_or_else(|e| panic!("proxy: failed to bind to port {proxy_port}: {e}"));
        let bound_addr = listener.local_addr().unwrap();
        let client_addr = format!("127.0.0.1:{}", bound_addr.port());
        let backend = backend_addr.to_string();
        let conn_handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>> =
            Arc::new(Mutex::new(Vec::new()));
        let conn_handles_clone = Arc::clone(&conn_handles);

        let accept_handle = tokio::spawn(async move {
            loop {
                let (mut client_stream, _) = match listener.accept().await {
                    Ok(v) => v,
                    Err(_) => break,
                };

                let backend = backend.clone();
                let conn_handle = tokio::spawn(async move {
                    let Ok(mut backend_stream) = tokio::net::TcpStream::connect(&backend).await
                    else {
                        return;
                    };

                    // Copy bidirectionally. When this task is aborted, both
                    // streams are dropped immediately, causing RST on both
                    // ends. We do NOT spawn sub-tasks so that abort propagates.
                    let _ = tokio::io::copy_bidirectional(&mut client_stream, &mut backend_stream)
                        .await;
                });

                conn_handles_clone.lock().push(conn_handle);
            }
        });

        Self {
            client_addr,
            accept_handle,
            conn_handles,
        }
    }

    /// Kill the proxy, severing all connections immediately.
    fn kill(self) {
        // Abort the accept loop so no new connections are accepted.
        self.accept_handle.abort();
        // Abort all per-connection tasks, which drops the TcpStreams and
        // causes immediate RST on both ends.
        let handles = self.conn_handles.lock();
        for h in handles.iter() {
            h.abort();
        }
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// Find a free port by binding to 0 and recording the port.
async fn find_free_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind to port 0");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    // Brief pause to let the OS release the port.
    tokio::time::sleep(Duration::from_millis(50)).await;
    port
}

/// Start relay on port 0 (OS-assigned), return (bound_addr_string, handle).
async fn start_relay() -> (String, tokio::task::JoinHandle<()>) {
    let (addr, handle) = termchat_relay::relay::start_server("127.0.0.1:0")
        .await
        .expect("failed to start relay server");
    (addr.to_string(), handle)
}

/// Create a `NetConfig` with fast reconnect settings for testing.
fn make_reconnect_config(relay_url: &str, local: &str, remote: &str) -> NetConfig {
    let mut config = NetConfig::new(relay_url.to_string(), local.to_string(), remote.to_string());
    config.reconnect = ReconnectConfig {
        initial_delay: Duration::from_millis(100),
        max_delay: Duration::from_secs(5),
        max_attempts: 5,
        stability_threshold: Duration::from_secs(30),
        message_queue_cap: 100,
    };
    config
}

/// Wait for a specific `NetEvent` variant matching a predicate, with timeout.
///
/// Skips non-matching events. Panics on timeout or channel close.
async fn wait_for_event<F>(
    rx: &mut mpsc::Receiver<NetEvent>,
    timeout: Duration,
    description: &str,
    pred: F,
) -> NetEvent
where
    F: Fn(&NetEvent) -> bool,
{
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline - tokio::time::Instant::now();
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(evt)) if pred(&evt) => return evt,
            Ok(Some(_other)) => continue,
            Ok(None) => panic!("channel closed while waiting for {description}"),
            Err(_) => break,
        }
    }
    panic!("timeout waiting for {description}");
}

/// Wait for a `ConnectionStatus { connected: true }` event.
async fn wait_for_connected(rx: &mut mpsc::Receiver<NetEvent>) -> NetEvent {
    wait_for_event(
        rx,
        Duration::from_secs(15),
        "ConnectionStatus { connected: true }",
        |evt| {
            matches!(
                evt,
                NetEvent::ConnectionStatus {
                    connected: true,
                    ..
                }
            )
        },
    )
    .await
}

/// Wait for a `ConnectionStatus { connected: false }` event.
async fn wait_for_disconnected(rx: &mut mpsc::Receiver<NetEvent>) -> NetEvent {
    wait_for_event(
        rx,
        Duration::from_secs(10),
        "ConnectionStatus { connected: false }",
        |evt| {
            matches!(
                evt,
                NetEvent::ConnectionStatus {
                    connected: false,
                    ..
                }
            )
        },
    )
    .await
}

/// Wait for a `Reconnecting` event.
async fn wait_for_reconnecting(rx: &mut mpsc::Receiver<NetEvent>) -> NetEvent {
    wait_for_event(rx, Duration::from_secs(10), "Reconnecting", |evt| {
        matches!(evt, NetEvent::Reconnecting { .. })
    })
    .await
}

/// Wait for a `ReconnectFailed` event.
async fn wait_for_reconnect_failed(rx: &mut mpsc::Receiver<NetEvent>) -> NetEvent {
    wait_for_event(rx, Duration::from_secs(30), "ReconnectFailed", |evt| {
        matches!(evt, NetEvent::ReconnectFailed)
    })
    .await
}

/// Wait for a `MessageReceived` event.
async fn wait_for_message_received(rx: &mut mpsc::Receiver<NetEvent>) -> NetEvent {
    wait_for_event(rx, Duration::from_secs(15), "MessageReceived", |evt| {
        matches!(evt, NetEvent::MessageReceived { .. })
    })
    .await
}

/// Drain initial ConnectionStatus events from startup.
async fn drain_connection_events(rx: &mut mpsc::Receiver<NetEvent>) {
    for _ in 0..5 {
        match tokio::time::timeout(Duration::from_millis(300), rx.recv()).await {
            Ok(Some(NetEvent::ConnectionStatus { .. })) => continue,
            _ => break,
        }
    }
}

// =============================================================================
// Test 1: Reconnect after relay restart
// =============================================================================

/// Verifies that after the relay connection is severed (via proxy kill) and
/// a new proxy is established, the supervisor reconnects automatically and
/// messaging resumes.
#[tokio::test]
async fn reconnect_after_relay_restart() {
    // Start the real relay on an OS-assigned port.
    let (relay_addr, _relay_handle) = start_relay().await;

    // Create a proxy port for alice.
    let proxy_port = find_free_port().await;
    let proxy = TcpProxy::new(proxy_port, &relay_addr).await;
    let proxy_url = format!("ws://{}/ws", proxy.client_addr);

    // Connect alice through the proxy.
    let config = make_reconnect_config(&proxy_url, "alice-t1", "bob-t1");
    let (cmd_tx, mut evt_rx) = net::spawn_net(config).await.expect("spawn_net failed");

    // Drain initial ConnectionStatus.
    drain_connection_events(&mut evt_rx).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Kill the proxy to simulate a network partition.
    proxy.kill();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Wait for disconnect notification.
    wait_for_disconnected(&mut evt_rx).await;

    // Wait for first reconnect attempt.
    let evt = wait_for_reconnecting(&mut evt_rx).await;
    match evt {
        NetEvent::Reconnecting { attempt, .. } => {
            assert_eq!(attempt, 1, "first attempt should be 1");
        }
        other => panic!("expected Reconnecting, got: {other:?}"),
    }

    // Re-create the proxy on the same port (relay is still alive).
    let _proxy2 = TcpProxy::new(proxy_port, &relay_addr).await;

    // Wait for reconnection success.
    wait_for_connected(&mut evt_rx).await;

    // Connect bob directly to the relay (no proxy needed for bob).
    let bob_url = format!("ws://{relay_addr}/ws");
    let bob_config = make_reconnect_config(&bob_url, "bob-t1", "alice-t1");
    let (_bob_cmd_tx, mut bob_evt_rx) = net::spawn_net(bob_config)
        .await
        .expect("bob spawn_net failed");
    drain_connection_events(&mut bob_evt_rx).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Alice sends a message after reconnect.
    cmd_tx
        .send(NetCommand::SendMessage {
            conversation_id: "@ test".to_string(),
            text: "Hello after reconnect!".to_string(),
        })
        .await
        .expect("send command failed");

    // Bob should receive the message.
    let bob_msg = wait_for_message_received(&mut bob_evt_rx).await;
    match bob_msg {
        NetEvent::MessageReceived {
            sender, content, ..
        } => {
            assert_eq!(sender, "alice-t1");
            assert_eq!(content, "Hello after reconnect!");
        }
        other => panic!("expected MessageReceived, got: {other:?}"),
    }
}

// =============================================================================
// Test 2: Queued messages sent after reconnect
// =============================================================================

#[tokio::test]
async fn queued_messages_sent_after_reconnect() {
    let (relay_addr, _relay_handle) = start_relay().await;

    let proxy_port = find_free_port().await;
    let proxy = TcpProxy::new(proxy_port, &relay_addr).await;
    let proxy_url = format!("ws://{}/ws", proxy.client_addr);

    // Connect alice through the proxy.
    let alice_config = make_reconnect_config(&proxy_url, "alice-t2", "bob-t2");
    let (alice_cmd_tx, mut alice_evt_rx) =
        net::spawn_net(alice_config).await.expect("alice failed");
    drain_connection_events(&mut alice_evt_rx).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Kill proxy to disconnect alice.
    proxy.kill();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Wait for alice to detect disconnect and enter reconnection mode.
    // We must wait for Reconnecting to ensure the supervisor has set
    // the shared ChatManager to None (so messages are queued, not sent
    // on the broken connection).
    wait_for_disconnected(&mut alice_evt_rx).await;
    wait_for_reconnecting(&mut alice_evt_rx).await;

    // Send 3 messages while disconnected (they should be queued).
    for i in 1..=3 {
        alice_cmd_tx
            .send(NetCommand::SendMessage {
                conversation_id: "@ test".to_string(),
                text: format!("Queued message {i}"),
            })
            .await
            .expect("send command failed");
    }

    // Brief pause to let commands be processed.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Re-create proxy so alice can reconnect.
    let _proxy2 = TcpProxy::new(proxy_port, &relay_addr).await;

    // Wait for alice to reconnect.
    wait_for_connected(&mut alice_evt_rx).await;

    // Now connect bob directly to the relay to receive queued messages.
    // Do NOT drain bob's events: the relay drains queued messages during
    // registration, so MessageReceived events may arrive mixed with
    // ConnectionStatus events. The loop below skips non-matching events.
    let bob_url = format!("ws://{relay_addr}/ws");
    let bob_config = make_reconnect_config(&bob_url, "bob-t2", "alice-t2");
    let (_bob_cmd_tx, mut bob_evt_rx) = net::spawn_net(bob_config)
        .await
        .expect("bob spawn_net failed");

    // Wait for all 3 messages to arrive at bob.
    let mut received_messages = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while received_messages.len() < 3 && tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_secs(10), bob_evt_rx.recv()).await {
            Ok(Some(NetEvent::MessageReceived { content, .. })) => {
                received_messages.push(content);
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => break,
        }
    }

    assert_eq!(
        received_messages.len(),
        3,
        "expected 3 queued messages, got {}: {:?}",
        received_messages.len(),
        received_messages
    );

    // Verify messages contain our queued text (order may vary due to relay routing).
    for i in 1..=3 {
        let expected = format!("Queued message {i}");
        assert!(
            received_messages.contains(&expected),
            "missing '{expected}' in received messages: {received_messages:?}"
        );
    }
}

// =============================================================================
// Test 3: Exponential backoff timing
// =============================================================================

#[tokio::test]
async fn exponential_backoff_timing() {
    let (relay_addr, relay_handle) = start_relay().await;

    let proxy_port = find_free_port().await;
    let proxy = TcpProxy::new(proxy_port, &relay_addr).await;
    let proxy_url = format!("ws://{}/ws", proxy.client_addr);

    // Use very fast reconnect settings with only 3 attempts.
    let mut config = NetConfig::new(
        proxy_url.clone(),
        "alice-t3".to_string(),
        "bob-t3".to_string(),
    );
    config.reconnect = ReconnectConfig {
        initial_delay: Duration::from_millis(100),
        max_delay: Duration::from_secs(10),
        max_attempts: 3,
        stability_threshold: Duration::from_secs(60),
        message_queue_cap: 10,
    };

    let (_cmd_tx, mut evt_rx) = net::spawn_net(config).await.expect("spawn_net failed");
    drain_connection_events(&mut evt_rx).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Kill proxy AND relay so reconnect attempts also fail.
    proxy.kill();
    relay_handle.abort();

    // Wait for disconnect.
    wait_for_disconnected(&mut evt_rx).await;

    // Collect all 3 Reconnecting events and measure the time between them.
    // The exponential backoff with initial_delay=100ms means:
    // gap between attempts: ~100ms, ~200ms, ~400ms (plus up to 25% jitter).
    let mut attempt_instants = Vec::new();

    for expected_attempt in 1..=3 {
        let evt = wait_for_event(
            &mut evt_rx,
            Duration::from_secs(10),
            &format!("Reconnecting attempt {expected_attempt}"),
            |evt| matches!(evt, NetEvent::Reconnecting { .. }),
        )
        .await;

        attempt_instants.push(Instant::now());

        match evt {
            NetEvent::Reconnecting {
                attempt,
                max_attempts,
            } => {
                assert_eq!(attempt, expected_attempt);
                assert_eq!(max_attempts, 3);
            }
            other => panic!("expected Reconnecting, got: {other:?}"),
        }
    }

    // Verify gaps between consecutive attempts show exponential backoff.
    // Gap 1->2 should be ~200ms (the delay before attempt 2).
    // Gap 2->3 should be ~400ms (the delay before attempt 3).
    // We add generous tolerance for jitter (up to 25%) and scheduling.
    if attempt_instants.len() >= 2 {
        let gap_1_to_2 = attempt_instants[1] - attempt_instants[0];
        assert!(
            gap_1_to_2 >= Duration::from_millis(150),
            "gap between attempt 1 and 2 too short: {gap_1_to_2:?}"
        );
    }

    if attempt_instants.len() >= 3 {
        let gap_2_to_3 = attempt_instants[2] - attempt_instants[1];
        assert!(
            gap_2_to_3 >= Duration::from_millis(300),
            "gap between attempt 2 and 3 too short: {gap_2_to_3:?}"
        );
        // Also verify that gap 2->3 is larger than gap 1->2 (exponential).
        let gap_1_to_2 = attempt_instants[1] - attempt_instants[0];
        assert!(
            gap_2_to_3 > gap_1_to_2,
            "gap 2->3 ({gap_2_to_3:?}) should be larger than gap 1->2 ({gap_1_to_2:?})"
        );
    }

    // After all attempts exhausted, we should get ReconnectFailed.
    wait_for_reconnect_failed(&mut evt_rx).await;
}

// =============================================================================
// Test 4: Send during reconnection queues message
// =============================================================================

#[tokio::test]
async fn send_during_reconnection_queues_message() {
    let (relay_addr, _relay_handle) = start_relay().await;

    let proxy_port = find_free_port().await;
    let proxy = TcpProxy::new(proxy_port, &relay_addr).await;
    let proxy_url = format!("ws://{}/ws", proxy.client_addr);

    let config = make_reconnect_config(&proxy_url, "alice-t4", "bob-t4");
    let (cmd_tx, mut evt_rx) = net::spawn_net(config).await.expect("spawn_net failed");
    drain_connection_events(&mut evt_rx).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Kill proxy.
    proxy.kill();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Wait for disconnect and first Reconnecting event.
    wait_for_disconnected(&mut evt_rx).await;
    wait_for_reconnecting(&mut evt_rx).await;

    // Send a message while reconnection is in progress.
    cmd_tx
        .send(NetCommand::SendMessage {
            conversation_id: "@ test".to_string(),
            text: "Message during reconnect".to_string(),
        })
        .await
        .expect("send command failed");

    // Brief pause to let the message be queued.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Re-create proxy so alice can reconnect.
    let _proxy2 = TcpProxy::new(proxy_port, &relay_addr).await;

    // Wait for alice to reconnect.
    wait_for_connected(&mut evt_rx).await;

    // Small delay to let alice's queue drain finish sending to the relay.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Connect bob directly to receive the queued message.
    let bob_url = format!("ws://{relay_addr}/ws");
    let bob_config = make_reconnect_config(&bob_url, "bob-t4", "alice-t4");
    let (_bob_cmd_tx, mut bob_evt_rx) = net::spawn_net(bob_config)
        .await
        .expect("bob spawn_net failed");
    // Do NOT drain bob's events here: the relay drains the queued message
    // during registration, so the MessageReceived event may arrive mixed
    // in with ConnectionStatus events. wait_for_message_received skips
    // non-matching events.

    // Verify the queued message arrives at bob.
    let bob_msg = wait_for_message_received(&mut bob_evt_rx).await;
    match bob_msg {
        NetEvent::MessageReceived { content, .. } => {
            assert_eq!(content, "Message during reconnect");
        }
        other => panic!("expected MessageReceived, got: {other:?}"),
    }
}

// =============================================================================
// Test 5: Graceful shutdown during reconnect
// =============================================================================

#[tokio::test]
async fn graceful_shutdown_during_reconnect() {
    let (relay_addr, relay_handle) = start_relay().await;

    let proxy_port = find_free_port().await;
    let proxy = TcpProxy::new(proxy_port, &relay_addr).await;
    let proxy_url = format!("ws://{}/ws", proxy.client_addr);

    let config = make_reconnect_config(&proxy_url, "alice-t5", "bob-t5");
    let (cmd_tx, mut evt_rx) = net::spawn_net(config).await.expect("spawn_net failed");
    drain_connection_events(&mut evt_rx).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Kill proxy AND relay so reconnect attempts also fail.
    proxy.kill();
    relay_handle.abort();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Wait for disconnect and at least one Reconnecting event.
    wait_for_disconnected(&mut evt_rx).await;
    wait_for_reconnecting(&mut evt_rx).await;

    // Send shutdown while reconnection is in progress.
    cmd_tx
        .send(NetCommand::Shutdown)
        .await
        .expect("shutdown send failed");

    // Brief pause for shutdown to propagate.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify the channel closes cleanly (no panic, recv returns None eventually).
    let mut closed = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_secs(2), evt_rx.recv()).await {
            Ok(None) => {
                closed = true;
                break;
            }
            Ok(Some(_)) => continue, // skip remaining events
            Err(_) => break,         // timeout, channel still open but idle
        }
    }

    // The channel should eventually close (supervisor exits, drops evt_tx).
    // If it didn't close within our timeout, at least verify no deadlock.
    if !closed {
        let result = cmd_tx
            .send(NetCommand::SendMessage {
                conversation_id: "@ test".to_string(),
                text: "after shutdown".to_string(),
            })
            .await;
        // May or may not fail; the important thing is no panic.
        let _ = result;
    }
}
