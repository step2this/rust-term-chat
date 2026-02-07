//! Chat application layer for TermChat.
//!
//! Contains the [`ChatManager`] which orchestrates the send pipeline
//! (validate -> serialize -> encrypt -> transmit), delivery acknowledgment
//! flow, message status tracking, and local history persistence.

pub mod history;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use tokio::sync::{Mutex, mpsc};

use termchat_proto::codec;
use termchat_proto::message::{
    ChatMessage, ConversationId, DeliveryAck, Envelope, MessageContent, MessageId, MessageMetadata,
    MessageStatus, Nack, NackReason, SenderId, Timestamp,
};

use crate::crypto::{CryptoError, CryptoSession};
use crate::transport::{PeerId, Transport, TransportError};

use history::{MessageStore, ResilientHistoryWriter};

/// Errors that can occur when sending a message through the pipeline.
#[derive(Debug, thiserror::Error)]
pub enum SendError {
    /// Message validation failed (empty, too large, etc.).
    #[error("validation failed: {0}")]
    Validation(#[from] termchat_proto::message::ValidationError),

    /// Serialization or deserialization failed.
    #[error("codec error: {0}")]
    Codec(#[from] codec::CodecError),

    /// Encryption or decryption failed.
    #[error("crypto error: {0}")]
    Crypto(#[from] CryptoError),

    /// Transport-level send or receive failed.
    #[error("transport error: {0}")]
    Transport(#[from] TransportError),

    /// Receive-side validation failed.
    #[error("receive validation failed: {0}")]
    ReceiveValidation(String),

    /// Payload size exceeded maximum before decryption.
    #[error("payload too large: {size} bytes (max {max} bytes)")]
    OversizedPayload {
        /// Actual size in bytes.
        size: usize,
        /// Maximum allowed size.
        max: usize,
    },
}

/// Configuration for send retry and ack timeout behavior.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Number of times to retry a failed send before giving up.
    pub send_retries: u32,
    /// How long to wait for a delivery ack before timing out.
    pub ack_timeout: Duration,
    /// Number of times to retry after an ack timeout.
    pub ack_retries: u32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            send_retries: 1,
            ack_timeout: Duration::from_secs(10),
            ack_retries: 1,
        }
    }
}

/// Maximum allowed encrypted payload size before decryption (64 KB).
const MAX_ENCRYPTED_PAYLOAD_SIZE: usize = 64 * 1024;

/// Maximum size of the duplicate detection set.
const MAX_DUPLICATE_TRACKING: usize = 10_000;

/// Clock skew tolerance for timestamp validation (±5 minutes).
const CLOCK_SKEW_TOLERANCE_MS: u64 = 5 * 60 * 1000;

/// Events emitted by the [`ChatManager`] for UI notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatEvent {
    /// A message's delivery status changed.
    StatusChanged {
        /// The message whose status changed.
        message_id: MessageId,
        /// The new status.
        status: MessageStatus,
    },
    /// An incoming chat message was received from a peer.
    MessageReceived {
        /// The received message.
        message: ChatMessage,
        /// The peer that sent it.
        from: PeerId,
    },
    /// An incoming message has a timestamp with clock skew.
    MessageReceivedWithClockSkew {
        /// The received message.
        message: ChatMessage,
        /// The peer that sent it.
        from: PeerId,
        /// Description of the clock skew.
        skew_description: String,
    },
}

/// Manages the chat send/receive pipeline with status tracking and history.
///
/// The pipeline ensures that plaintext never leaves the application
/// boundary (Invariant 1). Status tracking monitors the lifecycle
/// of sent messages through Pending -> Sent -> Delivered states.
///
/// History persistence is optional: if a store is provided via
/// [`with_history`](Self::with_history), messages are saved after send
/// and statuses are updated on ack delivery.
pub struct ChatManager<C: CryptoSession, T: Transport, S: MessageStore> {
    /// The crypto session used for encrypting/decrypting messages.
    crypto: C,
    /// The transport used for sending/receiving encrypted payloads.
    transport: T,
    /// The local sender identity.
    sender_id: SenderId,
    /// The remote peer to communicate with.
    peer_id: PeerId,
    /// Status of each sent message, keyed by message ID.
    statuses: Mutex<HashMap<MessageId, MessageStatus>>,
    /// Channel for emitting chat events to the UI layer.
    event_tx: mpsc::Sender<ChatEvent>,
    /// Optional resilient history writer for local persistence.
    history: Option<ResilientHistoryWriter<S>>,
    /// Set of recently seen message IDs for duplicate detection (Invariant 3).
    seen_message_ids: Mutex<HashSet<MessageId>>,
    /// Queue of pending acks that failed to send and need retry.
    pending_acks: Mutex<Vec<(MessageId, PeerId)>>,
}

