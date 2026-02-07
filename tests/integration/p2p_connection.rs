//! Integration tests for UC-003: Establish P2P Connection.
//!
//! Validates all success postconditions from the use case document.
//! Run with: `cargo test --test p2p_connection`

use std::net::SocketAddr;
use std::time::Duration;

use termchat::transport::quic::{QuicListener, QuicTransport};
use termchat::transport::{PeerId, Transport, TransportError, TransportType};

/// Helper: bind a listener and connect one client, returning (client, server_side).
async fn create_connected_pair() -> (QuicTransport, QuicTransport) {
    let listener = QuicListener::bind(
        "127.0.0.1:0".parse().expect("valid addr"),
        PeerId::new("responder"),
    )
    .expect("listener bind");

    let addr = listener.local_addr().expect("local addr");

    let accept_handle = tokio::spawn(async move { listener.accept().await });

    let initiator = QuicTransport::connect(
        addr,
        PeerId::new("initiator"),
        PeerId::new(addr.to_string()),
    )
    .await
    .expect("connect");

    let responder = accept_handle.await.expect("accept task").expect("accept");

    (initiator, responder)
}

// -----------------------------------------------------------------------
// Postcondition 1: Bidirectional QUIC connection between Initiator and
//                   Responder.
// -----------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_peer_connection_and_bidirectional_messaging() {
    let (initiator, responder) = create_connected_pair().await;

    // Initiator -> Responder
    initiator
        .send(initiator.remote_id(), b"hello from initiator")
        .await
        .expect("send i->r");
    let (from, data) = responder.recv().await.expect("recv i->r");
    assert_eq!(from, *responder.remote_id());
    assert_eq!(data, b"hello from initiator");

    // Responder -> Initiator
    responder
        .send(responder.remote_id(), b"hello from responder")
        .await
        .expect("send r->i");
    let (from, data) = initiator.recv().await.expect("recv r->i");
    assert_eq!(from, *initiator.remote_id());
    assert_eq!(data, b"hello from responder");
}

// -----------------------------------------------------------------------
// Postcondition 3 + 4: Transport trait fully satisfied and transport_type
//                       returns P2p.
// -----------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transport_type_returns_p2p_for_both_sides() {
    let (initiator, responder) = create_connected_pair().await;
    assert_eq!(initiator.transport_type(), TransportType::P2p);
    assert_eq!(responder.transport_type(), TransportType::P2p);
}

// -----------------------------------------------------------------------
// Postcondition 7: Connection timeout is enforced (default 10s).
// -----------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connection_timeout_enforced() {
    // Use a very short timeout connecting to a TEST-NET address that should
    // be unreachable, verifying the timeout fires.
    let addr: SocketAddr = "192.0.2.1:12345".parse().expect("valid addr");
    let start = std::time::Instant::now();
    let result = QuicTransport::connect_with_timeout(
        addr,
        PeerId::new("initiator"),
        PeerId::new("unreachable"),
        Duration::from_millis(500),
    )
    .await;

    assert!(
        result.is_err(),
        "connection to unreachable address should fail"
    );
    let elapsed = start.elapsed();
    // Should fail within roughly the timeout window (allowing generous margin).
    assert!(
        elapsed < Duration::from_secs(5),
        "should not hang indefinitely; elapsed: {elapsed:?}"
    );
}

// -----------------------------------------------------------------------
// Postcondition 3: is_connected reflects actual state.
// -----------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn is_connected_reflects_actual_state() {
    let (initiator, responder) = create_connected_pair().await;

    // Both should be connected after establishment.
    assert!(initiator.is_connected(initiator.remote_id()));
    assert!(responder.is_connected(responder.remote_id()));

    // Unknown peer should not be connected.
    assert!(!initiator.is_connected(&PeerId::new("nobody")));
}

// -----------------------------------------------------------------------
// Postcondition 2: Large payload round-trip (near 64KB limit).
// -----------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn large_payload_round_trip_near_limit() {
    let (initiator, responder) = create_connected_pair().await;

    // 64 KB payload — the maximum allowed.
    let payload: Vec<u8> = (0..65_536usize).map(|i| (i % 256) as u8).collect();
    initiator
        .send(initiator.remote_id(), &payload)
        .await
        .expect("send 64KB");

    let (_, data) = responder.recv().await.expect("recv 64KB");
    assert_eq!(data.len(), 65_536);
    assert_eq!(data, payload);
}

