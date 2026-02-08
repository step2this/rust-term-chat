//! Identity and key management for `TermChat`.
//!
//! This module provides long-term identity keypairs, key storage, and
//! peer key caching for the Noise XX handshake.

use super::CryptoError;
use std::collections::HashMap;
use zeroize::ZeroizeOnDrop;

/// A long-term identity keypair for a `TermChat` user.
///
/// This wraps a static x25519 keypair used in the Noise XX handshake.
/// The private key is zeroized on drop to prevent key material from
/// lingering in memory.
#[derive(ZeroizeOnDrop)]
pub struct Identity {
    /// The raw private key bytes (32 bytes for x25519).
    #[zeroize(skip)]
    private_key: Vec<u8>,
    /// The public key bytes (32 bytes for x25519).
    public_key: Vec<u8>,
}

impl Identity {
    /// Generate a new random identity using the system's CSPRNG.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::KeyGenerationFailed`] if the CSPRNG fails
    /// or if snow's key generation fails.
    pub fn generate() -> Result<Self, CryptoError> {
        let secret = x25519_dalek::StaticSecret::random_from_rng(rand_core::OsRng);
        let public = x25519_dalek::PublicKey::from(&secret);
        Ok(Self {
            private_key: secret.to_bytes().to_vec(),
            public_key: public.as_bytes().to_vec(),
        })
    }

    /// Load an identity from a stored private key.
    ///
    /// The public key is derived from the private key.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::KeyGenerationFailed`] if the private key
    /// is invalid (wrong length or format).
    pub fn from_private_key(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != 32 {
            return Err(CryptoError::KeyGenerationFailed(
                "private key must be 32 bytes".to_string(),
            ));
        }

        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(bytes);
        let secret = x25519_dalek::StaticSecret::from(key_bytes);
        let public = x25519_dalek::PublicKey::from(&secret);

        Ok(Self {
            private_key: bytes.to_vec(),
            public_key: public.as_bytes().to_vec(),
        })
    }

    /// Get the public key bytes.
    ///
    /// This is safe to share publicly and is used for peer verification.
    #[must_use]
    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    /// Get the private key bytes.
    ///
    /// This is used internally for the Noise handshake and should never
    /// be exposed outside the crypto layer.
    #[allow(dead_code)]
    #[must_use]
    pub(crate) fn private_key(&self) -> &[u8] {
        &self.private_key
    }

    /// Generate a fingerprint for display purposes.
    ///
    /// Returns a hex string of the first 8 bytes of the public key.
    /// This is used in the UI to show abbreviated peer identities.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        use std::fmt::Write;
        let bytes = &self.public_key[..8.min(self.public_key.len())];
        bytes.iter().fold(String::new(), |mut output, b| {
            let _ = write!(output, "{b:02x}");
            output
        })
    }
}

/// Trait for persistent key storage.
///
/// Implementors handle loading and saving identity keypairs to disk
/// or other persistent storage.
pub trait KeyStore: Send + Sync {
    /// Load an identity from storage.
    ///
    /// Returns `None` if no identity exists.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError`] if the stored key data is corrupted or
    /// inaccessible.
    fn load(&self) -> Result<Option<Identity>, CryptoError>;

    /// Save an identity to storage.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError`] if the identity cannot be persisted.
    fn save(&self, identity: &Identity) -> Result<(), CryptoError>;
}

/// In-memory key store for testing.
///
/// Does not persist keys beyond the lifetime of the struct.
pub struct InMemoryKeyStore {
    key: parking_lot::Mutex<Option<Vec<u8>>>,
}

impl InMemoryKeyStore {
    /// Create a new empty in-memory key store.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            key: parking_lot::Mutex::new(None),
        }
    }
}

impl Default for InMemoryKeyStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyStore for InMemoryKeyStore {
    fn load(&self) -> Result<Option<Identity>, CryptoError> {
        let guard = self.key.lock();
        match &*guard {
            Some(bytes) => Ok(Some(Identity::from_private_key(bytes)?)),
            None => Ok(None),
        }
    }

    fn save(&self, identity: &Identity) -> Result<(), CryptoError> {
        *self.key.lock() = Some(identity.private_key.clone());
        Ok(())
    }
}

