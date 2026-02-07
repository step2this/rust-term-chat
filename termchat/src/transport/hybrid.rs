//! Hybrid transport with fallback and offline queuing.
//!
//! [`HybridTransport`] wraps a preferred and a fallback [`Transport`].
//! When sending, it tries the preferred transport first. If that fails,
//! it falls back to the secondary transport. If both fail, the message
//! is queued in a [`PendingQueue`] and retried when connectivity is
//! restored.
//!
//! A background flush task can be spawned via [`HybridTransport::spawn_flush_task`]
//! to periodically drain the pending queue.

use std::collections::VecDeque;
use std::time::Duration;

use tokio::sync::Mutex;
use tracing;

use super::{PeerId, Transport, TransportError, TransportType};

/// A message waiting to be sent when transport becomes available.
#[derive(Debug, Clone)]
pub struct PendingMessage {
    /// The intended recipient.
    pub peer: PeerId,
    /// The encrypted payload.
    pub payload: Vec<u8>,
}

/// Queue of messages that could not be sent due to transport failure.
///
/// Messages are stored in FIFO order and flushed when transport
/// connectivity is restored.
pub struct PendingQueue {
    /// The queued messages, oldest first.
    queue: Mutex<VecDeque<PendingMessage>>,
}

impl PendingQueue {
    /// Create a new, empty pending queue.
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
        }
    }

    /// Add a message to the back of the queue.
    pub async fn enqueue(&self, peer: PeerId, payload: Vec<u8>) {
        let mut q = self.queue.lock().await;
        q.push_back(PendingMessage { peer, payload });
        tracing::info!(queue_len = q.len(), "message queued for offline delivery");
    }

    /// Remove and return the oldest message, if any.
    pub async fn dequeue(&self) -> Option<PendingMessage> {
        self.queue.lock().await.pop_front()
    }

    /// Return the number of messages currently queued.
    pub async fn len(&self) -> usize {
        self.queue.lock().await.len()
    }

    /// Return `true` if the queue is empty.
    pub async fn is_empty(&self) -> bool {
        self.queue.lock().await.is_empty()
    }

    /// Drain all messages from the queue, returning them as a vector.
    pub async fn drain_all(&self) -> Vec<PendingMessage> {
        let mut q = self.queue.lock().await;
        q.drain(..).collect()
    }
}

impl Default for PendingQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Transport that tries a preferred transport, falls back to a secondary,
/// and queues messages when both are unavailable.
///
/// # Type Parameters
///
/// - `P`: The preferred (primary) transport type.
/// - `F`: The fallback (secondary) transport type.
///
/// # Send behavior
///
/// 1. Try `preferred.send()`.
/// 2. If that fails, try `fallback.send()`.
/// 3. If both fail, enqueue the message in [`PendingQueue`].
///
/// # Recv behavior
///
/// Receives from whichever transport has data available first using
/// `tokio::select!`. Messages arriving on either the preferred or
/// fallback transport are returned as they arrive.
pub struct HybridTransport<P: Transport, F: Transport> {
    /// The preferred transport (e.g., P2P).
    preferred: P,
    /// The fallback transport (e.g., Relay).
    fallback: F,
    /// Queue of messages that failed to send on both transports.
    pub pending: PendingQueue,
}

impl<P: Transport, F: Transport> HybridTransport<P, F> {
    /// Create a new hybrid transport with the given preferred and fallback.
    #[must_use]
    pub fn new(preferred: P, fallback: F) -> Self {
        Self {
            preferred,
            fallback,
            pending: PendingQueue::new(),
        }
    }

    /// Attempt to flush all pending messages through the preferred transport,
    /// falling back to the secondary if needed.
    ///
    /// Returns the number of messages successfully sent.
    pub async fn flush_pending(&self) -> usize {
        let messages = self.pending.drain_all().await;
        let total = messages.len();
        let mut sent = 0;

        for msg in messages {
            if self.try_send(&msg.peer, &msg.payload).await.is_ok() {
                sent += 1;
            } else {
                // Re-queue messages that still cannot be sent.
                self.pending.enqueue(msg.peer, msg.payload).await;
            }
        }

        if sent > 0 {
            tracing::info!(sent, remaining = total - sent, "flushed pending messages");
        }

        sent
    }