impl<C: CryptoSession, T: Transport, S: MessageStore> ChatManager<C, T, S> {
    /// Creates a new `ChatManager` without history persistence.
    ///
    /// Returns the manager and a receiver for [`ChatEvent`]s that the
    /// UI layer should consume.
    pub fn new(
        crypto: C,
        transport: T,
        sender_id: SenderId,
        peer_id: PeerId,
        event_buffer: usize,
    ) -> (Self, mpsc::Receiver<ChatEvent>) {
        let (event_tx, event_rx) = mpsc::channel(event_buffer);
        let manager = Self {
            crypto,
            transport,
            sender_id,
            peer_id,
            statuses: Mutex::new(HashMap::new()),
            event_tx,
            history: None,
            seen_message_ids: Mutex::new(HashSet::new()),
            pending_acks: Mutex::new(Vec::new()),
        };
        (manager, event_rx)
    }

    /// Creates a new `ChatManager` with history persistence.
    ///
    /// Messages are saved to the store after sending, and statuses
    /// are updated when delivery acks arrive. Write failures are
    /// handled gracefully by [`ResilientHistoryWriter`].
    ///
    /// Returns the manager, a chat event receiver, and a history
    /// warning receiver.
    pub fn with_history(
        crypto: C,
        transport: T,
        sender_id: SenderId,
        peer_id: PeerId,
        event_buffer: usize,
        store: S,
        warning_buffer: usize,
    ) -> (
        Self,
        mpsc::Receiver<ChatEvent>,
        mpsc::Receiver<history::HistoryWarning>,
    ) {
        let (event_tx, event_rx) = mpsc::channel(event_buffer);
        let (writer, warning_rx) = ResilientHistoryWriter::new(store, warning_buffer);
        let manager = Self {
            crypto,
            transport,
            sender_id,
            peer_id,
            statuses: Mutex::new(HashMap::new()),
            event_tx,
            history: Some(writer),
            seen_message_ids: Mutex::new(HashSet::new()),
            pending_acks: Mutex::new(Vec::new()),
        };
        (manager, event_rx, warning_rx)
    }

    /// Send a message through the full pipeline.
    ///
    /// Pipeline steps (MSS 2-6):
    /// 1. Build [`ChatMessage`] with metadata (ID, timestamp, sender, conversation)
    /// 2. Validate the message (non-empty, within size limit)
    /// 3. Serialize via [`codec::encode`]
    /// 4. Encrypt via [`CryptoSession::encrypt`]
    /// 5. Transmit via [`Transport::send`]
    /// 6. Save to history (if configured)
    ///
    /// The message status is tracked internally and updated when an ack
    /// arrives via [`receive_one`](Self::receive_one).
    ///
    /// # Errors
    ///
    /// Returns [`SendError`] if any pipeline step fails. History write
    /// failures do not cause a send error (handled resiliently).
    pub async fn send_message(
        &self,
        content: MessageContent,
        conversation: ConversationId,
    ) -> Result<(MessageId, MessageStatus), SendError> {
        // Step 1: Build the ChatMessage with metadata
        let message_id = MessageId::new();
        let message = ChatMessage {
            metadata: MessageMetadata {
                message_id: message_id.clone(),
                timestamp: Timestamp::now(),
                sender_id: self.sender_id.clone(),
                conversation_id: conversation,
            },
            content,
        };

        // Step 2: Validate
        message.validate()?;

        // Step 3: Serialize
        let envelope = Envelope::Chat(message.clone());
        let serialized = codec::encode(&envelope)?;

        // Step 4: Encrypt (Invariant 1: plaintext never leaves app boundary)
        let encrypted = self.crypto.encrypt(&serialized)?;

        // Step 5: Transmit
        self.transport.send(&self.peer_id, &encrypted).await?;

        // Track status
        let status = MessageStatus::Sent;
        self.statuses
            .lock()
            .await
            .insert(message_id.clone(), status.clone());

        // Step 6: Save to history (resilient -- never fails the send)
        if let Some(ref history) = self.history {
            history.save(&message, status.clone()).await;
        }

        // Notify UI
        let _ = self.event_tx.try_send(ChatEvent::StatusChanged {
            message_id: message_id.clone(),
            status: status.clone(),
        });

        Ok((message_id, status))
    }

