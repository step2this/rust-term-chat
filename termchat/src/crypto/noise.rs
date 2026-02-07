//! Stubbed Noise session for UC-001 pipeline testing.
//!
//! This module provides [`StubNoiseSession`], a placeholder implementation
//! of [`CryptoSession`] that uses repeating-key XOR for "encryption". This
//! is **not cryptographically secure** — it exists solely so the send/receive
//! pipeline can be tested end-to-end before the real Noise XX handshake is
//! implemented in UC-005.
//!
//! # TODO: Replace with real Noise XX in UC-005
//!
//! The real implementation will use:
//! - `noise-protocol` crate with XX handshake pattern
//! - `x25519-dalek` for key exchange
//! - ChaCha20-Poly1305 for AEAD
//! - Per-session ephemeral keys with perfect forward secrecy

use super::{CryptoError, CryptoSession};

/// A fixed 32-byte key used by the stub for repeating-key XOR.
///
/// # Safety
///
/// This provides **zero** cryptographic security. It only ensures that
/// `encrypt(data) != data` for testing the invariant that plaintext
/// never appears on the wire.
// TODO: Replace with real Noise XX in UC-005
const STUB_KEY: [u8; 32] = [
    0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF,
    0xFE, 0xDC, 0xBA, 0x98, 0x76, 0x54, 0x32, 0x10, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42,
];

/// Stubbed Noise session using repeating-key XOR.
///
/// This is a **placeholder** — the real Noise XX implementation comes
/// in UC-005. It satisfies the [`CryptoSession`] contract just enough
/// to validate the send/receive pipeline:
///
/// - `encrypt(plaintext) != plaintext` (invariant: no plaintext on wire)
/// - `decrypt(encrypt(plaintext)) == plaintext` (round-trip correctness)
///
/// The `established` field controls whether the session is "active".
/// When `false`, all operations return [`CryptoError::NoSession`].
// TODO: Replace with real Noise XX in UC-005
pub struct StubNoiseSession {
    /// Whether this session is considered "established" (handshake complete).
    established: bool,
}

impl StubNoiseSession {
    /// Create a new stub session.
    ///
    /// If `established` is `true`, the session is immediately ready for
    /// encrypt/decrypt. If `false`, all operations will return
    /// [`CryptoError::NoSession`] until a handshake is performed.
    // TODO: Replace with real Noise XX in UC-005
    pub fn new(established: bool) -> Self {
        Self { established }
    }
}

/// XOR each byte of `data` with the corresponding byte of [`STUB_KEY`],
/// cycling the key if `data` is longer than 32 bytes.
// TODO: Replace with real Noise XX in UC-005
fn xor_with_key(data: &[u8]) -> Vec<u8> {
    data.iter()
        .enumerate()
        .map(|(i, byte)| byte ^ STUB_KEY[i % STUB_KEY.len()])
        .collect()
}

