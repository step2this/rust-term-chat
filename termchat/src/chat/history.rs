//! Local message history storage and failure-resilient writing.
//!
//! Defines the [`MessageStore`] trait for persisting messages and their
//! delivery status, plus [`ResilientHistoryWriter`] which wraps any store
//! to handle write failures gracefully (Extension 8a).
//!
//! # Extension 8a — History Write Failure
//!
//! If `MessageStore::save()` fails (disk full, database error, etc.):
//! 1. The error is logged (never crashes the application).
//! 2. The message delivery is still reported as successful to the caller.
//! 3. The failed write is queued for retry.
//! 4. A warning is emitted so the UI can display:
//!    "Message delivered but could not save to history".

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use tokio::sync::Mutex;

use termchat_proto::message::{ChatMessage, ConversationId, MessageId, MessageStatus};

/// Errors that can occur during history storage operations.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The underlying storage is full or unavailable.
    #[error("storage unavailable: {0}")]
    Unavailable(String),

    /// A write operation failed.
    #[error("write failed: {0}")]
    WriteFailed(String),

    /// A read operation failed.
    #[error("read failed: {0}")]
    ReadFailed(String),

    /// The requested item was not found.
    #[error("not found: {0}")]
    NotFound(String),
}

/// Trait for persisting chat messages and their delivery status.
///
/// Implementations include:
/// - `InMemoryStore` — in-memory store for testing (T-001-10)
/// - `SQLite` store -- persistent storage (Sprint 5)
pub trait MessageStore: Send + Sync {
    /// Save a message with its current delivery status.
    fn save(
        &self,
        msg: &ChatMessage,
        status: MessageStatus,
    ) -> impl std::future::Future<Output = Result<(), StoreError>> + Send;

    /// Update the delivery status of an existing message.
    fn update_status(
        &self,
        id: &MessageId,
        status: MessageStatus,
    ) -> impl std::future::Future<Output = Result<(), StoreError>> + Send;

    /// Retrieve messages from a conversation, most recent first.
    ///
    /// Returns up to `limit` messages along with their current status.
    fn get_conversation(
        &self,
        conversation: &ConversationId,
        limit: usize,
    ) -> impl std::future::Future<Output = Result<Vec<(ChatMessage, MessageStatus)>, StoreError>> + Send;
}

/// A history write operation that failed and needs to be retried.
#[derive(Debug, Clone)]
enum PendingWrite {
    /// A message save that failed.
    Save {
        /// The message to save.
        message: ChatMessage,
        /// The status at the time of the failed save.
        status: MessageStatus,
    },
    /// A status update that failed.
    StatusUpdate {
        /// The message whose status needs updating.
        message_id: MessageId,
        /// The new status.
        status: MessageStatus,
    },
}

/// Warning emitted when a history write fails.
///
/// The UI layer should watch for these and display a non-blocking
/// notification to the user.
#[derive(Debug, Clone)]
pub enum HistoryWarning {
    /// A message was delivered but could not be saved to history.
    SaveFailed {
        /// The ID of the message that could not be saved.
        message_id: MessageId,
        /// Description of the error.
        reason: String,
    },
    /// A status update could not be persisted.
    StatusUpdateFailed {
        /// The ID of the message whose status could not be updated.
        message_id: MessageId,
        /// Description of the error.
        reason: String,
    },
}

/// Wraps a [`MessageStore`] to handle write failures gracefully.
///
/// When a write fails, the `ResilientHistoryWriter`:
/// 1. Logs the error via `tracing::warn!`
/// 2. Queues the failed write for retry
/// 3. Emits a [`HistoryWarning`] through a channel
/// 4. Returns `Ok(())` to the caller so message delivery is not disrupted
///
/// Failed writes can be retried manually via [`flush_pending`](Self::flush_pending)
/// or automatically via [`spawn_retry_task`](Self::spawn_retry_task).
pub struct ResilientHistoryWriter<S: MessageStore> {
    /// The underlying store.
    store: S,
    /// Queue of writes that need to be retried.
    pending: Mutex<VecDeque<PendingWrite>>,
    /// Channel for emitting warnings to the UI layer.
    warning_tx: tokio::sync::mpsc::Sender<HistoryWarning>,
}