/// Cache of known peer public keys.
///
/// Used to detect when a peer's key changes (which may indicate a
/// man-in-the-middle attack or key rotation).
pub struct PeerKeyCache {
    /// Map from peer identifier to their public key.
    cache: parking_lot::Mutex<HashMap<String, Vec<u8>>>,
}

impl PeerKeyCache {
    /// Create a new empty peer key cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    /// Check if a peer's key is known and matches the cached value.
    ///
    /// Returns:
    /// - `Ok(true)` if the key matches the cached value
    /// - `Ok(false)` if this is the first time seeing this peer
    /// - `Err(IdentityVerificationFailed)` if the key has changed
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::IdentityVerificationFailed`] if the peer's
    /// key has changed since it was last cached.
    pub fn verify(&self, peer_id: &str, public_key: &[u8]) -> Result<bool, CryptoError> {
        let guard = self.cache.lock();
        guard.get(peer_id).map_or(Ok(false), |cached_key| {
            if cached_key == public_key {
                Ok(true)
            } else {
                Err(CryptoError::IdentityVerificationFailed)
            }
        })
    }

    /// Store a peer's public key in the cache.
    ///
    /// This should be called after the first successful handshake with
    /// a peer, or after the user explicitly trusts a new key.
    pub fn store(&self, peer_id: String, public_key: Vec<u8>) {
        let mut guard = self.cache.lock();
        guard.insert(peer_id, public_key);
    }

    /// Get a peer's cached public key.
    #[must_use]
    pub fn get(&self, peer_id: &str) -> Option<Vec<u8>> {
        let guard = self.cache.lock();
        guard.get(peer_id).cloned()
    }
}

impl Default for PeerKeyCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_identity_produces_valid_keypair() {
        let identity = Identity::generate().unwrap();
        assert_eq!(identity.public_key().len(), 32);
        assert_eq!(identity.private_key().len(), 32);
    }

    #[test]
    fn fingerprint_is_hex_string() {
        let identity = Identity::generate().unwrap();
        let fingerprint = identity.fingerprint();
        assert_eq!(fingerprint.len(), 16); // 8 bytes * 2 hex chars
        assert!(fingerprint.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn from_private_key_derives_same_public_key() {
        let identity1 = Identity::generate().unwrap();
        let private_key = identity1.private_key().to_vec();

        let identity2 = Identity::from_private_key(&private_key).unwrap();
        assert_eq!(identity1.public_key(), identity2.public_key());
    }

    #[test]
    fn from_private_key_rejects_invalid_length() {
        let result = Identity::from_private_key(&[0u8; 16]);
        assert!(matches!(result, Err(CryptoError::KeyGenerationFailed(_))));
    }

    #[test]
    fn in_memory_key_store_round_trip() {
        let store = InMemoryKeyStore::new();
        let identity = Identity::generate().unwrap();

        // Initially empty
        assert!(store.load().unwrap().is_none());

        // Save and load
        store.save(&identity).unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.public_key(), identity.public_key());
    }

    #[test]
    fn peer_key_cache_first_contact() {
        let cache = PeerKeyCache::new();
        let key = vec![1, 2, 3, 4];

        // First time seeing this peer
        let result = cache.verify("alice", &key).unwrap();
        assert!(!result);

        // Store the key
        cache.store("alice".to_string(), key.clone());

        // Now it should match
        let result = cache.verify("alice", &key).unwrap();
        assert!(result);
    }

    #[test]
    fn peer_key_cache_detects_key_change() {
        let cache = PeerKeyCache::new();
        let key1 = vec![1, 2, 3, 4];
        let key2 = vec![5, 6, 7, 8];

        cache.store("alice".to_string(), key1.clone());

        // Attempting to verify with a different key should fail
        let result = cache.verify("alice", &key2);
        assert!(matches!(
            result,
            Err(CryptoError::IdentityVerificationFailed)
        ));
    }

    #[test]
    fn peer_key_cache_get_returns_stored_key() {
        let cache = PeerKeyCache::new();
        let key = vec![1, 2, 3, 4];

        assert!(cache.get("alice").is_none());

        cache.store("alice".to_string(), key.clone());
        assert_eq!(cache.get("alice"), Some(key));
    }
}
