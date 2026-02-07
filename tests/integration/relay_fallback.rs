//! Integration tests for UC-004: Relay Messages via Server.
//!
//! Validates all success postconditions for relay transport:
//! - Message round-trips through the relay server
//! - Store-and-forward for offline recipients
//! - HybridTransport fallback to relay
//! - FIFO ordering, disconnect detection, concurrent peers

use std::time::Duration;

use termchat::transport::hybrid::HybridTransport;
use termchat::transport::loopback::LoopbackTransport;
use termchat::transport::relay::RelayTransport;
use termchat::transport::{PeerId, Transport, TransportError, TransportType};

/// Start the relay server in-process and return a ws:// URL.
async fn start_relay() -> (String, tokio::task::JoinHandle<()>) {
    let (addr, handle) = termchat_relay::relay::start_server("127.0.0.1:0")
        .await
        .expect("failed to start relay server");
    let url = format!("ws://{addr}/ws");
    (url, handle)
}

// =============================================================================
// T-004-15: Store-and-forward integration tests
// =============================================================================

#[tokio::test]
async fn store_and_forward_single_message() {
    let (url, _handle) = start_relay().await;

    // Client A connects and sends to offline Client B.
    let alice = RelayTransport::connect(&url, PeerId::new("alice"))
        .await
        .unwrap();

    alice
        .send(&PeerId::new("bob"), b"hello offline bob")
        .await
        .unwrap();

    // Brief pause to let the relay process the message.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Client B connects — should receive the queued message.
    let bob = RelayTransport::connect(&url, PeerId::new("bob"))
        .await
        .unwrap();

    let (from, data) = tokio::time::timeout(Duration::from_secs(5), bob.recv())
        .await
        .expect("recv timed out")
        .unwrap();

    assert_eq!(from, PeerId::new("alice"));
    assert_eq!(data, b"hello offline bob");
}

