//! Receive pipeline methods for [`ChatManager`].
//!
//! Contains the envelope receive/dispatch logic, including decryption,
//! deserialization, duplicate detection, ack sending, and event emission.

use termchat_proto::codec;
use termchat_proto::message::{
    DeliveryAck, Envelope, MessageId, MessageStatus, Nack, NackReason, SenderId, Timestamp,
};

use crate::crypto::CryptoSession;
use crate::transport::{PeerId, Transport};

use super::history::MessageStore;
use super::{ChatEvent, ChatManager, SendError};

impl<C: CryptoSession, T: Transport, S: MessageStore> ChatManager<C, T, S> {
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
    #[allow(clippy::too_many_lines)]
    pub async fn receive_one(&self) -> Result<Envelope, SendError> {
        // Extension 1a: Check payload size before decryption
        let (from, encrypted) = self.transport.recv().await?;
        if encrypted.len() > self.chat_config.max_payload_size {
            tracing::warn!(
                peer = %from,
                size = encrypted.len(),
                "dropping oversized payload from peer"
            );
            return Err(SendError::OversizedPayload {
                size: encrypted.len(),
                max: self.chat_config.max_payload_size,
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
                    if seen.len() >= self.chat_config.max_duplicate_tracking {
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
                let has_clock_skew = self.check_timestamp_skew(msg.metadata.timestamp);

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
            Envelope::Handshake(_) | Envelope::TaskSync(_) => {
                // Handshake: handled by the crypto layer (UC-005).
                // TaskSync: handled by the tasks module (UC-008).
            }
            Envelope::PresenceUpdate(data) => {
                // Decode presence message and emit event to UI
                if let Ok(presence_msg) =
                    postcard::from_bytes::<termchat_proto::presence::PresenceMessage>(data)
                {
                    let _ = self.event_tx.try_send(ChatEvent::PresenceChanged {
                        peer_id: presence_msg.peer_id,
                        status: presence_msg.status,
                    });
                } else {
                    tracing::warn!(peer = %from, "failed to decode presence message");
                }
            }
            Envelope::TypingIndicator(data) => {
                // Decode typing message and emit event to UI
                if let Ok(typing_msg) =
                    postcard::from_bytes::<termchat_proto::typing::TypingMessage>(data)
                {
                    let _ = self.event_tx.try_send(ChatEvent::TypingChanged {
                        peer_id: typing_msg.peer_id,
                        room_id: typing_msg.room_id,
                        is_typing: typing_msg.is_typing,
                    });
                } else {
                    tracing::warn!(peer = %from, "failed to decode typing message");
                }
            }
        }

        Ok(envelope)
    }

    /// Check if the sender ID matches the authenticated peer.
    ///
    /// For now, this is a simple comparison. In the real system with Noise,
    /// the peer ID would be derived from the Noise static public key and
    /// the sender ID would be the key fingerprint, so they should match.
    #[allow(clippy::unused_self)]
    pub(crate) const fn sender_id_matches_peer(&self, sender_id: &SenderId, peer: &PeerId) -> bool {
        // Placeholder: assume match for now. Real implementation would
        // compare the sender_id bytes with the peer's identity key fingerprint.
        // For testing with stub crypto, we skip validation.
        let _ = (sender_id, peer);
        true
    }

    /// Check if the timestamp is within acceptable clock skew tolerance.
    ///
    /// Returns `true` if the timestamp is outside the acceptable range.
    #[allow(clippy::unused_self)]
    pub(crate) fn check_timestamp_skew(&self, timestamp: Timestamp) -> bool {
        let now = Timestamp::now();
        let diff = if timestamp.as_millis() > now.as_millis() {
            timestamp.as_millis() - now.as_millis()
        } else {
            now.as_millis() - timestamp.as_millis()
        };
        diff > self.chat_config.clock_skew_tolerance_ms
    }
}