    /// Spawn a background task that periodically flushes the pending queue.
    ///
    /// The task runs every `interval` and stops when the returned
    /// [`tokio::task::JoinHandle`] is aborted or the runtime shuts down.
    ///
    /// # Note
    ///
    /// Requires `Self` to be wrapped in an `Arc` for shared ownership.
    /// This is a convenience method; callers may also flush manually.
    pub fn spawn_flush_task(
        self: &std::sync::Arc<Self>,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()>
    where
        P: 'static,
        F: 'static,
    {
        let transport = std::sync::Arc::clone(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            loop {
                tick.tick().await;
                if !transport.pending.is_empty().await {
                    transport.flush_pending().await;
                }
            }
        })
    }

    /// Internal: try preferred, then fallback. Returns the first success
    /// or the last error.
    async fn try_send(&self, peer: &PeerId, payload: &[u8]) -> Result<(), TransportError> {
        match self.preferred.send(peer, payload).await {
            Ok(()) => Ok(()),
            Err(preferred_err) => {
                tracing::debug!(
                    transport = %self.preferred.transport_type(),
                    err = %preferred_err,
                    "preferred transport failed, trying fallback"
                );
                self.fallback.send(peer, payload).await
            }
        }
    }
}

impl<P: Transport, F: Transport> Transport for HybridTransport<P, F> {
    async fn send(&self, peer: &PeerId, payload: &[u8]) -> Result<(), TransportError> {
        match self.try_send(peer, payload).await {
            Ok(()) => Ok(()),
            Err(err) => {
                tracing::warn!(
                    err = %err,
                    "all transports failed, queuing message for later delivery"
                );
                self.pending.enqueue(peer.clone(), payload.to_vec()).await;
                // Return the error so the caller knows it was queued, not delivered.
                Err(err)
            }
        }
    }

    async fn recv(&self) -> Result<(PeerId, Vec<u8>), TransportError> {
        // Multiplex recv across both transports: whichever has data first wins.
        // Box::pin is needed because RPITIT futures may not be Unpin.
        tokio::select! {
            result = Box::pin(self.preferred.recv()) => result,
            result = Box::pin(self.fallback.recv()) => result,
        }
    }

    fn is_connected(&self, peer: &PeerId) -> bool {
        self.preferred.is_connected(peer) || self.fallback.is_connected(peer)
    }

    fn transport_type(&self) -> TransportType {
        // Report whichever transport is currently connected.
        // Prefer reporting the preferred transport's type.
        if self.preferred.is_connected(&PeerId::new("")) {
            self.preferred.transport_type()
        } else {
            self.fallback.transport_type()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::loopback::LoopbackTransport;

    /// Create a loopback pair for the "preferred" transport.
    fn preferred_pair() -> (LoopbackTransport, LoopbackTransport) {
        LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 32)
    }

    /// Create a loopback pair for the "fallback" transport.
    fn fallback_pair() -> (LoopbackTransport, LoopbackTransport) {
        LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 32)
    }

    #[tokio::test]
    async fn send_via_preferred_when_available() {
        let (pref_a, pref_b) = preferred_pair();
        let (fall_a, _fall_b) = fallback_pair();

        let hybrid = HybridTransport::new(pref_a, fall_a);
        hybrid
            .send(&PeerId::new("bob"), b"hello via preferred")
            .await
            .unwrap();

        let (_from, data) = pref_b.recv().await.unwrap();
        assert_eq!(data, b"hello via preferred");
    }

    #[tokio::test]
    async fn fallback_when_preferred_fails() {
        let (pref_a, pref_b) = preferred_pair();
        let (fall_a, fall_b) = fallback_pair();

        // Drop the preferred receiver to make its send fail.
        drop(pref_b);

        let hybrid = HybridTransport::new(pref_a, fall_a);
        hybrid
            .send(&PeerId::new("bob"), b"hello via fallback")
            .await
            .unwrap();

        let (_from, data) = fall_b.recv().await.unwrap();
        assert_eq!(data, b"hello via fallback");
    }