#[tokio::test]
async fn store_and_forward_multiple_messages_fifo() {
    let (url, _handle) = start_relay().await;

    let alice = RelayTransport::connect(&url, PeerId::new("alice"))
        .await
        .unwrap();

    // Send 10 messages to offline Bob.
    for i in 0u32..10 {
        alice
            .send(&PeerId::new("bob"), &i.to_le_bytes())
            .await
            .unwrap();
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Bob connects and receives all messages in FIFO order.
    let bob = RelayTransport::connect(&url, PeerId::new("bob"))
        .await
        .unwrap();

    for i in 0u32..10 {
        let (from, data) = tokio::time::timeout(Duration::from_secs(5), bob.recv())
            .await
            .expect("recv timed out")
            .unwrap();
        assert_eq!(from, PeerId::new("alice"));
        let received = u32::from_le_bytes(data.try_into().unwrap());
        assert_eq!(received, i, "FIFO order violated at message {i}");
    }
}

#[tokio::test]
async fn store_and_forward_preserves_sender_identity() {
    let (url, _handle) = start_relay().await;

    // Multiple senders send to offline Bob.
    let alice = RelayTransport::connect(&url, PeerId::new("alice"))
        .await
        .unwrap();
    let carol = RelayTransport::connect(&url, PeerId::new("carol"))
        .await
        .unwrap();

    alice
        .send(&PeerId::new("bob"), b"from alice")
        .await
        .unwrap();
    carol
        .send(&PeerId::new("bob"), b"from carol")
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    let bob = RelayTransport::connect(&url, PeerId::new("bob"))
        .await
        .unwrap();

    // Both messages arrive with correct sender PeerId.
    let mut received = Vec::new();
    for _ in 0..2 {
        let (from, data) = tokio::time::timeout(Duration::from_secs(5), bob.recv())
            .await
            .expect("recv timed out")
            .unwrap();
        received.push((from.as_str().to_string(), data));
    }

    // Order is alice then carol (FIFO from relay's perspective).
    assert_eq!(received[0].0, "alice");
    assert_eq!(received[0].1, b"from alice");
    assert_eq!(received[1].0, "carol");
    assert_eq!(received[1].1, b"from carol");
}

// =============================================================================
// T-004-16: Integration test — relay_fallback end-to-end
// =============================================================================

#[tokio::test]
async fn message_round_trip_bidirectional() {
    let (url, _handle) = start_relay().await;

    let alice = RelayTransport::connect(&url, PeerId::new("alice"))
        .await
        .unwrap();
    let bob = RelayTransport::connect(&url, PeerId::new("bob"))
        .await
        .unwrap();

    // A → B.
    alice.send(&PeerId::new("bob"), b"hello bob").await.unwrap();
    let (from, data) = tokio::time::timeout(Duration::from_secs(5), bob.recv())
        .await
        .expect("recv timed out")
        .unwrap();
    assert_eq!(from, PeerId::new("alice"));
    assert_eq!(data, b"hello bob");

    // B → A.
    bob.send(&PeerId::new("alice"), b"hello alice")
        .await
        .unwrap();
    let (from, data) = tokio::time::timeout(Duration::from_secs(5), alice.recv())
        .await
        .expect("recv timed out")
        .unwrap();
    assert_eq!(from, PeerId::new("bob"));
    assert_eq!(data, b"hello alice");
}

#[tokio::test]
async fn transport_type_is_relay() {
    let (url, _handle) = start_relay().await;

    let transport = RelayTransport::connect(&url, PeerId::new("alice"))
        .await
        .unwrap();

    assert_eq!(transport.transport_type(), TransportType::Relay);
}

#[tokio::test]
async fn is_connected_returns_true_when_connected() {
    let (url, _handle) = start_relay().await;

    let alice = RelayTransport::connect(&url, PeerId::new("alice"))
        .await
        .unwrap();
    let bob = RelayTransport::connect(&url, PeerId::new("bob"))
        .await
        .unwrap();

    assert!(alice.is_connected(&PeerId::new("bob")));
    assert!(bob.is_connected(&PeerId::new("alice")));
}

#[tokio::test]
async fn fifo_ordering_50_messages() {
    let (url, _handle) = start_relay().await;

    let alice = RelayTransport::connect(&url, PeerId::new("alice"))
        .await
        .unwrap();
    let bob = RelayTransport::connect(&url, PeerId::new("bob"))
        .await
        .unwrap();

    // Send 50 messages.
    for i in 0u32..50 {
        alice
            .send(&PeerId::new("bob"), &i.to_le_bytes())
            .await
            .unwrap();
    }

    // Receive all 50 in order.
    for i in 0u32..50 {
        let (from, data) = tokio::time::timeout(Duration::from_secs(10), bob.recv())
            .await
            .expect("recv timed out")
            .unwrap();
        assert_eq!(from, PeerId::new("alice"));
        let received = u32::from_le_bytes(data.try_into().unwrap());
        assert_eq!(received, i, "FIFO order violated at message {i}");
    }
}

#[tokio::test]
async fn disconnect_detection_after_server_close() {
    // Use a minimal server that closes after registration.
    use futures_util::{SinkExt, StreamExt};
    use termchat_proto::relay::{self, RelayMessage};
    use tokio_tungstenite::tungstenite as ws;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("ws://{addr}/ws");

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws_stream = tokio_tungstenite::accept_async(stream).await.unwrap();

        if let Some(Ok(ws::Message::Binary(data))) = ws_stream.next().await {
            if let Ok(RelayMessage::Register { peer_id }) = relay::decode(&data) {
                let ack = RelayMessage::Registered { peer_id };
                let bytes = relay::encode(&ack).unwrap();
                let _ = ws_stream.send(ws::Message::Binary(bytes.into())).await;
            }
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = ws_stream.close(None).await;
    });

    let transport = RelayTransport::connect(&url, PeerId::new("alice"))
        .await
        .unwrap();
    assert!(transport.is_connected(&PeerId::new("anyone")));

    // Wait for server to close connection.
    server.await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(
        !transport.is_connected(&PeerId::new("anyone")),
        "should be disconnected after server close"
    );
}

