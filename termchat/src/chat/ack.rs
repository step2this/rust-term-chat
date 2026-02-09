//! Acknowledgment tracking and retry logic for [`ChatManager`].
//!
//! Contains [`RetryConfig`] for configuring send retry and ack timeout
//! behavior, plus the `await_ack` / `wait_for_ack` methods that block
//! until a delivery acknowledgment is received or a timeout expires.

use std::time::Duration;

use termchat_proto::message::{Envelope, MessageId, MessageStatus};

use crate::crypto::CryptoSession;
use crate::transport::Transport;

use super::history::MessageStore;
use super::{ChatManager, SendError};

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

impl<C: CryptoSession, T: Transport, S: MessageStore> ChatManager<C, T, S> {
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
}