    #[tokio::test]
    async fn queue_when_both_transports_fail() {
        let (pref_a, pref_b) = preferred_pair();
        let (fall_a, fall_b) = fallback_pair();

        // Drop both receivers to make all sends fail.
        drop(pref_b);
        drop(fall_b);

        let hybrid = HybridTransport::new(pref_a, fall_a);
        let result = hybrid.send(&PeerId::new("bob"), b"offline msg").await;

        // Send returns an error (caller knows it was not delivered).
        assert!(result.is_err());

        // But the message is queued for later.
        assert_eq!(hybrid.pending.len().await, 1);

        let msg = hybrid.pending.dequeue().await.unwrap();
        assert_eq!(msg.peer, PeerId::new("bob"));
        assert_eq!(msg.payload, b"offline msg");
    }

    #[tokio::test]
    async fn flush_pending_delivers_queued_messages() {
        let (pref_a, pref_b) = preferred_pair();
        let (fall_a, fall_b) = fallback_pair();

        // Drop both to simulate offline.
        drop(pref_b);
        drop(fall_b);

        let hybrid = HybridTransport::new(pref_a, fall_a);
        let _ = hybrid.send(&PeerId::new("bob"), b"queued").await;
        assert_eq!(hybrid.pending.len().await, 1);

        // "Reconnect" is not possible with dropped loopback channels,
        // so flushing will re-queue the message.
        let sent = hybrid.flush_pending().await;
        assert_eq!(sent, 0);
        assert_eq!(hybrid.pending.len().await, 1);
    }

    #[tokio::test]
    async fn flush_pending_with_working_transport() {
        // Both transports work — nothing queued initially.
        let (pref_a, pref_b) = preferred_pair();
        let (fall_a, _fall_b) = fallback_pair();

        let hybrid = HybridTransport::new(pref_a, fall_a);

        // Manually enqueue a message (simulating a prior offline period).
        hybrid
            .pending
            .enqueue(PeerId::new("bob"), b"delayed".to_vec())
            .await;
        assert_eq!(hybrid.pending.len().await, 1);

        let sent = hybrid.flush_pending().await;
        assert_eq!(sent, 1);
        assert!(hybrid.pending.is_empty().await);

        // Verify the message actually arrived.
        let (_from, data) = pref_b.recv().await.unwrap();
        assert_eq!(data, b"delayed");
    }

    #[tokio::test]
    async fn is_connected_checks_both_transports() {
        let (pref_a, pref_b) = preferred_pair();
        let (fall_a, _fall_b) = fallback_pair();

        let hybrid = HybridTransport::new(pref_a, fall_a);
        assert!(hybrid.is_connected(&PeerId::new("bob")));

        // Drop preferred — fallback still connected.
        drop(pref_b);
        assert!(hybrid.is_connected(&PeerId::new("bob")));
    }

    #[tokio::test]
    async fn pending_queue_is_fifo() {
        let queue = PendingQueue::new();

        queue.enqueue(PeerId::new("bob"), b"first".to_vec()).await;
        queue.enqueue(PeerId::new("bob"), b"second".to_vec()).await;
        queue.enqueue(PeerId::new("bob"), b"third".to_vec()).await;

        assert_eq!(queue.len().await, 3);

        let m1 = queue.dequeue().await.unwrap();
        assert_eq!(m1.payload, b"first");
        let m2 = queue.dequeue().await.unwrap();
        assert_eq!(m2.payload, b"second");
        let m3 = queue.dequeue().await.unwrap();
        assert_eq!(m3.payload, b"third");

        assert!(queue.is_empty().await);
        assert!(queue.dequeue().await.is_none());
    }

    #[tokio::test]
    async fn multiple_offline_messages_queued_in_order() {
        let (pref_a, pref_b) = preferred_pair();
        let (fall_a, fall_b) = fallback_pair();

        drop(pref_b);
        drop(fall_b);

        let hybrid = HybridTransport::new(pref_a, fall_a);

        for i in 0u32..5 {
            let _ = hybrid.send(&PeerId::new("bob"), &i.to_le_bytes()).await;
        }

        assert_eq!(hybrid.pending.len().await, 5);

        let messages = hybrid.pending.drain_all().await;
        for (i, msg) in messages.iter().enumerate() {
            let val = u32::from_le_bytes(msg.payload.clone().try_into().unwrap());
            assert_eq!(val, i as u32);
        }
    }

    // --- T-004-14: HybridTransport recv multiplexing tests ---