#[tokio::test]
async fn send_after_disconnect_returns_error() {
    use futures_util::{SinkExt, StreamExt};
    use termchat_proto::relay::{self, RelayMessage};
    use tokio_tungstenite::tungstenite as ws;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("ws://{addr}/ws");

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws_stream = tokio_tungstenite::accept_async(stream).await.unwrap();

        if let Some(Ok(ws::Message::Binary(data))) = ws_stream.next().await {
            if let Ok(RelayMessage::Register { peer_id }) = relay::decode(&data) {
                let ack = RelayMessage::Registered { peer_id };
                let bytes = relay::encode(&ack).unwrap();
                let _ = ws_stream.send(ws::Message::Binary(bytes.into())).await;
            }
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = ws_stream.close(None).await;
    });

    let transport = RelayTransport::connect(&url, PeerId::new("alice"))
        .await
        .unwrap();

    // Wait for disconnect.
    server.await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        if !transport.is_connected(&PeerId::new("bob")) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let result = transport.send(&PeerId::new("bob"), b"hello").await;
    assert!(
        matches!(result, Err(TransportError::ConnectionClosed)),
        "expected ConnectionClosed, got: {:?}",
        result
    );
}

#[tokio::test]
async fn recv_after_disconnect_returns_error() {
    use futures_util::{SinkExt, StreamExt};
    use termchat_proto::relay::{self, RelayMessage};
    use tokio_tungstenite::tungstenite as ws;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("ws://{addr}/ws");

    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws_stream = tokio_tungstenite::accept_async(stream).await.unwrap();

        if let Some(Ok(ws::Message::Binary(data))) = ws_stream.next().await {
            if let Ok(RelayMessage::Register { peer_id }) = relay::decode(&data) {
                let ack = RelayMessage::Registered { peer_id };
                let bytes = relay::encode(&ack).unwrap();
                let _ = ws_stream.send(ws::Message::Binary(bytes.into())).await;
            }
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = ws_stream.close(None).await;
    });

    let transport = RelayTransport::connect(&url, PeerId::new("alice"))
        .await
        .unwrap();

    let result = tokio::time::timeout(Duration::from_secs(5), transport.recv()).await;
    match result {
        Ok(Err(TransportError::ConnectionClosed)) => {} // expected
        Ok(other) => panic!("expected ConnectionClosed, got: {:?}", other),
        Err(_) => panic!("recv did not return within timeout after disconnect"),
    }
}

#[tokio::test]
async fn hybrid_transport_fallback_to_relay() {
    let (url, _handle) = start_relay().await;

    // Create a loopback pair for "preferred" — then break it.
    let (pref_alice, pref_bob) =
        LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 32);
    // Drop the preferred remote to make preferred sends fail.
    drop(pref_bob);

    // Create relay transports for fallback.
    let relay_alice = RelayTransport::connect(&url, PeerId::new("alice"))
        .await
        .unwrap();
    let relay_bob = RelayTransport::connect(&url, PeerId::new("bob"))
        .await
        .unwrap();

    // Build HybridTransport with broken preferred and working relay fallback.
    let hybrid = HybridTransport::new(pref_alice, relay_alice);

    // Send via hybrid — preferred fails, should fall through to relay.
    hybrid
        .send(&PeerId::new("bob"), b"via fallback relay")
        .await
        .unwrap();

    // Bob receives via his relay transport.
    let (from, data) = tokio::time::timeout(Duration::from_secs(5), relay_bob.recv())
        .await
        .expect("recv timed out")
        .unwrap();

    assert_eq!(from, PeerId::new("alice"));
    assert_eq!(data, b"via fallback relay");
}

#[tokio::test]
async fn multiple_peers_concurrent_exchange() {
    let (url, _handle) = start_relay().await;

    let alice = RelayTransport::connect(&url, PeerId::new("alice"))
        .await
        .unwrap();
    let bob = RelayTransport::connect(&url, PeerId::new("bob"))
        .await
        .unwrap();
    let carol = RelayTransport::connect(&url, PeerId::new("carol"))
        .await
        .unwrap();

    // All three exchange messages concurrently.
    alice.send(&PeerId::new("bob"), b"a->b").await.unwrap();
    bob.send(&PeerId::new("carol"), b"b->c").await.unwrap();
    carol.send(&PeerId::new("alice"), b"c->a").await.unwrap();

    // Each receives exactly one message.
    let (from, data) = tokio::time::timeout(Duration::from_secs(5), bob.recv())
        .await
        .expect("recv timed out")
        .unwrap();
    assert_eq!(from, PeerId::new("alice"));
    assert_eq!(data, b"a->b");

    let (from, data) = tokio::time::timeout(Duration::from_secs(5), carol.recv())
        .await
        .expect("recv timed out")
        .unwrap();
    assert_eq!(from, PeerId::new("bob"));
    assert_eq!(data, b"b->c");

    let (from, data) = tokio::time::timeout(Duration::from_secs(5), alice.recv())
        .await
        .expect("recv timed out")
        .unwrap();
    assert_eq!(from, PeerId::new("carol"));
    assert_eq!(data, b"c->a");
}