impl<S: MessageStore> ResilientHistoryWriter<S> {
    /// Create a new resilient writer wrapping the given store.
    ///
    /// Returns the writer and a receiver for [`HistoryWarning`] events
    /// that the UI can consume.
    #[must_use]
    pub fn new(
        store: S,
        warning_buffer: usize,
    ) -> (Self, tokio::sync::mpsc::Receiver<HistoryWarning>) {
        let (tx, rx) = tokio::sync::mpsc::channel(warning_buffer);
        let writer = Self {
            store,
            pending: Mutex::new(VecDeque::new()),
            warning_tx: tx,
        };
        (writer, rx)
    }

    /// Save a message, handling failures gracefully.
    ///
    /// If the underlying store fails, the write is queued for retry
    /// and a warning is emitted. The caller always gets `Ok(())` so
    /// that message delivery is not disrupted.
    pub async fn save(&self, msg: &ChatMessage, status: MessageStatus) {
        if let Err(err) = self.store.save(msg, status.clone()).await {
            let msg_id = msg.metadata.message_id.clone();
            tracing::warn!(
                message_id = %msg_id,
                error = %err,
                "history save failed — message delivered but not persisted"
            );

            self.pending.lock().await.push_back(PendingWrite::Save {
                message: msg.clone(),
                status,
            });

            // Best-effort warning emission — if the channel is full, drop it.
            let _ = self.warning_tx.try_send(HistoryWarning::SaveFailed {
                message_id: msg_id,
                reason: err.to_string(),
            });
        }
    }

    /// Update a message's status, handling failures gracefully.
    ///
    /// Same resilience behavior as [`save`](Self::save).
    pub async fn update_status(&self, id: &MessageId, status: MessageStatus) {
        if let Err(err) = self.store.update_status(id, status.clone()).await {
            tracing::warn!(
                message_id = %id,
                error = %err,
                "history status update failed"
            );

            self.pending
                .lock()
                .await
                .push_back(PendingWrite::StatusUpdate {
                    message_id: id.clone(),
                    status,
                });

            let _ = self
                .warning_tx
                .try_send(HistoryWarning::StatusUpdateFailed {
                    message_id: id.clone(),
                    reason: err.to_string(),
                });
        }
    }

    /// Delegate reads directly to the underlying store.
    ///
    /// Read failures are NOT handled resiliently -- they bubble up to the
    /// caller, since there is nothing useful to retry.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if the underlying store read fails.
    pub async fn get_conversation(
        &self,
        conversation: &ConversationId,
        limit: usize,
    ) -> Result<Vec<(ChatMessage, MessageStatus)>, StoreError> {
        self.store.get_conversation(conversation, limit).await
    }

    /// Attempt to flush all pending writes through the store.
    ///
    /// Returns the number of writes successfully completed.
    pub async fn flush_pending(&self) -> usize {
        let writes: Vec<PendingWrite> = {
            let mut q = self.pending.lock().await;
            q.drain(..).collect()
        };

        let total = writes.len();
        let mut succeeded = 0;

        for write in writes {
            let result = match &write {
                PendingWrite::Save { message, status } => {
                    self.store.save(message, status.clone()).await
                }
                PendingWrite::StatusUpdate { message_id, status } => {
                    self.store.update_status(message_id, status.clone()).await
                }
            };

            if result.is_ok() {
                succeeded += 1;
            } else {
                // Re-queue for another attempt.
                self.pending.lock().await.push_back(write);
            }
        }

        if succeeded > 0 {
            tracing::info!(
                succeeded,
                remaining = total - succeeded,
                "flushed pending history writes"
            );
        }

        succeeded
    }

    /// Return the number of pending writes awaiting retry.
    pub async fn pending_count(&self) -> usize {
        self.pending.lock().await.len()
    }

