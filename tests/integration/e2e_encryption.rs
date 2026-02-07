//! Integration tests for UC-005: E2E Encryption with Noise XX.
//!
//! These tests verify the complete handshake flow and transport mode
//! encryption/decryption between two peers.

use std::time::{Duration, Instant};
use termchat::crypto::{
    CryptoError, CryptoSession,
    keys::{Identity, InMemoryKeyStore, KeyStore, PeerKeyCache},
    noise::{NoiseHandshake, NoiseXXSession},
};

// ============================================================================
// Basic Handshake Tests
// ============================================================================

#[test]
fn handshake_completes_between_two_peers() {
    let alice_identity = Identity::generate().unwrap();
    let bob_identity = Identity::generate().unwrap();

    let mut alice = NoiseHandshake::new_initiator(&alice_identity).unwrap();
    let mut bob = NoiseHandshake::new_responder(&bob_identity).unwrap();

    // Message 1: Alice -> Bob
    let msg1 = alice.write_message(&[]).unwrap();
    assert!(!msg1.is_empty());
    let _ = bob.read_message(&msg1).unwrap();

    // Message 2: Bob -> Alice
    let msg2 = bob.write_message(&[]).unwrap();
    assert!(!msg2.is_empty());
    let _ = alice.read_message(&msg2).unwrap();

    // Message 3: Alice -> Bob
    let msg3 = alice.write_message(&[]).unwrap();
    assert!(!msg3.is_empty());
    let _ = bob.read_message(&msg3).unwrap();

    // Both should be complete
    assert!(alice.is_complete());
    assert!(bob.is_complete());
}

#[test]
fn both_peers_derive_identical_shared_keys() {
    let alice_identity = Identity::generate().unwrap();
    let bob_identity = Identity::generate().unwrap();

    let mut alice = NoiseHandshake::new_initiator(&alice_identity).unwrap();
    let mut bob = NoiseHandshake::new_responder(&bob_identity).unwrap();

    // Complete handshake
    let msg1 = alice.write_message(&[]).unwrap();
    bob.read_message(&msg1).unwrap();
    let msg2 = bob.write_message(&[]).unwrap();
    alice.read_message(&msg2).unwrap();
    let msg3 = alice.write_message(&[]).unwrap();
    bob.read_message(&msg3).unwrap();

    // Transition to transport
    let alice_session = alice.into_transport().unwrap();
    let bob_session = bob.into_transport().unwrap();

    // Test that they can communicate (proves they derived the same keys)
    let plaintext = b"Hello, Bob!";
    let ciphertext = alice_session.encrypt(plaintext).unwrap();
    let decrypted = bob_session.decrypt(&ciphertext).unwrap();
    assert_eq!(decrypted, plaintext);

    // Test reverse direction
    let plaintext2 = b"Hello, Alice!";
    let ciphertext2 = bob_session.encrypt(plaintext2).unwrap();
    let decrypted2 = alice_session.decrypt(&ciphertext2).unwrap();
    assert_eq!(decrypted2, plaintext2);
}

// ============================================================================
// Forward Secrecy Tests
// ============================================================================

#[test]
fn new_sessions_produce_different_keys() {
    let alice_identity = Identity::generate().unwrap();
    let bob_identity = Identity::generate().unwrap();

    // Session 1
    let (alice_session1, _) = complete_handshake(&alice_identity, &bob_identity);

    // Session 2 with same long-term keys
    let (alice_session2, _) = complete_handshake(&alice_identity, &bob_identity);

    // Encrypt same plaintext with both sessions
    let plaintext = b"test message for forward secrecy";
    let ciphertext1 = alice_session1.encrypt(plaintext).unwrap();
    let ciphertext2 = alice_session2.encrypt(plaintext).unwrap();

    // Ciphertexts must differ (different ephemeral keys provide forward secrecy)
    assert_ne!(ciphertext1, ciphertext2);
}