    /// Send a message with transport-level retry on failure (Extension 6a).
    ///
    /// If the initial send fails, retries up to `config.send_retries` times
    /// on the same transport before returning an error.
    pub async fn send_message_with_retry(
        &self,
        content: MessageContent,
        conversation: ConversationId,
        config: &RetryConfig,
    ) -> Result<(MessageId, MessageStatus), SendError> {
        let mut last_err = None;

        for attempt in 0..=config.send_retries {
            match self
                .send_message(content.clone(), conversation.clone())
                .await
            {
                Ok(result) => return Ok(result),
                Err(SendError::Transport(e)) => {
                    tracing::debug!(
                        attempt,
                        max_retries = config.send_retries,
                        error = %e,
                        "send failed, will retry"
                    );
                    last_err = Some(SendError::Transport(e));
                }
                Err(e) => return Err(e), // Non-transport errors are not retryable
            }
        }

        Err(last_err.expect("loop ran at least once"))
    }

    /// Wait for a delivery ack for a specific message, with timeout (Extension 7a).
    ///
    /// Calls [`receive_one`](Self::receive_one) in a loop until either:
    /// - A matching ack arrives (returns `MessageStatus::Delivered`)
    /// - The timeout expires (returns `MessageStatus::Sent`)
    ///
    /// If the first attempt times out, retries up to `config.ack_retries` times.
    /// Non-ack envelopes received during the wait are still processed normally.
    pub async fn await_ack(&self, message_id: &MessageId, config: &RetryConfig) -> MessageStatus {
        for attempt in 0..=config.ack_retries {
            match tokio::time::timeout(config.ack_timeout, self.wait_for_ack(message_id)).await {
                Ok(Ok(())) => return MessageStatus::Delivered,
                Ok(Err(_)) => {
                    // Transport/decode error during receive -- treat as timeout
                    tracing::debug!(attempt, "error while waiting for ack, treating as timeout");
                }
                Err(_) => {
                    tracing::debug!(
                        attempt,
                        max_retries = config.ack_retries,
                        "ack timeout expired"
                    );
                }
            }
        }

        // All retries exhausted -- mark as Sent (not Delivered)
        tracing::info!(
            message_id = %message_id,
            "no ack received after retries, status remains Sent"
        );
        MessageStatus::Sent
    }

    /// Internal: keep receiving envelopes until we get an ack for the given message.
    async fn wait_for_ack(&self, target_id: &MessageId) -> Result<(), SendError> {
        loop {
            let envelope = self.receive_one().await?;
            if let Envelope::Ack(ack) = &envelope
                && ack.message_id == *target_id
            {
                return Ok(());
            }
            // Non-matching envelopes are already processed by receive_one
        }
    }