impl CryptoSession for StubNoiseSession {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if !self.established {
            return Err(CryptoError::NoSession);
        }
        Ok(xor_with_key(plaintext))
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if !self.established {
            return Err(CryptoError::NoSession);
        }
        // XOR is its own inverse, so decrypt == encrypt.
        Ok(xor_with_key(ciphertext))
    }

    fn is_established(&self) -> bool {
        self.established
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_round_trip() {
        let session = StubNoiseSession::new(true);
        let plaintext = b"hello, world! This is a test message.";

        let ciphertext = session.encrypt(plaintext).unwrap();
        let decrypted = session.decrypt(&ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypted_differs_from_plaintext() {
        let session = StubNoiseSession::new(true);
        let plaintext = b"secret message";

        let ciphertext = session.encrypt(plaintext).unwrap();
        assert_ne!(ciphertext, plaintext.to_vec());
    }

    #[test]
    fn encrypt_preserves_length() {
        let session = StubNoiseSession::new(true);
        let plaintext = b"some data of known length";

        let ciphertext = session.encrypt(plaintext).unwrap();
        assert_eq!(ciphertext.len(), plaintext.len());
    }

    #[test]
    fn empty_plaintext_round_trips() {
        let session = StubNoiseSession::new(true);
        let plaintext = b"";

        let ciphertext = session.encrypt(plaintext).unwrap();
        let decrypted = session.decrypt(&ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn large_payload_round_trips() {
        let session = StubNoiseSession::new(true);
        // Payload larger than the 32-byte key to test cycling.
        let plaintext: Vec<u8> = (0u8..=255).cycle().take(1024).collect();

        let ciphertext = session.encrypt(&plaintext).unwrap();
        let decrypted = session.decrypt(&ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
        assert_ne!(ciphertext, plaintext);
    }

    #[test]
    fn no_session_encrypt_returns_error() {
        let session = StubNoiseSession::new(false);

        let result = session.encrypt(b"hello");
        assert!(matches!(result, Err(CryptoError::NoSession)));
    }

    #[test]
    fn no_session_decrypt_returns_error() {
        let session = StubNoiseSession::new(false);

        let result = session.decrypt(b"hello");
        assert!(matches!(result, Err(CryptoError::NoSession)));
    }

    #[test]
    fn is_established_reflects_state() {
        let active = StubNoiseSession::new(true);
        assert!(active.is_established());

        let inactive = StubNoiseSession::new(false);
        assert!(!inactive.is_established());
    }
}

// ============================================================================
// Real Noise XX Implementation (UC-005)
// ============================================================================

use super::keys::Identity;
use std::sync::Mutex;

/// State of the Noise XX handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeState {
    /// Handshake not started.
    Idle,
    /// Initiator sent message 1, waiting for message 2.
    WaitingForMessage2,
    /// Responder sent message 2, waiting for message 3.
    WaitingForMessage3,
    /// Handshake complete, session ready.
    Complete,
    /// Handshake failed.
    Failed(String),
}

/// Manages the 3-message Noise XX handshake.
pub struct NoiseHandshake {
    handshake: snow::HandshakeState,
    is_initiator: bool,
    state: HandshakeState,
}

impl NoiseHandshake {
    /// Create a new handshake as the initiator.
    pub fn new_initiator(identity: &Identity) -> Result<Self, CryptoError> {
        let params: snow::params::NoiseParams = "Noise_XX_25519_ChaChaPoly_BLAKE2s"
            .parse()
            .map_err(|e: snow::Error| CryptoError::HandshakeFailed(e.to_string()))?;

        let builder = snow::Builder::new(params).local_private_key(identity.private_key());

        let handshake = builder
            .build_initiator()
            .map_err(|e| CryptoError::HandshakeFailed(e.to_string()))?;

        Ok(Self {
            handshake,
            is_initiator: true,
            state: HandshakeState::Idle,
        })
    }

    /// Create a new handshake as the responder.
    pub fn new_responder(identity: &Identity) -> Result<Self, CryptoError> {
        let params: snow::params::NoiseParams = "Noise_XX_25519_ChaChaPoly_BLAKE2s"
            .parse()
            .map_err(|e: snow::Error| CryptoError::HandshakeFailed(e.to_string()))?;

        let builder = snow::Builder::new(params).local_private_key(identity.private_key());

        let handshake = builder
            .build_responder()
            .map_err(|e| CryptoError::HandshakeFailed(e.to_string()))?;

        Ok(Self {
            handshake,
            is_initiator: false,
            state: HandshakeState::Idle,
        })
    }

    /// Write the next handshake message.
    pub fn write_message(&mut self, payload: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if matches!(
            self.state,
            HandshakeState::Complete | HandshakeState::Failed(_)
        ) {
            return Err(CryptoError::HandshakeStateError(
                "handshake already complete or failed".to_string(),
            ));
        }

        let mut buf = vec![0u8; 65535];
        let len = self
            .handshake
            .write_message(payload, &mut buf)
            .map_err(|e| {
                self.state = HandshakeState::Failed(e.to_string());
                CryptoError::HandshakeFailed(e.to_string())
            })?;
        buf.truncate(len);

        if self.handshake.is_handshake_finished() {
            self.state = HandshakeState::Complete;
        } else {
            // Update state based on role
            self.state = if self.is_initiator {
                HandshakeState::WaitingForMessage2
            } else {
                HandshakeState::WaitingForMessage3
            };
        }

        Ok(buf)
    }

    /// Read and process a received handshake message.
    pub fn read_message(&mut self, message: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if matches!(
            self.state,
            HandshakeState::Complete | HandshakeState::Failed(_)
        ) {
            return Err(CryptoError::HandshakeStateError(
                "handshake already complete or failed".to_string(),
            ));
        }

        let mut buf = vec![0u8; 65535];
        let len = self
            .handshake
            .read_message(message, &mut buf)
            .map_err(|e| {
                self.state = HandshakeState::Failed(e.to_string());
                CryptoError::HandshakeFailed(e.to_string())
            })?;
        buf.truncate(len);

        if self.handshake.is_handshake_finished() {
            self.state = HandshakeState::Complete;
        }

        Ok(buf)
    }

    /// Check if the handshake is complete.
    pub fn is_complete(&self) -> bool {
        self.state == HandshakeState::Complete
    }

    /// Get the current handshake state.
    pub fn state(&self) -> &HandshakeState {
        &self.state
    }

    /// Get the remote peer's static public key (available after message 2 for initiator, message 3 for responder).
    pub fn remote_public_key(&self) -> Option<Vec<u8>> {
        self.handshake.get_remote_static().map(|k| k.to_vec())
    }

    /// Transition to transport mode after handshake completion.
    pub fn into_transport(self) -> Result<NoiseXXSession, CryptoError> {
        if self.state != HandshakeState::Complete {
            return Err(CryptoError::HandshakeStateError(
                "handshake not complete".to_string(),
            ));
        }

        let transport = self
            .handshake
            .into_transport_mode()
            .map_err(|e| CryptoError::HandshakeFailed(e.to_string()))?;

        Ok(NoiseXXSession {
            transport: Mutex::new(transport),
        })
    }
}

/// A Noise XX session implementing the CryptoSession trait.
///
/// Created from a completed NoiseHandshake via `into_transport()`.
/// Uses ChaCha20-Poly1305 AEAD for encryption/decryption.
pub struct NoiseXXSession {
    transport: Mutex<snow::TransportState>,
}

impl NoiseXXSession {
    /// Get the remote peer's static public key.
    pub fn remote_public_key(&self) -> Vec<u8> {
        self.transport
            .lock()
            .unwrap()
            .get_remote_static()
            .map(|k| k.to_vec())
            .unwrap_or_default()
    }
}

impl CryptoSession for NoiseXXSession {
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let mut buf = vec![0u8; plaintext.len() + 16]; // 16 bytes for AEAD tag
        let mut transport = self
            .transport
            .lock()
            .map_err(|e| CryptoError::EncryptionFailed(format!("lock poisoned: {e}")))?;
        let len = transport
            .write_message(plaintext, &mut buf)
            .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;
        buf.truncate(len);
        Ok(buf)
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let mut buf = vec![0u8; ciphertext.len()];
        let mut transport = self
            .transport
            .lock()
            .map_err(|e| CryptoError::DecryptionFailed(format!("lock poisoned: {e}")))?;
        let len = transport
            .read_message(ciphertext, &mut buf)
            .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;
        buf.truncate(len);
        Ok(buf)
    }

    fn is_established(&self) -> bool {
        true
    }
}