    /// Spawn a background task that periodically flushes pending writes.
    ///
    /// The task runs every `interval` and stops when the returned
    /// [`tokio::task::JoinHandle`] is aborted or the runtime shuts down.
    pub fn spawn_retry_task(
        self: &std::sync::Arc<Self>,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()>
    where
        S: 'static,
    {
        let writer = std::sync::Arc::clone(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            loop {
                tick.tick().await;
                if writer.pending_count().await > 0 {
                    writer.flush_pending().await;
                }
            }
        })
    }
}

/// In-memory implementation of [`MessageStore`] for testing.
///
/// Stores messages in a `HashMap` keyed by [`MessageId`]. Messages are
/// grouped by conversation for retrieval. This store is not persistent --
/// all data is lost when the process exits.
///
/// For persistent storage, a SQLite-backed implementation will be added
/// in Sprint 5 (UC-006).
pub struct InMemoryStore {
    /// Messages keyed by their ID, along with their current status.
    messages: Mutex<HashMap<MessageId, (ChatMessage, MessageStatus)>>,
}

impl InMemoryStore {
    /// Create a new, empty in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            messages: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageStore for InMemoryStore {
    async fn save(&self, msg: &ChatMessage, status: MessageStatus) -> Result<(), StoreError> {
        self.messages
            .lock()
            .await
            .insert(msg.metadata.message_id.clone(), (msg.clone(), status));
        Ok(())
    }

    async fn update_status(&self, id: &MessageId, status: MessageStatus) -> Result<(), StoreError> {
        let mut messages = self.messages.lock().await;
        match messages.get_mut(id) {
            Some(entry) => {
                entry.1 = status;
                Ok(())
            }
            None => Err(StoreError::NotFound(format!("message {id}"))),
        }
    }

