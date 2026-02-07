# Use Case: UC-002 Receive Direct Message

## Classification
- **Goal Level**: 🌊 User Goal
- **Scope**: System (black box)
- **Priority**: P0 Critical
- **Complexity**: 🟡 Medium

## Actors
- **Primary Actor**: Recipient (terminal user)
- **Supporting Actors**: Transport Layer, Crypto Layer, Sender, History Store
- **Stakeholders & Interests**:
  - Recipient: message arrives intact, promptly displayed, sender is authenticated
  - Sender: delivery acknowledgment is sent back reliably
  - System: no plaintext ever touches the network, messages are persisted locally

## Conditions
- **Preconditions** (must be true before starting):
  1. Recipient has a valid identity keypair
  2. Recipient knows Sender's public identity (keypair exchange has occurred; Noise session may or may not be established yet)
  3. Recipient's application is running and listening on at least one transport
- **Success Postconditions** (true when done right):
  1. Decrypted message is displayed in the correct conversation in the UI
  2. Message is stored in Recipient's local history with correct metadata
  3. Delivery acknowledgment is sent back to Sender
  4. Message was decrypted only inside the application boundary (never plaintext on wire)
  5. UI scrolls or notifies if Recipient is viewing a different conversation
- **Failure Postconditions** (true when it fails gracefully):
  1. Recipient sees an error indicator (e.g., "failed to decrypt message") rather than a crash
  2. Corrupted or undecryptable messages are logged for debugging but not displayed as content
  3. Sender does NOT receive a false delivery acknowledgment
- **Invariants** (must remain true throughout):
  1. Plaintext message never leaves the application boundary
  2. Message ordering is preserved per-conversation
  3. Duplicate messages are detected and deduplicated

## Main Success Scenario
1. Transport Layer receives an encrypted payload from network
2. System tentatively identifies the source peer from transport connection metadata (authoritative identity is verified cryptographically in step 4)
3. System looks up the Noise session for that peer
4. System decrypts the payload using the Noise session
5. System deserializes the decrypted bytes into a message struct
6. System validates message metadata (timestamp, sender ID, conversation ID)
7. System stores the message in local history
8. System sends delivery acknowledgment back to Sender via same transport
9. System renders the message in the appropriate conversation panel
10. System updates the sidebar (unread count, conversation ordering)

## Extensions (What Can Go Wrong)
- **1a. Payload exceeds maximum size (64KB)**:
  1. System drops the payload silently
  2. System logs a warning with source peer info
  3. No acknowledgment is sent, use case ends
- **3a. No Noise session found for source peer**:
  1. System checks if a handshake is in progress
  2. If yes, buffers the payload (max 10 buffered) and waits for handshake completion
  3. If no, initiates E2E handshake (UC-005), buffers payload
  4. On handshake success, returns to step 4 for buffered payloads
  5. On handshake failure, drops buffered payloads, logs error, use case ends
- **4a. Decryption fails (corrupted or tampered payload)**:
  1. System logs decryption error with payload hash (not content)
  2. System does NOT display any content to Recipient
  3. System does NOT send delivery acknowledgment
  4. Use case ends
- **4b. Decryption fails due to out-of-order Noise nonce**:
  1. System attempts to resync nonce window (accept nonces within a sliding window)
  2. If resync succeeds, returns to step 4
  3. If resync fails, treats as corrupted payload (extension 4a)
- **5a. Deserialization fails (unknown message format or version)**:
  1. System logs deserialization error
  2. System sends a NACK (negative acknowledgment) to Sender indicating format error
  3. Use case ends
- **6a. Message metadata fails validation (e.g., timestamp too far in future/past)**:
  1. System logs the validation failure
  2. System displays the message with a warning indicator ("clock skew detected")
  3. Continues to step 7
- **6c. Sender ID in message metadata does not match Noise-authenticated peer identity**:
  1. System rejects the message (potential spoofing or relay injection)
  2. System logs security warning with both IDs (transport-authenticated vs. claimed)
  3. System does NOT display the message
  4. System does NOT send delivery acknowledgment
  5. Use case ends
- **6b. Conversation ID references unknown conversation**:
  1. System creates a new conversation for this peer
  2. Returns to step 7
- **7a. Local history storage fails (disk full, database error)**:
  1. System displays the message in UI (still useful even if not persisted)
  2. System shows warning: "Message could not be saved to history"
  3. System still sends acknowledgment (step 8)
  4. Continues to step 9
- **8a. Acknowledgment transmission fails**:
  1. System queues the acknowledgment for retry
  2. Continues to step 9 (display is not blocked by ack failure)
- **9a. Conversation panel is not currently visible (Recipient is in a different conversation)**:
  1. System increments unread count for the conversation in sidebar
  2. System shows notification indicator (bold conversation name, badge count)
  3. If terminal bell is enabled, rings bell
  4. Message is still stored (step 7 already completed)

## Variations
- **1a.** Payload may arrive via P2P (QUIC) or relay (WebSocket) — decryption path is identical regardless of transport
- **5a.** Message may be a control message (typing indicator, read receipt) rather than a chat message — routed to appropriate handler instead of chat display
- **9a.** Recipient may have multiple conversations open in split view — message renders in the correct pane

## Agent Execution Notes
- **Verification Command**: `cargo test --test send_receive`
- **Test File**: `tests/integration/send_receive.rs`
- **Depends On**: UC-005 (E2E Handshake)
- **Blocks**: UC-007 (Agent Join — agents receive messages too)
- **Estimated Complexity**: M / ~2000 tokens per agent turn
- **Agent Assignment**: Teammate:Builder

## Acceptance Criteria (for grading)
- [ ] All success postconditions verified by automated test
- [ ] All extension paths have explicit handling
- [ ] No invariant violations detected
- [ ] Code passes lint + clippy (`cargo clippy -- -D warnings`)
- [ ] Reviewer agent approves
- [ ] Received messages display in correct conversation with correct metadata
- [ ] Delivery acknowledgment is sent back to Sender
- [ ] Corrupted/tampered payloads are rejected without crash or content leak
- [ ] Duplicate messages are deduplicated
- [ ] Unread indicators update when message arrives in non-active conversation
- [ ] Messages persist in local history across application restarts
