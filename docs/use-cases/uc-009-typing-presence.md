# Use Case: UC-009 Display Typing Indicators and Presence Status

## Classification
- **Goal Level**: Sea Level (User Goal)
- **Scope**: System (black box)
- **Priority**: P2 Medium
- **Complexity**: Medium

## Actors
- **Primary Actor**: Terminal User (Sender/Receiver)
- **Supporting Actors**: Transport Layer, Crypto Layer, Remote Peers

## Conditions
- **Preconditions**:
  1. User has a valid identity keypair
  2. At least one transport (P2P or relay) is available
  3. E2E encryption session is established (UC-005)
- **Success Postconditions**:
  1. Sidebar shows presence dots (green=online, yellow=away, gray=offline) for DM conversations
  2. Chat panel shows "X is typing..." indicator when remote peer types
  3. Local typing detection triggers on keystroke and expires after 3s idle
  4. Presence and typing messages travel encrypted through the existing pipeline
  5. Fire-and-forget: send failures are silently logged, never crash
- **Failure Postconditions**:
  1. Transport failure does not crash or stall the UI
  2. Malformed presence/typing messages are logged and dropped
- **Invariants**:
  1. Presence and typing data never leaves the application boundary as plaintext
  2. Typing indicators are transient (no persistence)

## Main Success Scenario
1. User opens TermChat; sidebar shows presence dots next to DM conversations
2. Remote peer starts typing; chat panel shows "Alice is typing..."
3. Remote peer stops typing (or 3s timeout); typing indicator disappears
4. Remote peer goes away; sidebar presence dot changes to yellow
5. Remote peer disconnects; sidebar presence dot changes to gray/hollow

## Extensions
- **2a. Multiple peers typing simultaneously**:
  1. Chat panel shows "Alice and Bob are typing..."
  2. For 3+ peers: "Alice and 2 others are typing..."
- **4a. Presence message decode fails**:
  1. System logs warning and drops the message
  2. No UI change occurs

## Agent Execution Notes
- **Verification Command**: `cargo test --test presence_typing`
- **Test File**: `tests/integration/presence_typing.rs`
- **Depends On**: UC-001 (Send), UC-002 (Receive), UC-005 (E2E Handshake)
- **Estimated Complexity**: Medium

## Acceptance Criteria
- [x] `cargo test --test presence_typing` passes (17 tests)
- [x] `cargo test` passes (499 total, 0 regressions)
- [x] `cargo fmt --check` passes
- [x] `cargo clippy -- -D warnings` passes
- [x] Presence dots render in sidebar for DM conversations
- [x] Typing indicator renders in chat panel
- [x] Local typing detection triggers on keystroke, expires after 3s
- [x] Presence/typing messages round-trip through encrypted pipeline
- [x] Fire-and-forget: disconnected transport does not panic