    /// Receive and process one incoming envelope from the transport.
    ///
    /// Handles the following cases:
    /// - **Chat message**: Validates, decrypts, deserializes, checks for duplicates,
    ///   stores in history, and automatically sends back a [`DeliveryAck`].
    ///   Emits a [`ChatEvent::MessageReceived`].
    /// - **Delivery ack**: Updates the tracked status from `Sent` to
    ///   `Delivered`. Updates history if configured. Emits a
    ///   [`ChatEvent::StatusChanged`].
    /// - **Nack**: Logs the negative acknowledgment (UC-002 Extension 5a).
    ///
    /// # Errors
    ///
    /// Returns [`SendError`] if transport receive, decryption, or
    /// deserialization fails. Validation failures on the receive side
    /// result in the message being dropped silently or a NACK being sent.
    pub async fn receive_one(&self) -> Result<Envelope, SendError> {
        // Extension 1a: Check payload size before decryption
        let (from, encrypted) = self.transport.recv().await?;
        if encrypted.len() > MAX_ENCRYPTED_PAYLOAD_SIZE {
            tracing::warn!(
                peer = %from,
                size = encrypted.len(),
                "dropping oversized payload from peer"
            );
            return Err(SendError::OversizedPayload {
                size: encrypted.len(),
                max: MAX_ENCRYPTED_PAYLOAD_SIZE,
            });
        }

        // Step 4: Decrypt (Extension 4a handled by crypto layer)
        let decrypted = self.crypto.decrypt(&encrypted)?;

        // Step 5: Deserialize (Extension 5a)
        let envelope = match codec::decode(&decrypted) {
            Ok(env) => env,
            Err(e) => {
                tracing::warn!(peer = %from, error = %e, "deserialization failed, sending NACK");
                // Send NACK for deserialization failure
                // We can't extract a message ID since deserialization failed, so use a dummy
                let nack = Nack {
                    message_id: MessageId::new(),
                    reason: NackReason::DeserializationFailed,
                };
                let _ = self.send_envelope(&Envelope::Nack(nack), &from).await;
                return Err(SendError::Codec(e));
            }
        };

        match &envelope {
            Envelope::Chat(msg) => {
                // Duplicate detection (Invariant 3)
                let msg_id = msg.metadata.message_id.clone();
                {
                    let mut seen = self.seen_message_ids.lock().await;
                    if seen.contains(&msg_id) {
                        tracing::debug!(message_id = %msg_id, "duplicate message dropped");
                        return Ok(envelope);
                    }
                    // Track this message ID
                    if seen.len() >= MAX_DUPLICATE_TRACKING {
                        // Simple eviction: clear half the set
                        seen.clear();
                    }
                    seen.insert(msg_id.clone());
                }

                // Step 6: Validate metadata
                // Extension 6c: Check sender ID matches transport peer
                if !self.sender_id_matches_peer(&msg.metadata.sender_id, &from) {
                    tracing::warn!(
                        peer = %from,
                        claimed_sender = %msg.metadata.sender_id,
                        "sender ID mismatch, rejecting message"
                    );
                    let nack = Nack {
                        message_id: msg_id.clone(),
                        reason: NackReason::SenderIdMismatch,
                    };
                    let _ = self.send_envelope(&Envelope::Nack(nack), &from).await;
                    return Err(SendError::ReceiveValidation(
                        "sender ID does not match authenticated peer".into(),
                    ));
                }

                // Extension 6a: Check timestamp for clock skew
                let has_clock_skew = self.check_timestamp_skew(&msg.metadata.timestamp);

                // Step 7: Store in history (Extension 7a handled by ResilientHistoryWriter)
                if let Some(ref history) = self.history {
                    history.save(msg, MessageStatus::Delivered).await;
                }

                // Step 8: Send delivery acknowledgment (Extension 8a: queue on failure)
                let ack = DeliveryAck {
                    message_id: msg_id.clone(),
                    timestamp: Timestamp::now(),
                };
                if let Err(e) = self.send_envelope(&Envelope::Ack(ack), &from).await {
                    tracing::warn!(
                        message_id = %msg_id,
                        error = %e,
                        "failed to send ack, queueing for retry"
                    );
                    self.pending_acks
                        .lock()
                        .await
                        .push((msg_id.clone(), from.clone()));
                }

                // Step 9: Emit event to UI
                if has_clock_skew {
                    let _ = self
                        .event_tx
                        .try_send(ChatEvent::MessageReceivedWithClockSkew {
                            message: msg.clone(),
                            from: from.clone(),
                            skew_description: "timestamp outside acceptable range".into(),
                        });
                } else {
                    let _ = self.event_tx.try_send(ChatEvent::MessageReceived {
                        message: msg.clone(),
                        from: from.clone(),
                    });
                }
            }
            Envelope::Ack(ack) => {
                // Update tracked status
                let mut statuses = self.statuses.lock().await;
                if let Some(status) = statuses.get_mut(&ack.message_id) {
                    *status = MessageStatus::Delivered;
                }
                drop(statuses);

                // Update history
                if let Some(ref history) = self.history {
                    history
                        .update_status(&ack.message_id, MessageStatus::Delivered)
                        .await;
                }

                // Notify UI of status change
                let _ = self.event_tx.try_send(ChatEvent::StatusChanged {
                    message_id: ack.message_id.clone(),
                    status: MessageStatus::Delivered,
                });
            }
            Envelope::Nack(nack) => {
                // Log the NACK (Extension 5a)
                tracing::warn!(
                    message_id = %nack.message_id,
                    reason = ?nack.reason,
                    "received NACK from peer"
                );
                // For now, just log. Future work: update message status to Failed.
            }
            Envelope::Handshake(_) => {
                // Handshake messages are handled by the crypto layer (UC-005).
            }
        }

        Ok(envelope)
    }

    /// Check if the sender ID matches the authenticated peer.
    ///
    /// For now, this is a simple comparison. In the real system with Noise,
    /// the peer ID would be derived from the Noise static public key and
    /// the sender ID would be the key fingerprint, so they should match.
    fn sender_id_matches_peer(&self, sender_id: &SenderId, peer: &PeerId) -> bool {
        // Placeholder: assume match for now. Real implementation would
        // compare the sender_id bytes with the peer's identity key fingerprint.
        // For testing with stub crypto, we skip validation.
        let _ = (sender_id, peer);
        true
    }