#[tokio::test]
async fn relay_queue_eviction_at_cap() {
    let (url, _handle) = start_relay().await;

    let alice = RelayTransport::connect(&url, PeerId::new("alice"))
        .await
        .unwrap();

    // Send 1001 messages to offline Bob — relay should evict the oldest.
    for i in 0u32..1001 {
        alice
            .send(&PeerId::new("bob"), &i.to_le_bytes())
            .await
            .unwrap();
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    let bob = RelayTransport::connect(&url, PeerId::new("bob"))
        .await
        .unwrap();

    // Bob should receive exactly 1000 messages (i=1 through i=1000).
    let mut received = Vec::new();
    for _ in 0..1000 {
        let (_from, data) = tokio::time::timeout(Duration::from_secs(10), bob.recv())
            .await
            .expect("recv timed out")
            .unwrap();
        let val = u32::from_le_bytes(data.try_into().unwrap());
        received.push(val);
    }

    assert_eq!(received.len(), 1000);
    // The first message (i=0) should have been evicted.
    assert_eq!(
        received[0], 1,
        "oldest message should be evicted (i=0 gone)"
    );
    assert_eq!(received[999], 1000, "newest message should be i=1000");

    // Verify FIFO order within the remaining messages.
    for window in received.windows(2) {
        assert!(
            window[0] < window[1],
            "FIFO order violated: {} >= {}",
            window[0],
            window[1]
        );
    }
}

#[tokio::test]
async fn peer_id_enforcement_via_relay() {
    let (url, _handle) = start_relay().await;

    // Alice connects and will attempt to spoof the `from` field.
    // We need to use raw WebSocket to send a spoofed message,
    // since RelayTransport always sets from = local_id.
    use futures_util::{SinkExt, StreamExt};
    use termchat_proto::relay::{self, RelayMessage};
    use tokio_tungstenite::tungstenite as ws;

    // Alice registers with a raw WebSocket.
    let (mut ws_alice, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let reg = RelayMessage::Register {
        peer_id: "alice".to_string(),
    };
    let bytes = relay::encode(&reg).unwrap();
    ws_alice
        .send(ws::Message::Binary(bytes.into()))
        .await
        .unwrap();
    // Wait for Registered ack.
    let _ = ws_alice.next().await.unwrap().unwrap();

    // Bob connects normally.
    let bob = RelayTransport::connect(&url, PeerId::new("bob"))
        .await
        .unwrap();

    // Alice sends with a spoofed `from`.
    let spoofed = RelayMessage::RelayPayload {
        from: "evil-impersonator".to_string(),
        to: "bob".to_string(),
        payload: b"spoofed".to_vec(),
    };
    let bytes = relay::encode(&spoofed).unwrap();
    ws_alice
        .send(ws::Message::Binary(bytes.into()))
        .await
        .unwrap();

    // Bob receives the message — the relay should have overwritten `from`
    // with Alice's registered PeerId.
    let (from, data) = tokio::time::timeout(Duration::from_secs(5), bob.recv())
        .await
        .expect("recv timed out")
        .unwrap();

    assert_eq!(
        from,
        PeerId::new("alice"),
        "relay must enforce PeerId, not allow spoofing"
    );
    assert_eq!(data, b"spoofed");
}

#[tokio::test]
async fn connect_to_invalid_url_returns_error() {
    let result = RelayTransport::connect("ws://127.0.0.1:1", PeerId::new("alice")).await;
    assert!(result.is_err(), "should fail on invalid/unreachable URL");
}