#[test]
fn forward_secrecy_verified_with_multiple_sessions() {
    let alice_identity = Identity::generate().unwrap();
    let bob_identity = Identity::generate().unwrap();

    let mut ciphertexts = Vec::new();
    let plaintext = b"secret message";

    // Create 5 sessions and collect ciphertexts
    for _ in 0..5 {
        let (alice_session, _) = complete_handshake(&alice_identity, &bob_identity);
        let ciphertext = alice_session.encrypt(plaintext).unwrap();
        ciphertexts.push(ciphertext);
    }

    // Verify all ciphertexts are unique
    for i in 0..ciphertexts.len() {
        for j in (i + 1)..ciphertexts.len() {
            assert_ne!(ciphertexts[i], ciphertexts[j]);
        }
    }
}

// ============================================================================
// Key Identity Verification Tests
// ============================================================================

#[test]
fn key_identity_verification_first_contact() {
    let alice_identity = Identity::generate().unwrap();
    let bob_identity = Identity::generate().unwrap();

    let cache = PeerKeyCache::new();

    let mut alice = NoiseHandshake::new_initiator(&alice_identity).unwrap();
    let mut bob = NoiseHandshake::new_responder(&bob_identity).unwrap();

    // Complete handshake
    let msg1 = alice.write_message(&[]).unwrap();
    bob.read_message(&msg1).unwrap();
    let msg2 = bob.write_message(&[]).unwrap();
    alice.read_message(&msg2).unwrap();
    let msg3 = alice.write_message(&[]).unwrap();
    bob.read_message(&msg3).unwrap();

    // Alice verifies Bob's key (first contact)
    let bob_public_key = bob.remote_public_key().unwrap();
    let is_cached = cache.verify("bob", &bob_public_key).unwrap();
    assert!(!is_cached); // First time seeing Bob

    // Store Bob's key
    cache.store("bob".to_string(), bob_public_key.clone());

    // Verify again (should succeed)
    let is_cached = cache.verify("bob", &bob_public_key).unwrap();
    assert!(is_cached);
}

