//! Cryptographic session layer for TermChat.
//!
//! Defines the [`CryptoSession`] trait for encrypt/decrypt operations and
//! error types. The trait is the **only** boundary where plaintext exists —
//! all data entering [`CryptoSession::encrypt`] is plaintext and all data
//! leaving it is ciphertext. No other layer should handle plaintext.
//!
//! # Current status
//!
//! A stubbed implementation ([`noise::StubNoiseSession`]) is provided for
//! UC-001 pipeline testing. The real Noise XX handshake implementation will
//! replace it in UC-005.

pub mod noise;

/// Errors that can occur during cryptographic operations.
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    /// No Noise session has been established with the peer.
    ///
    /// The caller should initiate a handshake (UC-005) before retrying.
    #[error("no crypto session established — handshake required")]
    NoSession,

    /// Encryption failed.
    #[error("encryption failed: {0}")]
    EncryptionFailed(String),

    /// Decryption failed (corrupted ciphertext, wrong key, or tampered data).
    #[error("decryption failed: {0}")]
    DecryptionFailed(String),
}

/// Trait for encrypting and decrypting message payloads.
///
/// # Invariant
///
/// The `encrypt`/`decrypt` boundary is the **only** place where plaintext
/// is handled. All data passed to [`Transport::send`](super::transport::Transport::send)
/// must have already passed through `encrypt`. All data received from
/// [`Transport::recv`](super::transport::Transport::recv) must pass through
/// `decrypt` before being interpreted.
///
/// # Implementors
///
/// - [`noise::StubNoiseSession`] — placeholder using XOR (UC-001)
/// - Real Noise XX session — coming in UC-005
pub trait CryptoSession: Send + Sync {
    /// Encrypt a plaintext payload, returning ciphertext bytes.
    ///
    /// The returned bytes must be safe to transmit over the network.
    /// They must differ from the input plaintext.
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError>;

    /// Decrypt a ciphertext payload, recovering the original plaintext.
    ///
    /// Returns an error if the ciphertext is corrupted, tampered with,
    /// or was encrypted with a different key.
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError>;

    /// Returns `true` if this session has completed the handshake and
    /// is ready to encrypt/decrypt.
    fn is_established(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::noise::StubNoiseSession;
    use super::*;

    /// Helper: simulate the pipeline's pre-encrypt check.
    /// The send pipeline should call `is_established()` or attempt
    /// `encrypt()` and handle `CryptoError::NoSession`.
    fn try_encrypt_via_trait(
        session: &dyn CryptoSession,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        session.encrypt(plaintext)
    }

    #[test]
    fn no_session_encrypt_returns_no_session_error() {
        let session = StubNoiseSession::new(false);
        let result = try_encrypt_via_trait(&session, b"hello");
        assert!(matches!(result, Err(CryptoError::NoSession)));
    }

    #[test]
    fn no_session_decrypt_returns_no_session_error() {
        let session = StubNoiseSession::new(false);
        let result = session.decrypt(b"ciphertext");
        assert!(matches!(result, Err(CryptoError::NoSession)));
    }

    #[test]
    fn no_session_is_established_returns_false() {
        let session = StubNoiseSession::new(false);
        assert!(!session.is_established());
    }

    #[test]
    fn established_session_encrypts_via_trait_object() {
        let session = StubNoiseSession::new(true);
        let plaintext = b"test message";
        let ciphertext = try_encrypt_via_trait(&session, plaintext).unwrap();
        assert_ne!(ciphertext, plaintext.to_vec());

        let decrypted = session.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn error_display_messages_are_descriptive() {
        let no_session = CryptoError::NoSession;
        assert!(no_session.to_string().contains("handshake"));

        let enc_fail = CryptoError::EncryptionFailed("bad key".to_string());
        assert!(enc_fail.to_string().contains("bad key"));

        let dec_fail = CryptoError::DecryptionFailed("tampered".to_string());
        assert!(dec_fail.to_string().contains("tampered"));
    }
}