    /// Check if the timestamp is within acceptable clock skew tolerance.
    ///
    /// Returns `true` if the timestamp is outside the acceptable range.
    fn check_timestamp_skew(&self, timestamp: &Timestamp) -> bool {
        let now = Timestamp::now();
        let diff = if timestamp.as_millis() > now.as_millis() {
            timestamp.as_millis() - now.as_millis()
        } else {
            now.as_millis() - timestamp.as_millis()
        };
        diff > CLOCK_SKEW_TOLERANCE_MS
    }

    /// Get the current status of a sent message.
    pub async fn get_status(&self, message_id: &MessageId) -> Option<MessageStatus> {
        self.statuses.lock().await.get(message_id).cloned()
    }

    /// Returns a reference to the underlying transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Returns a reference to the underlying crypto session.
    pub fn crypto(&self) -> &C {
        &self.crypto
    }

    /// Returns a reference to the history writer, if configured.
    pub fn history(&self) -> Option<&ResilientHistoryWriter<S>> {
        self.history.as_ref()
    }

    /// Internal: encrypt, serialize, and send an envelope to a peer.
    async fn send_envelope(&self, envelope: &Envelope, peer: &PeerId) -> Result<(), SendError> {
        let serialized = codec::encode(envelope)?;
        let encrypted = self.crypto.encrypt(&serialized)?;
        self.transport.send(peer, &encrypted).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::noise::StubNoiseSession;
    use crate::transport::loopback::LoopbackTransport;
    use history::InMemoryStore;

    /// Creates an Alice ChatManager + Bob ChatManager connected via loopback
    /// (no history).
    fn setup_pair() -> (
        ChatManager<StubNoiseSession, LoopbackTransport, InMemoryStore>,
        mpsc::Receiver<ChatEvent>,
        ChatManager<StubNoiseSession, LoopbackTransport, InMemoryStore>,
        mpsc::Receiver<ChatEvent>,
    ) {
        let (transport_a, transport_b) =
            LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 32);

        let (alice, alice_events) = ChatManager::new(
            StubNoiseSession::new(true),
            transport_a,
            SenderId::new(vec![0xaa]),
            PeerId::new("bob"),
            32,
        );

        let (bob, bob_events) = ChatManager::new(
            StubNoiseSession::new(true),
            transport_b,
            SenderId::new(vec![0xbb]),
            PeerId::new("alice"),
            32,
        );

        (alice, alice_events, bob, bob_events)
    }

    /// Creates a single ChatManager (Alice) with a raw transport endpoint (Bob's),
    /// no history.
    fn setup_single() -> (
        ChatManager<StubNoiseSession, LoopbackTransport, InMemoryStore>,
        mpsc::Receiver<ChatEvent>,
        LoopbackTransport,
    ) {
        let (transport_a, transport_b) =
            LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 32);

        let (manager, events) = ChatManager::new(
            StubNoiseSession::new(true),
            transport_a,
            SenderId::new(vec![0xaa, 0xbb]),
            PeerId::new("bob"),
            32,
        );

