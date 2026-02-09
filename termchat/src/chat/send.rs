//! Send pipeline methods for [`ChatManager`].
//!
//! Contains the main send pipeline, retry logic, and fire-and-forget
//! message types (presence updates, typing indicators).

use termchat_proto::codec;
use termchat_proto::message::{
    ChatMessage, ConversationId, Envelope, MessageContent, MessageId, MessageMetadata,
    MessageStatus, Timestamp,
};

use crate::crypto::CryptoSession;
use crate::transport::Transport;

use super::history::MessageStore;
use super::{ChatEvent, ChatManager, RetryConfig, SendError};

impl<C: CryptoSession, T: Transport, S: MessageStore> ChatManager<C, T, S> {
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
    ///
    /// # Errors
    ///
    /// Returns [`SendError`] if all retry attempts fail.
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

        Err(last_err.unwrap_or_else(|| unreachable!("loop ran at least once")))
    }

    /// Send a presence update to the connected peer.
    ///
    /// Presence messages are fire-and-forget: no ack is expected, and send
    /// failures are logged but do not propagate errors.
    pub async fn send_presence(&self, presence: &termchat_proto::presence::PresenceMessage) {
        let Ok(data) = postcard::to_allocvec(presence) else {
            tracing::warn!("failed to serialize presence message");
            return;
        };
        let envelope = Envelope::PresenceUpdate(data);
        if let Err(e) = self.send_envelope(&envelope, &self.peer_id).await {
            tracing::debug!(error = %e, "failed to send presence update (fire-and-forget)");
        }
    }

    /// Send a typing indicator to the connected peer.
    ///
    /// Typing indicators are fire-and-forget: no ack is expected, and send
    /// failures are logged but do not propagate errors.
    pub async fn send_typing(&self, typing: &termchat_proto::typing::TypingMessage) {
        let Ok(data) = postcard::to_allocvec(typing) else {
            tracing::warn!("failed to serialize typing message");
            return;
        };
        let envelope = Envelope::TypingIndicator(data);
        if let Err(e) = self.send_envelope(&envelope, &self.peer_id).await {
            tracing::debug!(error = %e, "failed to send typing indicator (fire-and-forget)");
        }
    }
}