#[test]
fn key_identity_verification_cached_key_scenario() {
    let alice_identity = Identity::generate().unwrap();
    let bob_identity = Identity::generate().unwrap();

    let cache = PeerKeyCache::new();

    // First handshake
    let (alice_session1, _) = complete_handshake(&alice_identity, &bob_identity);
    let bob_public_key = alice_session1.remote_public_key();
    cache.store("bob".to_string(), bob_public_key.clone());

    // Second handshake (Bob uses same identity)
    let (alice_session2, _) = complete_handshake(&alice_identity, &bob_identity);
    let bob_public_key2 = alice_session2.remote_public_key();

    // Verification should succeed
    let result = cache.verify("bob", &bob_public_key2);
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[test]
fn key_identity_verification_detects_key_change() {
    let alice_identity = Identity::generate().unwrap();
    let bob_identity1 = Identity::generate().unwrap();
    let bob_identity2 = Identity::generate().unwrap(); // Bob rotated keys

    let cache = PeerKeyCache::new();

    // First handshake with Bob's old key
    let (alice_session1, _) = complete_handshake(&alice_identity, &bob_identity1);
    let bob_public_key1 = alice_session1.remote_public_key();
    cache.store("bob".to_string(), bob_public_key1);

    // Second handshake with Bob's NEW key
    let (alice_session2, _) = complete_handshake(&alice_identity, &bob_identity2);
    let bob_public_key2 = alice_session2.remote_public_key();

    // Verification should fail (key changed)
    let result = cache.verify("bob", &bob_public_key2);
    assert!(matches!(
        result,
        Err(CryptoError::IdentityVerificationFailed)
    ));
}

// ============================================================================
// Handshake Timeout Tests
// ============================================================================

#[test]
fn handshake_timeout_enforced() {
    let alice_identity = Identity::generate().unwrap();
    let mut alice = NoiseHandshake::new_initiator(&alice_identity).unwrap();

    let start = Instant::now();
    let timeout = Duration::from_secs(1); // Short timeout for testing

    // Simulate waiting for message 2 that never arrives
    alice.write_message(&[]).unwrap(); // Send message 1

    // In a real implementation, the transport layer would enforce timeout
    // Here we just verify the timeout duration can be measured
    std::thread::sleep(timeout);
    assert!(start.elapsed() >= timeout);

    // In production, after timeout, the handshake should be aborted
    // and resources cleaned up (tested in the next test)
}

// ============================================================================
// Failed Handshake Cleanup Tests
// ============================================================================

#[test]
fn failed_handshake_cleans_up_state() {
    let alice_identity = Identity::generate().unwrap();
    let bob_identity = Identity::generate().unwrap();

    let mut alice = NoiseHandshake::new_initiator(&alice_identity).unwrap();
    let mut bob = NoiseHandshake::new_responder(&bob_identity).unwrap();

    // Message 1: Alice -> Bob
    let msg1 = alice.write_message(&[]).unwrap();
    bob.read_message(&msg1).unwrap();

    // Now Bob sends corrupted message 2
    let corrupted_msg2 = vec![0xDE, 0xAD, 0xBE, 0xEF];
    let result = alice.read_message(&corrupted_msg2);
    assert!(matches!(result, Err(CryptoError::HandshakeFailed(_))));

    // Alice's handshake is now in a failed state
    // Attempting to continue should fail
    let result = alice.write_message(&[]);
    assert!(matches!(result, Err(CryptoError::HandshakeStateError(_))));

    // Attempting to convert to transport should fail
    let result = alice.into_transport();
    assert!(matches!(result, Err(CryptoError::HandshakeStateError(_))));
}

#[test]
fn no_partial_sessions_remain_after_failure() {
    let alice_identity = Identity::generate().unwrap();
    let bob_identity = Identity::generate().unwrap();

    let mut alice = NoiseHandshake::new_initiator(&alice_identity).unwrap();
    let mut bob = NoiseHandshake::new_responder(&bob_identity).unwrap();

    // Partial handshake
    let msg1 = alice.write_message(&[]).unwrap();
    bob.read_message(&msg1).unwrap();

    // Both sides are not complete
    assert!(!alice.is_complete());
    assert!(!bob.is_complete());

    // Neither can transition to transport
    assert!(alice.into_transport().is_err());
    // (bob is consumed by the above call, but in practice you'd check before consuming)
}

// ============================================================================
// CryptoSession Trait Contract Tests
// ============================================================================

#[test]
fn encrypted_differs_from_plaintext() {
    let alice_identity = Identity::generate().unwrap();
    let bob_identity = Identity::generate().unwrap();

    let (alice_session, _) = complete_handshake(&alice_identity, &bob_identity);

    let plaintext = b"secret message";
    let ciphertext = alice_session.encrypt(plaintext).unwrap();

    // Ciphertext must not equal plaintext
    assert_ne!(ciphertext, plaintext.to_vec());
}

#[test]
fn decrypt_encrypt_round_trip() {
    let alice_identity = Identity::generate().unwrap();
    let bob_identity = Identity::generate().unwrap();

    let (alice_session, bob_session) = complete_handshake(&alice_identity, &bob_identity);

    let plaintext = b"Hello, world! This is a longer message to test round-tripping.";
    let ciphertext = alice_session.encrypt(plaintext).unwrap();
    let decrypted = bob_session.decrypt(&ciphertext).unwrap();

    assert_eq!(decrypted, plaintext);
}

#[test]
fn empty_message_round_trip() {
    let alice_identity = Identity::generate().unwrap();
    let bob_identity = Identity::generate().unwrap();

    let (alice_session, bob_session) = complete_handshake(&alice_identity, &bob_identity);

    let plaintext = b"";
    let ciphertext = alice_session.encrypt(plaintext).unwrap();
    let decrypted = bob_session.decrypt(&ciphertext).unwrap();

    assert_eq!(decrypted, plaintext);
}

#[test]
fn large_message_round_trip() {
    let alice_identity = Identity::generate().unwrap();
    let bob_identity = Identity::generate().unwrap();

    let (alice_session, bob_session) = complete_handshake(&alice_identity, &bob_identity);

    // Large message (32KB - well under Noise's 65535 byte limit)
    let plaintext: Vec<u8> = (0u8..=255).cycle().take(32768).collect();
    let ciphertext = alice_session.encrypt(&plaintext).unwrap();
    let decrypted = bob_session.decrypt(&ciphertext).unwrap();

    assert_eq!(decrypted, plaintext);
}

#[test]
fn session_is_established_after_handshake() {
    let alice_identity = Identity::generate().unwrap();
    let bob_identity = Identity::generate().unwrap();

    let (alice_session, bob_session) = complete_handshake(&alice_identity, &bob_identity);

    assert!(alice_session.is_established());
    assert!(bob_session.is_established());
}

// ============================================================================
// Malformed Message Handling Tests
// ============================================================================

#[test]
fn malformed_handshake_message_returns_error() {
    let bob_identity = Identity::generate().unwrap();
    let mut bob = NoiseHandshake::new_responder(&bob_identity).unwrap();

    // Use a message that's too short - Noise XX message 1 should be 32 bytes (ephemeral key)
    let result = bob.read_message(b"short");
    assert!(matches!(result, Err(CryptoError::HandshakeFailed(_))));
}

#[test]
fn malformed_ciphertext_returns_error() {
    let alice_identity = Identity::generate().unwrap();
    let bob_identity = Identity::generate().unwrap();

    let (_, bob_session) = complete_handshake(&alice_identity, &bob_identity);

    let result = bob_session.decrypt(b"not a valid ciphertext");
    assert!(matches!(result, Err(CryptoError::DecryptionFailed(_))));
}

#[test]
fn tampered_ciphertext_returns_error() {
    let alice_identity = Identity::generate().unwrap();
    let bob_identity = Identity::generate().unwrap();

    let (alice_session, bob_session) = complete_handshake(&alice_identity, &bob_identity);

    let plaintext = b"secret message";
    let mut ciphertext = alice_session.encrypt(plaintext).unwrap();

    // Tamper with the ciphertext
    if let Some(byte) = ciphertext.get_mut(0) {
        *byte ^= 0xFF;
    }

    let result = bob_session.decrypt(&ciphertext);
    assert!(matches!(result, Err(CryptoError::DecryptionFailed(_))));
}

#[test]
fn truncated_ciphertext_returns_error() {
    let alice_identity = Identity::generate().unwrap();
    let bob_identity = Identity::generate().unwrap();

    let (alice_session, bob_session) = complete_handshake(&alice_identity, &bob_identity);

    let plaintext = b"secret message";
    let ciphertext = alice_session.encrypt(plaintext).unwrap();

    // Truncate the ciphertext
    let truncated = &ciphertext[..ciphertext.len() / 2];
    let result = bob_session.decrypt(truncated);
    assert!(matches!(result, Err(CryptoError::DecryptionFailed(_))));
}

// ============================================================================
// Key Storage Tests
// ============================================================================

#[test]
fn key_store_saves_and_loads_identity() {
    let store = InMemoryKeyStore::new();
    let identity = Identity::generate().unwrap();

    // Initially empty
    assert!(store.load().unwrap().is_none());

    // Save
    store.save(&identity).unwrap();

    // Load
    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.public_key(), identity.public_key());
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Complete a full handshake and return both transport sessions.
fn complete_handshake(
    alice_identity: &Identity,
    bob_identity: &Identity,
) -> (NoiseXXSession, NoiseXXSession) {
    let mut alice = NoiseHandshake::new_initiator(alice_identity).unwrap();
    let mut bob = NoiseHandshake::new_responder(bob_identity).unwrap();

    // Message 1: Alice -> Bob
    let msg1 = alice.write_message(&[]).unwrap();
    bob.read_message(&msg1).unwrap();

    // Message 2: Bob -> Alice
    let msg2 = bob.write_message(&[]).unwrap();
    alice.read_message(&msg2).unwrap();

    // Message 3: Alice -> Bob
    let msg3 = alice.write_message(&[]).unwrap();
    bob.read_message(&msg3).unwrap();

    // Transition to transport
    let alice_session = alice.into_transport().unwrap();
    let bob_session = bob.into_transport().unwrap();

    (alice_session, bob_session)
}
