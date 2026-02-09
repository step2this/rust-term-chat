# Use Case: UC-015 Agent Crypto/Transport Fan-Out

## Classification
- **Goal Level**: Subfunction
- **Scope**: Component (white box)
- **Priority**: P2 Medium
- **Complexity**: Medium

## Actors
- **Primary Actor**: Agent Participant (automated)
- **Supporting Actors**: ChatManager, NoiseXXSession, LoopbackTransport
- **Stakeholders & Interests**:
  - Security Auditor: agent fan-out messages are encrypted with real Noise XX crypto, not stub XOR
  - Developer: confidence that the crypto pipeline works end-to-end for agent-originated messages

## Conditions
- **Preconditions**:
  1. NoiseHandshake and NoiseXXSession exist and are functional (UC-005)
  2. AgentParticipant exists and can produce OutboundAgentMessage (UC-007)
  3. ChatManager send pipeline (validate -> serialize -> encrypt -> transmit) is operational (UC-001)
- **Success Postconditions**:
  1. Integration test passes showing a message encrypted with real Noise XX (not stub XOR)
  2. Ciphertext on the wire differs from plaintext
  3. Ciphertext includes AEAD tag overhead (plaintext.len() + 16 bytes)
  4. Recipient decrypts the message successfully and recovers original content
- **Failure Postconditions**:
  1. If handshake fails, no transport session is created and test reports the failure
  2. If decryption fails, test reports the mismatch
- **Invariants**:
  1. Plaintext never appears on the transport wire
  2. Message ordering is preserved

## Main Success Scenario
1. Test generates two Identity keypairs (alice, bob)
2. Test performs the 3-message Noise XX handshake dance between initiator and responder
3. Both sides transition to transport mode via `into_transport()`, yielding `NoiseXXSession` pairs
4. Test creates a ChatManager with the real `NoiseXXSession` (alice side) and `LoopbackTransport`
5. Test creates a second ChatManager with the real `NoiseXXSession` (bob side) and the other LoopbackTransport endpoint
6. Alice sends a message through her ChatManager
7. Bob receives and decrypts the message through his ChatManager
8. Test verifies the decrypted content matches the original plaintext
9. Test verifies the wire bytes (captured from raw transport) contain no plaintext substring

## Extensions
- **2a. Handshake fails (corrupted message)**:
  1. `NoiseHandshake::read_message` returns `CryptoError::HandshakeFailed`
  2. Test reports failure (no session created)
- **6a. Encryption fails**:
  1. `CryptoSession::encrypt` returns `CryptoError::EncryptionFailed`
  2. `ChatManager::send_message` returns `SendError::Crypto`
- **7a. Decryption fails (tampered ciphertext)**:
  1. `CryptoSession::decrypt` returns `CryptoError::DecryptionFailed`
  2. `ChatManager::receive_one` returns `SendError::Crypto`

## Variations
- **4a.** LoopbackTransport is used instead of real network transport (testing crypto, not network)

## Agent Execution Notes
- **Verification Command**: `cargo test --test agent_bridge`
- **Test File**: `tests/integration/agent_bridge.rs`
- **Depends On**: UC-005 (E2E Handshake), UC-007 (Agent Join)
- **Blocks**: None
- **Estimated Complexity**: Medium / ~15 tool calls
- **Agent Assignment**: Teammate:Builder-Agent

## Acceptance Criteria
- [ ] `cargo test --test agent_bridge` passes with new Noise XX test
- [ ] Message round-trips between two ChatManagers using real NoiseXXSession
- [ ] Wire capture shows only encrypted bytes (no plaintext)
- [ ] Ciphertext length includes AEAD overhead (plaintext + 16 bytes)
- [ ] cargo clippy -- -D warnings passes
- [ ] cargo fmt --check passes
