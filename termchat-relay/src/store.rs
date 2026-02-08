//! In-memory store-and-forward message queue for offline peers.
//!
//! The [`MessageStore`] holds per-peer FIFO queues of messages that could not
//! be delivered because the recipient was not connected at the time. When a
//! peer registers, its queue is drained and all stored messages are delivered.

use std::collections::{HashMap, VecDeque};
use tokio::sync::RwLock;
use tokio::time::Instant;

/// Default maximum number of queued messages per peer before FIFO eviction.
const DEFAULT_MAX_QUEUE_SIZE: usize = 1000;

/// A message stored for later delivery to an offline peer.
#[derive(Debug, Clone)]
pub struct StoredMessage {
    /// `PeerId` of the sender.
    pub from: String,
    /// Opaque encrypted payload bytes.
    pub payload: Vec<u8>,
    /// When the message was enqueued (used for future TTL expiry).
    #[allow(dead_code)]
    pub queued_at: Instant,
}

/// In-memory per-peer message queue with FIFO eviction.
///
/// Thread-safe via [`RwLock`]. Each peer has an independent queue capped at
/// a configurable maximum; when the cap is exceeded the oldest message
/// is dropped.
pub struct MessageStore {
    queues: RwLock<HashMap<String, VecDeque<StoredMessage>>>,
    max_queue_size: usize,
}

impl Default for MessageStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageStore {
    /// Creates a new, empty message store with the default queue size limit.
    #[must_use]
    pub fn new() -> Self {
        Self {
            queues: RwLock::new(HashMap::new()),
            max_queue_size: DEFAULT_MAX_QUEUE_SIZE,
        }
    }

    /// Creates a new, empty message store with a custom queue size limit.
    #[must_use]
    pub fn with_max_queue_size(max_queue_size: usize) -> Self {
        Self {
            queues: RwLock::new(HashMap::new()),
            max_queue_size,
        }
    }

    /// Enqueues a message for the given peer, returning the new queue length.
    ///
    /// If the peer's queue exceeds the configured maximum, the oldest message
    /// is evicted (FIFO).
    #[allow(clippy::cast_possible_truncation)]
    pub async fn enqueue(&self, to: &str, from: &str, payload: Vec<u8>) -> u32 {
        let mut queues = self.queues.write().await;
        let queue = queues.entry(to.to_string()).or_default();
        queue.push_back(StoredMessage {
            from: from.to_string(),
            payload,
            queued_at: Instant::now(),
        });
        if queue.len() > self.max_queue_size {
            queue.pop_front();
        }
        // Safe: max_queue_size is bounded, well within u32 range.
        let len = queue.len() as u32;
        drop(queues);
        len
    }

    /// Drains all queued messages for a peer, returning them in FIFO order.
    ///
    /// The peer's queue is empty after this call. Returns an empty `Vec` if the
    /// peer has no queued messages.
    pub async fn drain(&self, peer_id: &str) -> Vec<StoredMessage> {
        let mut queues = self.queues.write().await;
        queues
            .remove(peer_id)
            .map(|q| q.into_iter().collect())
            .unwrap_or_default()
    }

    /// Returns the number of messages currently queued for a peer.
    #[allow(dead_code, clippy::cast_possible_truncation)]
    pub async fn queue_len(&self, peer_id: &str) -> u32 {
        let queues = self.queues.read().await;
        // Safe: MAX_QUEUE_SIZE is 1000, well within u32 range.
        queues.get(peer_id).map_or(0, |q| q.len() as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn enqueue_and_drain_round_trip() {
        let store = MessageStore::new();
        store.enqueue("bob", "alice", vec![1, 2, 3]).await;
        store.enqueue("bob", "carol", vec![4, 5]).await;

        let msgs = store.drain("bob").await;
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].from, "alice");
        assert_eq!(msgs[0].payload, vec![1, 2, 3]);
        assert_eq!(msgs[1].from, "carol");
        assert_eq!(msgs[1].payload, vec![4, 5]);
    }

    #[tokio::test]
    async fn drain_preserves_fifo_order() {
        let store = MessageStore::new();
        for i in 0..10u8 {
            store.enqueue("peer", "sender", vec![i]).await;
        }
        let msgs = store.drain("peer").await;
        for (i, msg) in msgs.iter().enumerate() {
            assert_eq!(msg.payload, vec![i as u8]);
        }
    }

    #[tokio::test]
    async fn fifo_eviction_at_cap() {
        let store = MessageStore::new();
        // Enqueue 1001 messages — the first should be evicted.
        for i in 0..1001u32 {
            store
                .enqueue("peer", "sender", i.to_le_bytes().to_vec())
                .await;
        }
        let msgs = store.drain("peer").await;
        assert_eq!(msgs.len(), 1000);
        // The first message (i=0) should have been evicted; oldest is i=1.
        assert_eq!(msgs[0].payload, 1u32.to_le_bytes().to_vec());
        // The last message should be i=1000.
        assert_eq!(msgs[999].payload, 1000u32.to_le_bytes().to_vec());
    }

    #[tokio::test]
    async fn drain_empty_for_unknown_peer() {
        let store = MessageStore::new();
        let msgs = store.drain("unknown").await;
        assert!(msgs.is_empty());
    }

    #[tokio::test]
    async fn independent_per_peer_queues() {
        let store = MessageStore::new();
        store.enqueue("alice", "sender", vec![1]).await;
        store.enqueue("bob", "sender", vec![2]).await;

        let alice_msgs = store.drain("alice").await;
        let bob_msgs = store.drain("bob").await;
        assert_eq!(alice_msgs.len(), 1);
        assert_eq!(alice_msgs[0].payload, vec![1]);
        assert_eq!(bob_msgs.len(), 1);
        assert_eq!(bob_msgs[0].payload, vec![2]);
    }

    #[tokio::test]
    async fn queue_len_reflects_current_state() {
        let store = MessageStore::new();
        assert_eq!(store.queue_len("peer").await, 0);

        store.enqueue("peer", "sender", vec![1]).await;
        assert_eq!(store.queue_len("peer").await, 1);

        store.enqueue("peer", "sender", vec![2]).await;
        assert_eq!(store.queue_len("peer").await, 2);

        store.drain("peer").await;
        assert_eq!(store.queue_len("peer").await, 0);
    }

    #[tokio::test]
    async fn drain_clears_queue() {
        let store = MessageStore::new();
        store.enqueue("peer", "sender", vec![1]).await;
        store.drain("peer").await;

        let msgs = store.drain("peer").await;
        assert!(msgs.is_empty());
    }
}
