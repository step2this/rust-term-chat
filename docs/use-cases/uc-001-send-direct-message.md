# Use Case: UC-001 Send Direct Message

## Classification
- **Goal Level**: 🌊 User Goal
- **Scope**: System (black box)
- **Priority**: P0 Critical
- **Complexity**: 🟡 Medium

## Actors
- **Primary Actor**: Sender (terminal user)
- **Supporting Actors**: Transport Layer, Crypto Layer, Recipient
- **Stakeholders & Interests**:
  - Sender: message is delivered, confirmed, and encrypted
  - Recipient: message arrives intact, sender is authenticated
  - System: no plaintext ever touches the network

## Conditions
- **Preconditions** (must be true before starting):
  1. Sender has a valid identity keypair
  2. Sender knows Recipient's public identity (keypair exchange has occurred; Noise session may or may not be established yet)
  3. At least one transport (P2P or relay) is available
- **Success Postconditions** (true when done right):
  1. Message is stored in Sender's local history
  2. Message is delivered to Recipient (or queued at relay)
  3. Sender sees delivery confirmation in UI
  4. Message was encrypted in transit (never plaintext on wire)
- **Failure Postconditions** (true when it fails gracefully):
  1. Sender sees clear error message
  2. Message is saved locally as "pending"
  3. System will retry when transport is available
- **Invariants** (must remain true throughout):
  1. Plaintext message never leaves the application boundary
  2. Message ordering is preserved per-conversation

## Main Success Scenario
1. Sender types message in input box and presses Enter
2. System validates message (non-empty, within size limit)
3. System serializes message with metadata (timestamp, sender ID, conversation ID)
4. System encrypts serialized message using established Noise session
5. System selects best available transport (P2P preferred, relay fallback)
6. System transmits encrypted payload
7. System receives delivery acknowledgment
8. System updates local history with "delivered" status
9. System renders delivery confirmation in UI

## Extensions (What Can Go Wrong)
- **2a. Message is empty**:
  1. System ignores the submission (no-op)
  2. Input box remains focused, returns to step 1
- **2b. Message exceeds size limit (64KB)**:
  1. System shows error: "Message too long (max 64KB)"
  2. Message remains in input box for editing, returns to step 1
- **3a. Serialization fails**:
  1. System logs internal error with details
  2. System shows error: "Internal error: could not prepare message"
  3. Message remains in input box, returns to step 1
- **4a. No Noise session established**:
  1. System initiates E2E handshake (UC-005)
  2. On success, returns to step 4
  3. On failure, shows error "Could not establish secure connection", saves as "pending"
- **5a. P2P connection fails**:
  1. System falls back to relay transport
  2. Returns to step 6
- **5b. No transport available (offline)**:
  1. System saves message as "pending" in local history
  2. System shows "queued — will send when connected"
  3. Use case ends (retry handled by background process)
- **6a. Transmission fails mid-send**:
  1. System retries transmission once on same transport
  2. If retry fails, falls back to alternate transport (step 5a/5b logic)
- **8a. Local history write fails (disk full, database error)**:
  1. System logs storage error
  2. System shows warning: "Message delivered but could not save to history"
  3. Delivery confirmation still shown (message was sent successfully)
  4. System queues history write for retry
- **7a. No acknowledgment within timeout (10s)**:
  1. System retries transmission once
  2. If still no ack, marks message as "sent" (not "delivered")
  3. System shows "sent but unconfirmed" in UI

## Variations
- **1a.** Sender may paste multiline text — message includes newlines, displayed as-is
- **1b.** Sender may use /commands (e.g., `/ask-agent`) — routed to command handler, not sent as message

## Agent Execution Notes
- **Verification Command**: `cargo test --test send_receive`
- **Test File**: `tests/integration/send_receive.rs`
- **Depends On**: UC-005 (E2E Handshake)
- **Blocks**: UC-007 (Agent Join — agents send messages too)
- **Estimated Complexity**: M / ~2000 tokens per agent turn
- **Agent Assignment**: Teammate:Builder

## Acceptance Criteria (for grading)
- [ ] All success postconditions verified by automated test
- [ ] All extension paths have explicit handling
- [ ] No invariant violations detected
- [ ] Code passes lint + clippy (`cargo clippy -- -D warnings`)
- [ ] Reviewer agent approves
- [ ] Message round-trips between two local instances
- [ ] Wire capture shows only encrypted bytes (no plaintext)
- [ ] Offline message queuing works (send while disconnected, delivers on reconnect)
- [ ] UI shows sent → delivered status transition