    #[tokio::test]
    async fn recv_from_fallback_only() {
        let (pref_a, _pref_b) = preferred_pair();
        let (fall_a, fall_b) = fallback_pair();

        let hybrid = HybridTransport::new(pref_a, fall_a);

        // Send via the fallback transport's remote side.
        fall_b
            .send(&PeerId::new("alice"), b"via fallback")
            .await
            .unwrap();

        // recv() should return the fallback message.
        let (from, data) = tokio::time::timeout(std::time::Duration::from_secs(5), hybrid.recv())
            .await
            .expect("recv timed out")
            .unwrap();

        assert_eq!(from, PeerId::new("bob"));
        assert_eq!(data, b"via fallback");
    }

    #[tokio::test]
    async fn recv_from_preferred_still_works() {
        let (pref_a, pref_b) = preferred_pair();
        let (fall_a, _fall_b) = fallback_pair();

        let hybrid = HybridTransport::new(pref_a, fall_a);

        // Send via the preferred transport's remote side.
        pref_b
            .send(&PeerId::new("alice"), b"via preferred")
            .await
            .unwrap();

        let (from, data) = tokio::time::timeout(std::time::Duration::from_secs(5), hybrid.recv())
            .await
            .expect("recv timed out")
            .unwrap();

        assert_eq!(from, PeerId::new("bob"));
        assert_eq!(data, b"via preferred");
    }

    #[tokio::test]
    async fn recv_interleaved_from_both_transports() {
        let (pref_a, pref_b) = preferred_pair();
        let (fall_a, fall_b) = fallback_pair();

        let hybrid = HybridTransport::new(pref_a, fall_a);

        // Send messages on both transports.
        pref_b.send(&PeerId::new("alice"), b"pref-1").await.unwrap();
        fall_b.send(&PeerId::new("alice"), b"fall-1").await.unwrap();
        pref_b.send(&PeerId::new("alice"), b"pref-2").await.unwrap();
        fall_b.send(&PeerId::new("alice"), b"fall-2").await.unwrap();

        // Receive all four messages. Order between transports is not
        // deterministic (select! picks whichever is ready first), but
        // all messages must arrive.
        let mut received = Vec::new();
        for _ in 0..4 {
            let (_from, data) =
                tokio::time::timeout(std::time::Duration::from_secs(5), hybrid.recv())
                    .await
                    .expect("recv timed out")
                    .unwrap();
            received.push(data);
        }

        // Sort for deterministic comparison.
        received.sort();
        let mut expected: Vec<Vec<u8>> = vec![
            b"fall-1".to_vec(),
            b"fall-2".to_vec(),
            b"pref-1".to_vec(),
            b"pref-2".to_vec(),
        ];
        expected.sort();
        assert_eq!(received, expected);
    }

    #[tokio::test]
    async fn recv_returns_data_when_only_fallback_has_messages() {
        // Both transports are open. Only the fallback has data. The
        // preferred transport's recv() blocks (no data available).
        // select! should pick the fallback branch since it's ready.
        let (pref_a, _pref_b) = preferred_pair();
        let (fall_a, fall_b) = fallback_pair();

        let hybrid = HybridTransport::new(pref_a, fall_a);

        // Send on fallback only — preferred stays open but empty.
        fall_b
            .send(&PeerId::new("alice"), b"only fallback")
            .await
            .unwrap();

        let (from, data) = tokio::time::timeout(std::time::Duration::from_secs(5), hybrid.recv())
            .await
            .expect("recv timed out")
            .unwrap();

        assert_eq!(from, PeerId::new("bob"));
        assert_eq!(data, b"only fallback");
    }

    #[tokio::test]
    async fn recv_returns_data_when_only_preferred_has_messages() {
        // Inverse: only preferred has data, fallback blocks.
        let (pref_a, pref_b) = preferred_pair();
        let (fall_a, _fall_b) = fallback_pair();

        let hybrid = HybridTransport::new(pref_a, fall_a);

        pref_b
            .send(&PeerId::new("alice"), b"only preferred")
            .await
            .unwrap();

        let (from, data) = tokio::time::timeout(std::time::Duration::from_secs(5), hybrid.recv())
            .await
            .expect("recv timed out")
            .unwrap();

        assert_eq!(from, PeerId::new("bob"));
        assert_eq!(data, b"only preferred");
    }
}
