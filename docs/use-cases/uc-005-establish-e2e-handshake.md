# Use Case: UC-005 Establish E2E Handshake

## Classification
- **Goal Level**: 🐟 Subfunction
- **Scope**: System (black box)
- **Priority**: P0 Critical
- **Complexity**: 🔴 High

## Actors
- **Primary Actor**: Initiator (the peer that starts the handshake — either terminal user's client)
- **Supporting Actors**: Responder (the other peer), Transport Layer, Key Store
- **Stakeholders & Interests**:
  - Initiator: establish a secure channel with verified peer identity
  - Responder: authenticate the initiator before accepting messages
  - System: all subsequent messages are encrypted with forward secrecy; no key material leaks

## Conditions
- **Preconditions** (must be true before starting):
  1. Initiator has a valid long-term identity keypair (x25519) stored in the Key Store
  2. Responder has a valid long-term identity keypair (x25519) stored in the Key Store
  3. A transport connection exists between Initiator and Responder (P2P or relay)
  4. Initiator knows the Responder's peer ID (but NOT necessarily their public key — XX pattern handles this)
- **Success Postconditions** (true when done right):
  1. A Noise XX session is established between Initiator and Responder
  2. Both peers have authenticated each other's long-term identity keys
  3. A shared symmetric key (ChaCha20-Poly1305) is derived for message encryption
  4. Ephemeral keys were used — compromising long-term keys cannot decrypt this session (forward secrecy)
  5. The session is stored in memory, keyed by peer ID, for use by UC-001/UC-002
  6. Both peers' public keys are cached locally for future verification
- **Failure Postconditions** (true when it fails gracefully):
  1. No partial session state remains (clean rollback)
  2. Initiator sees a clear error message ("Handshake failed: <reason>")
  3. No key material is logged or transmitted in plaintext
  4. The transport connection remains usable (handshake failure doesn't kill the connection)
- **Invariants** (must remain true throughout):
  1. Key material (private keys, ephemeral keys, shared secrets) is never logged, serialized to disk, or transmitted in plaintext
  2. Handshake messages follow the Noise XX pattern exactly (no custom deviations)
  3. Both sides must complete all 3 handshake messages before the session is usable

## Main Success Scenario
1. Initiator generates an ephemeral x25519 keypair
2. Initiator constructs Noise XX message 1 (→ e) and sends it via Transport Layer
3. Responder receives message 1, generates their own ephemeral keypair
4. Responder constructs Noise XX message 2 (← e, ee, s, es) containing their encrypted static key
5. Responder sends message 2 via Transport Layer
6. Initiator receives message 2, decrypts Responder's static key, verifies identity
7. Initiator constructs Noise XX message 3 (→ s, se) containing their encrypted static key
8. Initiator sends message 3 via Transport Layer
9. Responder receives message 3, decrypts Initiator's static key, verifies identity
10. Both peers derive the shared transport keys (CipherState pair for send/receive)
11. Both peers store the Noise session in memory, keyed by peer ID
12. Handshake is complete — both peers can now encrypt/decrypt messages

## Extensions (What Can Go Wrong)
- **1a. Ephemeral key generation fails (CSPRNG error)**:
  1. Initiator reports "Handshake failed: could not generate ephemeral keypair"
  2. No handshake state to clean up
  3. Use case fails
- **2a. Transport send fails (connection dropped)**:
  1. Initiator discards ephemeral keypair (no reuse)
  2. Initiator reports "Handshake failed: connection lost"
  3. Use case fails; caller (UC-001) may retry with a new connection
- **3a. Responder ephemeral key generation fails (CSPRNG error)**:
  1. Responder drops received message 1
  2. Responder logs error "Handshake failed: could not generate ephemeral keypair"
  3. Use case fails; Initiator will time out (extension 6b)
- **3b. Message 1 is malformed or corrupted**:
  1. Responder's Noise library returns a parse/processing error
  2. Responder logs error with payload hash (not content)
  3. Responder discards the message, no state created
  4. Use case fails; Initiator will time out (extension 6b)
- **4a. Responder is already in a handshake with this Initiator**:
  1. Responder compares peer IDs lexicographically to break the tie
  2. The peer with the lower ID becomes the Initiator; the other becomes Responder
  3. The losing handshake is abandoned, returns to step 3 in the winning role
- **5a. Transport send fails on message 2**:
  1. Responder discards ephemeral keypair
  2. Responder cleans up partial handshake state
  3. Use case fails; Initiator will time out (extension 6b)
- **6a. Responder's static key fails identity verification**:
  1. Initiator checks if Responder's key is in the local known-keys cache
  2. If key is unknown (first contact): prompt user "New peer detected. Accept key fingerprint <fingerprint>?"
  3. If key has CHANGED since last contact: show warning "Peer identity has changed! Previous fingerprint: <old>. New: <new>. Accept?"
  4. If user rejects: Initiator aborts handshake, reports "Handshake rejected: identity not trusted"
  5. If user accepts: Initiator caches the new key, returns to step 7
- **6c. Message 2 is malformed or fails Noise processing**:
  1. Initiator's Noise library returns a decryption/parse error
  2. Initiator discards ephemeral keypair and all handshake state
  3. Initiator reports "Handshake failed: invalid message from peer"
  4. Use case fails
- **6b. Message 2 not received within timeout (15s)**:
  1. Initiator discards ephemeral keypair
  2. Initiator reports "Handshake timed out waiting for response"
  3. Use case fails
- **8a. Transport send fails on message 3**:
  1. Initiator discards all handshake state
  2. Initiator reports "Handshake failed: connection lost during final step"
  3. Use case fails; Responder will time out
- **9a. Initiator's static key fails identity verification on Responder side**:
  1. Same key verification logic as extension 6a but from Responder's perspective
  2. If rejected, Responder sends an error message and aborts
  3. Initiator receives abort, reports "Handshake rejected by peer"
- **9c. Message 3 is malformed or fails Noise processing**:
  1. Responder's Noise library returns a decryption/parse error
  2. Responder discards all handshake state
  3. Responder logs error "Handshake failed: invalid message from peer"
  4. Use case fails; Initiator has a half-established session that must be invalidated
- **9b. Message 3 not received within timeout (15s)**:
  1. Responder discards all handshake state
  2. Responder logs timeout (Initiator may have crashed)
  3. Use case fails
- **10a. Key derivation produces weak or zero key material**:
  1. Both peers detect this via a key validation check
  2. Both peers abort the session and discard all state
  3. Report "Handshake failed: key derivation error" (this should never happen with correct Noise implementation)

## Variations
- **6a-alt.** In automated/CI mode, key verification may use a trust-on-first-use (TOFU) policy without user prompts
- **1a.** If peers have previously handshaked and cached each other's keys, the UI skips the "new peer" prompt (still performs cryptographic verification)

## Agent Execution Notes
- **Verification Command**: `cargo test --test e2e_encryption`
- **Test File**: `tests/integration/e2e_encryption.rs`
- **Depends On**: None (foundational — requires only key generation)
- **Blocks**: UC-001 (Send Direct Message), UC-002 (Receive Direct Message)
- **Estimated Complexity**: L / ~3000 tokens per agent turn
- **Agent Assignment**: Teammate:Builder

## Acceptance Criteria (for grading)
- [ ] All success postconditions verified by automated test
- [ ] All extension paths have explicit handling
- [ ] No invariant violations detected
- [ ] Code passes lint + clippy (`cargo clippy -- -D warnings`)
- [ ] Reviewer agent approves
- [ ] Noise XX 3-message handshake completes between two local instances
- [ ] Both peers derive identical shared keys (verified by encrypting/decrypting a test message)
- [ ] Forward secrecy: new sessions produce different keys even with same long-term keys
- [ ] Key identity verification works (first contact, changed key, cached key scenarios)
- [ ] Handshake timeout is enforced (15s per message)
- [ ] Concurrent handshake collision is resolved deterministically
- [ ] No key material appears in logs (verified by log capture in test)
- [ ] Failed handshake cleans up all state (no partial sessions remain)