// -----------------------------------------------------------------------
// Invariant 2: Message ordering preserved within the connection.
// -----------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multiple_sequential_messages_preserve_fifo_order() {
    let (initiator, responder) = create_connected_pair().await;

    let count = 50u32;
    for i in 0..count {
        initiator
            .send(initiator.remote_id(), &i.to_le_bytes())
            .await
            .expect("send");
    }

    for i in 0..count {
        let (_, data) = responder.recv().await.expect("recv");
        let received = u32::from_le_bytes(data.try_into().expect("4 bytes"));
        assert_eq!(received, i, "FIFO order violated at index {i}");
    }
}

// -----------------------------------------------------------------------
// Invariant 3: Connection drop detected by is_connected().
// -----------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connection_drop_detection() {
    let (initiator, responder) = create_connected_pair().await;
    let remote_id = initiator.remote_id().clone();

    // Drop the responder to trigger connection close.
    drop(responder);

    // Wait for the close to propagate through QUIC.
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(
        !initiator.is_connected(&remote_id),
        "is_connected should return false after remote closes"
    );

    // Subsequent send should fail.
    let send_result = initiator.send(&remote_id, b"hello?").await;
    assert!(
        send_result.is_err(),
        "send after connection drop should fail"
    );

    // Subsequent recv should fail.
    let recv_result = initiator.recv().await;
    assert!(
        recv_result.is_err(),
        "recv after connection drop should fail"
    );
}

// -----------------------------------------------------------------------
// Failure Postcondition 4: Listener continues accepting after a failed
//                           connection.
// -----------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn listener_accepts_multiple_connections_sequentially() {
    let listener = QuicListener::bind(
        "127.0.0.1:0".parse().expect("valid addr"),
        PeerId::new("server"),
    )
    .expect("bind");

    let addr = listener.local_addr().expect("local addr");

    // First connection.
    let c1_handle = tokio::spawn(async move {
        QuicTransport::connect(addr, PeerId::new("client-1"), PeerId::new(addr.to_string())).await
    });
    let server1 = listener.accept().await.expect("accept 1");
    let client1 = c1_handle.await.expect("join 1").expect("connect 1");

    // Verify first connection works.
    client1
        .send(client1.remote_id(), b"msg-1")
        .await
        .expect("send 1");
    let (_, data) = server1.recv().await.expect("recv 1");
    assert_eq!(data, b"msg-1");

    // Second connection — same listener.
    let c2_handle = tokio::spawn(async move {
        QuicTransport::connect(addr, PeerId::new("client-2"), PeerId::new(addr.to_string())).await
    });
    let server2 = listener.accept().await.expect("accept 2");
    let client2 = c2_handle.await.expect("join 2").expect("connect 2");

    // Verify second connection works.
    client2
        .send(client2.remote_id(), b"msg-2")
        .await
        .expect("send 2");
    let (_, data) = server2.recv().await.expect("recv 2");
    assert_eq!(data, b"msg-2");
}

// -----------------------------------------------------------------------
// Extension 8a: PeerId validation (send to wrong peer -> Unreachable).
// -----------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn send_to_wrong_peer_returns_unreachable() {
    let (initiator, _responder) = create_connected_pair().await;

    let wrong_peer = PeerId::new("wrong-peer-id");
    let result = initiator.send(&wrong_peer, b"should fail").await;
    assert!(
        matches!(result, Err(TransportError::Unreachable(ref p)) if *p == wrong_peer),
        "expected Unreachable(wrong-peer-id), got {result:?}"
    );
}

// -----------------------------------------------------------------------
// Invariant 1: Transport never inspects/modifies payload bytes.
// -----------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn payload_bytes_are_opaque() {
    let (initiator, responder) = create_connected_pair().await;

    // Send binary data containing every byte value and some patterns that
    // might trip up naive text handling (null bytes, 0xFF, length-prefix-like
    // sequences).
    let mut payload = Vec::with_capacity(512);
    for b in 0u16..256 {
        payload.push(b as u8);
    }
    // Add some adversarial patterns: a fake length prefix.
    payload.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);

    initiator
        .send(initiator.remote_id(), &payload)
        .await
        .expect("send opaque");
    let (_, data) = responder.recv().await.expect("recv opaque");
    assert_eq!(data, payload, "transport must not modify payload bytes");
}