    async fn get_conversation(
        &self,
        conversation: &ConversationId,
        limit: usize,
    ) -> Result<Vec<(ChatMessage, MessageStatus)>, StoreError> {
        let mut results: Vec<(ChatMessage, MessageStatus)> = self
            .messages
            .lock()
            .await
            .values()
            .filter(|(msg, _)| msg.metadata.conversation_id == *conversation)
            .cloned()
            .collect();

        // Sort by timestamp, most recent first
        results.sort_by(|a, b| b.0.metadata.timestamp.cmp(&a.0.metadata.timestamp));
        results.truncate(limit);

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use termchat_proto::message::{
        ChatMessage, ConversationId, MessageContent, MessageId, MessageMetadata, MessageStatus,
        SenderId, Timestamp,
    };

    /// A test store that can be configured to fail on save/update_status.
    struct FailingStore {
        should_fail: AtomicBool,
    }

    impl FailingStore {
        fn new(should_fail: bool) -> Self {
            Self {
                should_fail: AtomicBool::new(should_fail),
            }
        }

        fn set_failing(&self, fail: bool) {
            self.should_fail.store(fail, Ordering::SeqCst);
        }
    }

    impl MessageStore for FailingStore {
        async fn save(&self, _msg: &ChatMessage, _status: MessageStatus) -> Result<(), StoreError> {
            if self.should_fail.load(Ordering::SeqCst) {
                Err(StoreError::WriteFailed("disk full".to_string()))
            } else {
                Ok(())
            }
        }

        async fn update_status(
            &self,
            _id: &MessageId,
            _status: MessageStatus,
        ) -> Result<(), StoreError> {
            if self.should_fail.load(Ordering::SeqCst) {
                Err(StoreError::WriteFailed("disk full".to_string()))
            } else {
                Ok(())
            }
        }

        async fn get_conversation(
            &self,
            _conversation: &ConversationId,
            _limit: usize,
        ) -> Result<Vec<(ChatMessage, MessageStatus)>, StoreError> {
            Ok(vec![])
        }
    }

    fn make_test_message() -> ChatMessage {
        ChatMessage {
            metadata: MessageMetadata {
                message_id: MessageId::new(),
                timestamp: Timestamp::now(),
                sender_id: SenderId::new(vec![1, 2, 3]),
                conversation_id: ConversationId::new(),
            },
            content: MessageContent::Text("test message".into()),
        }
    }

    #[tokio::test]
    async fn save_failure_does_not_crash() {
        let store = FailingStore::new(true);
        let (writer, _rx) = ResilientHistoryWriter::new(store, 16);

        let msg = make_test_message();
        // This should NOT panic or return an error.
        writer.save(&msg, MessageStatus::Delivered).await;
    }

    #[tokio::test]
    async fn save_failure_queues_for_retry() {
        let store = FailingStore::new(true);
        let (writer, _rx) = ResilientHistoryWriter::new(store, 16);

        let msg = make_test_message();
        writer.save(&msg, MessageStatus::Delivered).await;

        assert_eq!(writer.pending_count().await, 1);
    }

    #[tokio::test]
    async fn save_failure_emits_warning() {
        let store = FailingStore::new(true);
        let (writer, mut rx) = ResilientHistoryWriter::new(store, 16);

        let msg = make_test_message();
        let expected_id = msg.metadata.message_id.clone();
        writer.save(&msg, MessageStatus::Delivered).await;

        let warning = rx.try_recv().unwrap();
        match warning {
            HistoryWarning::SaveFailed { message_id, reason } => {
                assert_eq!(message_id, expected_id);
                assert!(reason.contains("disk full"));
            }
            _ => panic!("expected SaveFailed warning"),
        }
    }

    #[tokio::test]
    async fn successful_save_does_not_queue() {
        let store = FailingStore::new(false);
        let (writer, _rx) = ResilientHistoryWriter::new(store, 16);

        let msg = make_test_message();
        writer.save(&msg, MessageStatus::Delivered).await;

        assert_eq!(writer.pending_count().await, 0);
    }

    #[tokio::test]
    async fn successful_save_does_not_emit_warning() {
        let store = FailingStore::new(false);
        let (writer, mut rx) = ResilientHistoryWriter::new(store, 16);

        let msg = make_test_message();
        writer.save(&msg, MessageStatus::Delivered).await;

        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn status_update_failure_queues_and_warns() {
        let store = FailingStore::new(true);
        let (writer, mut rx) = ResilientHistoryWriter::new(store, 16);

        let msg_id = MessageId::new();
        writer
            .update_status(&msg_id, MessageStatus::Delivered)
            .await;

        assert_eq!(writer.pending_count().await, 1);

        let warning = rx.try_recv().unwrap();
        assert!(matches!(warning, HistoryWarning::StatusUpdateFailed { .. }));
    }

    #[tokio::test]
    async fn flush_pending_retries_successfully() {
        let store = FailingStore::new(true);
        let (writer, _rx) = ResilientHistoryWriter::new(store, 16);

        let msg = make_test_message();
        writer.save(&msg, MessageStatus::Delivered).await;
        assert_eq!(writer.pending_count().await, 1);

        // "Fix" the store so retries succeed.
        writer.store.set_failing(false);

        let flushed = writer.flush_pending().await;
        assert_eq!(flushed, 1);
        assert_eq!(writer.pending_count().await, 0);
    }

    #[tokio::test]
    async fn flush_pending_requeues_on_continued_failure() {
        let store = FailingStore::new(true);
        let (writer, _rx) = ResilientHistoryWriter::new(store, 16);

        let msg = make_test_message();
        writer.save(&msg, MessageStatus::Delivered).await;
        assert_eq!(writer.pending_count().await, 1);

        // Store still failing — flush should re-queue.
        let flushed = writer.flush_pending().await;
        assert_eq!(flushed, 0);
        assert_eq!(writer.pending_count().await, 1);
    }

    // --- InMemoryStore tests ---

    #[tokio::test]
    async fn in_memory_save_and_retrieve() {
        let store = InMemoryStore::new();
        let msg = make_test_message();
        let conv_id = msg.metadata.conversation_id.clone();

        store.save(&msg, MessageStatus::Sent).await.unwrap();

        let results = store.get_conversation(&conv_id, 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, msg);
        assert_eq!(results[0].1, MessageStatus::Sent);
    }

    #[tokio::test]
    async fn in_memory_update_status() {
        let store = InMemoryStore::new();
        let msg = make_test_message();
        let msg_id = msg.metadata.message_id.clone();
        let conv_id = msg.metadata.conversation_id.clone();

        store.save(&msg, MessageStatus::Sent).await.unwrap();
        store
            .update_status(&msg_id, MessageStatus::Delivered)
            .await
            .unwrap();

        let results = store.get_conversation(&conv_id, 10).await.unwrap();
        assert_eq!(results[0].1, MessageStatus::Delivered);
    }

    #[tokio::test]
    async fn in_memory_update_nonexistent_returns_not_found() {
        let store = InMemoryStore::new();
        let result = store
            .update_status(&MessageId::new(), MessageStatus::Delivered)
            .await;
        assert!(matches!(result, Err(StoreError::NotFound(_))));
    }

    #[tokio::test]
    async fn in_memory_get_conversation_filters_by_conversation() {
        let store = InMemoryStore::new();
        let conv1 = ConversationId::new();
        let conv2 = ConversationId::new();

        let msg1 = ChatMessage {
            metadata: MessageMetadata {
                message_id: MessageId::new(),
                timestamp: Timestamp::from_millis(1000),
                sender_id: SenderId::new(vec![1]),
                conversation_id: conv1.clone(),
            },
            content: MessageContent::Text("in conv1".into()),
        };

        let msg2 = ChatMessage {
            metadata: MessageMetadata {
                message_id: MessageId::new(),
                timestamp: Timestamp::from_millis(2000),
                sender_id: SenderId::new(vec![2]),
                conversation_id: conv2.clone(),
            },
            content: MessageContent::Text("in conv2".into()),
        };

        store.save(&msg1, MessageStatus::Sent).await.unwrap();
        store.save(&msg2, MessageStatus::Sent).await.unwrap();

        let results1 = store.get_conversation(&conv1, 10).await.unwrap();
        assert_eq!(results1.len(), 1);
        assert_eq!(results1[0].0, msg1);

        let results2 = store.get_conversation(&conv2, 10).await.unwrap();
        assert_eq!(results2.len(), 1);
        assert_eq!(results2[0].0, msg2);
    }

    #[tokio::test]
    async fn in_memory_get_conversation_respects_limit() {
        let store = InMemoryStore::new();
        let conv = ConversationId::new();

        for i in 0..10u64 {
            let msg = ChatMessage {
                metadata: MessageMetadata {
                    message_id: MessageId::new(),
                    timestamp: Timestamp::from_millis(i),
                    sender_id: SenderId::new(vec![1]),
                    conversation_id: conv.clone(),
                },
                content: MessageContent::Text(format!("msg {i}")),
            };
            store.save(&msg, MessageStatus::Sent).await.unwrap();
        }

        let results = store.get_conversation(&conv, 3).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn in_memory_get_conversation_returns_most_recent_first() {
        let store = InMemoryStore::new();
        let conv = ConversationId::new();

        for i in 0..5u64 {
            let msg = ChatMessage {
                metadata: MessageMetadata {
                    message_id: MessageId::new(),
                    timestamp: Timestamp::from_millis(i * 100),
                    sender_id: SenderId::new(vec![1]),
                    conversation_id: conv.clone(),
                },
                content: MessageContent::Text(format!("msg {i}")),
            };
            store.save(&msg, MessageStatus::Sent).await.unwrap();
        }

        let results = store.get_conversation(&conv, 5).await.unwrap();
        // Most recent first
        for i in 0..4 {
            assert!(
                results[i].0.metadata.timestamp >= results[i + 1].0.metadata.timestamp,
                "messages should be sorted most recent first"
            );
        }
    }

    #[tokio::test]
    async fn in_memory_empty_conversation() {
        let store = InMemoryStore::new();
        let results = store
            .get_conversation(&ConversationId::new(), 10)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    // --- ResilientHistoryWriter tests ---

    #[tokio::test]
    async fn multiple_failures_queue_in_order() {
        let store = FailingStore::new(true);
        let (writer, _rx) = ResilientHistoryWriter::new(store, 16);

        for _ in 0..5 {
            let msg = make_test_message();
            writer.save(&msg, MessageStatus::Delivered).await;
        }

        assert_eq!(writer.pending_count().await, 5);
    }
}