        (manager, events, transport_b)
    }

    /// Creates an Alice+Bob pair where Alice has history enabled.
    fn setup_pair_with_history() -> (
        ChatManager<StubNoiseSession, LoopbackTransport, InMemoryStore>,
        mpsc::Receiver<ChatEvent>,
        mpsc::Receiver<history::HistoryWarning>,
        ChatManager<StubNoiseSession, LoopbackTransport, InMemoryStore>,
        mpsc::Receiver<ChatEvent>,
    ) {
        let (transport_a, transport_b) =
            LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 32);

        let (alice, alice_events, alice_warnings) = ChatManager::with_history(
            StubNoiseSession::new(true),
            transport_a,
            SenderId::new(vec![0xaa]),
            PeerId::new("bob"),
            32,
            InMemoryStore::new(),
            16,
        );

        let (bob, bob_events) = ChatManager::new(
            StubNoiseSession::new(true),
            transport_b,
            SenderId::new(vec![0xbb]),
            PeerId::new("alice"),
            32,
        );

        (alice, alice_events, alice_warnings, bob, bob_events)
    }

    #[tokio::test]
    async fn send_message_succeeds() {
        let (manager, _events, _bob_transport) = setup_single();
        let content = MessageContent::Text("hello, world!".into());
        let conversation = ConversationId::new();

        let result = manager.send_message(content, conversation).await;
        assert!(result.is_ok());

        let (_, status) = result.unwrap();
        assert_eq!(status, MessageStatus::Sent);
    }

    #[tokio::test]
    async fn send_message_tracks_status_as_sent() {
        let (manager, _events, _bob_transport) = setup_single();
        let conversation = ConversationId::new();

        let (msg_id, _) = manager
            .send_message(MessageContent::Text("hello".into()), conversation)
            .await
            .unwrap();

        let status = manager.get_status(&msg_id).await;
        assert_eq!(status, Some(MessageStatus::Sent));
    }

    #[tokio::test]
    async fn send_emits_status_changed_event() {
        let (manager, mut events, _bob_transport) = setup_single();
        let conversation = ConversationId::new();

        let (msg_id, _) = manager
            .send_message(MessageContent::Text("hello".into()), conversation)
            .await
            .unwrap();

        let event = events.try_recv().unwrap();
        assert_eq!(
            event,
            ChatEvent::StatusChanged {
                message_id: msg_id,
                status: MessageStatus::Sent,
            }
        );
    }

    #[tokio::test]
    async fn send_message_arrives_encrypted() {
        let (manager, _events, bob_transport) = setup_single();
        let content = MessageContent::Text("secret message".into());
        let conversation = ConversationId::new();

        manager.send_message(content, conversation).await.unwrap();

        let (_from, encrypted_bytes) = bob_transport.recv().await.unwrap();
        assert!(!encrypted_bytes.windows(14).any(|w| w == b"secret message"));
    }

    #[tokio::test]
    async fn send_and_decrypt_round_trip() {
        let (alice, _alice_events, bob, mut bob_events) = setup_pair();

        let content = MessageContent::Text("round trip test".into());
        let conversation = ConversationId::new();

        alice.send_message(content, conversation).await.unwrap();

        let envelope = bob.receive_one().await.unwrap();
        match envelope {
            Envelope::Chat(msg) => {
                let MessageContent::Text(ref text) = msg.content;
                assert_eq!(text, "round trip test");
            }
            _ => panic!("expected Chat envelope"),
        }

        let event = bob_events.try_recv().unwrap();
        match event {
            ChatEvent::MessageReceived { message, .. } => {
                let MessageContent::Text(ref text) = message.content;
                assert_eq!(text, "round trip test");
            }
            _ => panic!("expected MessageReceived event"),
        }
    }

    #[tokio::test]
    async fn delivery_ack_updates_status_to_delivered() {
        let (alice, mut alice_events, bob, _bob_events) = setup_pair();

        let conversation = ConversationId::new();

        let (msg_id, _) = alice
            .send_message(MessageContent::Text("ack me".into()), conversation)
            .await
            .unwrap();

        let _ = alice_events.try_recv().unwrap(); // Sent event

        assert_eq!(alice.get_status(&msg_id).await, Some(MessageStatus::Sent));

        bob.receive_one().await.unwrap(); // auto-acks
        alice.receive_one().await.unwrap(); // receives ack

        assert_eq!(
            alice.get_status(&msg_id).await,
            Some(MessageStatus::Delivered)
        );

        let event = alice_events.try_recv().unwrap();
        assert_eq!(
            event,
            ChatEvent::StatusChanged {
                message_id: msg_id,
                status: MessageStatus::Delivered,
            }
        );
    }

    #[tokio::test]
    async fn send_empty_message_fails_validation() {
        let (manager, _events, _bob_transport) = setup_single();
        let content = MessageContent::Text(String::new());
        let conversation = ConversationId::new();

        let result = manager.send_message(content, conversation).await;
        assert!(matches!(result, Err(SendError::Validation(_))));
    }

    #[tokio::test]
    async fn send_oversized_message_fails_validation() {
        let (manager, _events, _bob_transport) = setup_single();
        let big_text = "a".repeat(termchat_proto::message::MAX_MESSAGE_SIZE + 1);
        let content = MessageContent::Text(big_text);
        let conversation = ConversationId::new();

        let result = manager.send_message(content, conversation).await;
        assert!(matches!(result, Err(SendError::Validation(_))));
    }

    #[tokio::test]
    async fn send_with_no_crypto_session_fails() {
        let (transport_a, _transport_b) =
            LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 32);
        let (manager, _events): (
            ChatManager<StubNoiseSession, LoopbackTransport, InMemoryStore>,
            _,
        ) = ChatManager::new(
            StubNoiseSession::new(false),
            transport_a,
            SenderId::new(vec![0xaa]),
            PeerId::new("bob"),
            32,
        );

        let content = MessageContent::Text("hello".into());
        let conversation = ConversationId::new();

        let result = manager.send_message(content, conversation).await;
        assert!(matches!(result, Err(SendError::Crypto(_))));
    }

    #[tokio::test]
    async fn send_with_disconnected_transport_fails() {
        let (transport_a, transport_b) =
            LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 32);
        drop(transport_b);

        let (manager, _events): (
            ChatManager<StubNoiseSession, LoopbackTransport, InMemoryStore>,
            _,
        ) = ChatManager::new(
            StubNoiseSession::new(true),
            transport_a,
            SenderId::new(vec![0xaa]),
            PeerId::new("bob"),
            32,
        );

        let content = MessageContent::Text("hello".into());
        let conversation = ConversationId::new();

        let result = manager.send_message(content, conversation).await;
        assert!(matches!(result, Err(SendError::Transport(_))));
    }

    #[tokio::test]
    async fn send_returns_unique_message_ids() {
        let (manager, _events, _bob_transport) = setup_single();
        let conversation = ConversationId::new();

        let (id1, _) = manager
            .send_message(MessageContent::Text("msg1".into()), conversation.clone())
            .await
            .unwrap();
        let (id2, _) = manager
            .send_message(MessageContent::Text("msg2".into()), conversation)
            .await
            .unwrap();

        assert_ne!(id1, id2);
    }

    #[tokio::test]
    async fn messages_preserve_order() {
        let (alice, _alice_events, bob, _bob_events) = setup_pair();
        let conversation = ConversationId::new();

        for i in 0..5 {
            let content = MessageContent::Text(format!("message {i}"));
            alice
                .send_message(content, conversation.clone())
                .await
                .unwrap();
        }

        for i in 0..5 {
            let envelope = bob.receive_one().await.unwrap();
            match envelope {
                Envelope::Chat(msg) => {
                    let MessageContent::Text(ref text) = msg.content;
                    assert_eq!(text, &format!("message {i}"));
                }
                _ => panic!("expected Chat envelope at position {i}"),
            }
        }
    }

    #[tokio::test]
    async fn multiple_messages_all_get_acked() {
        let (alice, mut alice_events, bob, _bob_events) = setup_pair();
        let conversation = ConversationId::new();

        let mut msg_ids = Vec::new();
        for i in 0..3 {
            let (id, _) = alice
                .send_message(
                    MessageContent::Text(format!("msg {i}")),
                    conversation.clone(),
                )
                .await
                .unwrap();
            msg_ids.push(id);
        }

        // Drain Sent events
        for _ in 0..3 {
            let _ = alice_events.try_recv().unwrap();
        }

        // Bob receives all 3 (auto-acks)
        for _ in 0..3 {
            bob.receive_one().await.unwrap();
        }

        // Alice receives all 3 acks
        for _ in 0..3 {
            alice.receive_one().await.unwrap();
        }

        for id in &msg_ids {
            assert_eq!(alice.get_status(id).await, Some(MessageStatus::Delivered));
        }

        for _ in 0..3 {
            let event = alice_events.try_recv().unwrap();
            match event {
                ChatEvent::StatusChanged {
                    status: MessageStatus::Delivered,
                    ..
                } => {}
                _ => panic!("expected StatusChanged(Delivered) event"),
            }
        }
    }

    // --- History integration tests ---

    #[tokio::test]
    async fn send_saves_message_to_history() {
        let (alice, _events, _warnings, _bob, _bob_events) = setup_pair_with_history();
        let conversation = ConversationId::new();

        alice
            .send_message(
                MessageContent::Text("saved msg".into()),
                conversation.clone(),
            )
            .await
            .unwrap();

        let history = alice.history().unwrap();
        let messages = history.get_conversation(&conversation, 10).await.unwrap();
        assert_eq!(messages.len(), 1);
        let MessageContent::Text(ref text) = messages[0].0.content;
        assert_eq!(text, "saved msg");
        assert_eq!(messages[0].1, MessageStatus::Sent);
    }

    #[tokio::test]
    async fn ack_updates_history_status() {
        let (alice, _events, _warnings, bob, _bob_events) = setup_pair_with_history();
        let conversation = ConversationId::new();

        alice
            .send_message(
                MessageContent::Text("track me".into()),
                conversation.clone(),
            )
            .await
            .unwrap();

        // Bob receives (auto-acks)
        bob.receive_one().await.unwrap();
        // Alice receives the ack
        alice.receive_one().await.unwrap();

        let history = alice.history().unwrap();
        let messages = history.get_conversation(&conversation, 10).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].1, MessageStatus::Delivered);
    }

    #[tokio::test]
    async fn multiple_messages_saved_to_history() {
        let (alice, _events, _warnings, _bob, _bob_events) = setup_pair_with_history();
        let conversation = ConversationId::new();

        for i in 0..3 {
            alice
                .send_message(
                    MessageContent::Text(format!("history msg {i}")),
                    conversation.clone(),
                )
                .await
                .unwrap();
        }

        let history = alice.history().unwrap();
        let messages = history.get_conversation(&conversation, 10).await.unwrap();
        assert_eq!(messages.len(), 3);
    }

    // --- Retry and timeout tests ---

    #[tokio::test]
    async fn await_ack_returns_delivered_when_ack_received() {
        let (alice, _alice_events, bob, _bob_events) = setup_pair();
        let config = RetryConfig {
            ack_timeout: Duration::from_secs(5),
            ack_retries: 0,
            ..Default::default()
        };
        let conversation = ConversationId::new();

        let (msg_id, _) = alice
            .send_message(MessageContent::Text("ack test".into()), conversation)
            .await
            .unwrap();

        // Bob receives and auto-acks in a separate task
        let bob_task = tokio::spawn(async move {
            bob.receive_one().await.unwrap();
        });

        let status = alice.await_ack(&msg_id, &config).await;
        assert_eq!(status, MessageStatus::Delivered);

        bob_task.await.unwrap();
    }

    #[tokio::test]
    async fn await_ack_returns_sent_on_timeout() {
        let (alice, _alice_events, _bob, _bob_events) = setup_pair();
        let config = RetryConfig {
            ack_timeout: Duration::from_millis(50), // very short timeout
            ack_retries: 0,
            ..Default::default()
        };
        let conversation = ConversationId::new();

        let (msg_id, _) = alice
            .send_message(MessageContent::Text("no ack".into()), conversation)
            .await
            .unwrap();

        // Don't have bob receive, so no ack is sent
        let status = alice.await_ack(&msg_id, &config).await;
        assert_eq!(status, MessageStatus::Sent);
    }

    #[tokio::test]
    async fn await_ack_retries_on_timeout() {
        let (alice, _alice_events, _bob, _bob_events) = setup_pair();
        let config = RetryConfig {
            ack_timeout: Duration::from_millis(30),
            ack_retries: 2, // retry twice
            ..Default::default()
        };
        let conversation = ConversationId::new();

        let (msg_id, _) = alice
            .send_message(MessageContent::Text("retry ack".into()), conversation)
            .await
            .unwrap();

        let start = tokio::time::Instant::now();
        let status = alice.await_ack(&msg_id, &config).await;
        let elapsed = start.elapsed();

        assert_eq!(status, MessageStatus::Sent);
        // Should have waited at least 3 * 30ms (initial + 2 retries)
        assert!(elapsed >= Duration::from_millis(80));
    }

    #[tokio::test]
    async fn send_with_retry_succeeds_on_first_try() {
        let (alice, _alice_events, _bob, _bob_events) = setup_pair();
        let config = RetryConfig::default();
        let conversation = ConversationId::new();

        let result = alice
            .send_message_with_retry(
                MessageContent::Text("retry test".into()),
                conversation,
                &config,
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn send_with_retry_fails_after_retries_exhausted() {
        let (transport_a, transport_b) =
            LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 32);
        drop(transport_b); // disconnect

        let (manager, _events): (
            ChatManager<StubNoiseSession, LoopbackTransport, InMemoryStore>,
            _,
        ) = ChatManager::new(
            StubNoiseSession::new(true),
            transport_a,
            SenderId::new(vec![0xaa]),
            PeerId::new("bob"),
            32,
        );

        let config = RetryConfig {
            send_retries: 2,
            ..Default::default()
        };

        let result = manager
            .send_message_with_retry(
                MessageContent::Text("will fail".into()),
                ConversationId::new(),
                &config,
            )
            .await;
        assert!(matches!(result, Err(SendError::Transport(_))));
    }

    #[tokio::test]
    async fn send_with_retry_does_not_retry_validation_errors() {
        let (alice, _alice_events, _bob, _bob_events) = setup_pair();
        let config = RetryConfig {
            send_retries: 3,
            ..Default::default()
        };

        // Empty message -- should fail immediately without retries
        let result = alice
            .send_message_with_retry(
                MessageContent::Text(String::new()),
                ConversationId::new(),
                &config,
            )
            .await;
        assert!(matches!(result, Err(SendError::Validation(_))));
    }

    #[tokio::test]
    async fn retry_config_default_values() {
        let config = RetryConfig::default();
        assert_eq!(config.send_retries, 1);
        assert_eq!(config.ack_timeout, Duration::from_secs(10));
        assert_eq!(config.ack_retries, 1);
    }
}
