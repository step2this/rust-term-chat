# Use Case: UC-014 Refactor ChatManager into Focused Submodules

## Classification
- **Goal Level**: :fish: Subfunction
- **Scope**: Component (white box)
- **Priority**: P2 Medium
- **Complexity**: :green_circle: Low

## Actors
- **Primary Actor**: Developer (maintainer improving code organization)
- **Supporting Actors**: None (internal refactor, no runtime behavior change)
- **Stakeholders & Interests**:
  - Maintainer: ChatManager is easier to navigate, test, and extend
  - Future contributors: clear module boundaries reduce onboarding friction

## Conditions
- **Preconditions** (must be true before starting):
  1. Workspace compiles (`cargo check --workspace` passes)
  2. All existing tests pass (`cargo test --workspace`)
  3. ChatManager exists as a single module (`termchat/src/chat/mod.rs`)
- **Success Postconditions** (true when done right):
  1. ChatManager production code in `mod.rs` is under 300 lines (non-test)
  2. Send logic extracted to `chat/send.rs` (send_message, send_message_with_retry, send_presence, send_typing)
  3. Receive logic extracted to `chat/receive.rs` (receive_one, sender_id_matches_peer, check_timestamp_skew)
  4. Ack/retry logic extracted to `chat/ack.rs` (RetryConfig, await_ack, wait_for_ack)
  5. All existing tests continue to pass with zero failures
  6. No public API changes — callers are unaffected
  7. `cargo clippy --workspace -- -D warnings` passes
- **Failure Postconditions** (true when it fails gracefully):
  1. If extraction breaks compilation, revert to single-module state
- **Invariants** (must remain true throughout):
  1. All 695+ tests continue to pass
  2. No production behavior changes
  3. No public API surface changes

## Main Success Scenario
1. Developer identifies ChatManager as too large (~625 lines of production code)
2. Developer extracts ack/retry logic into `chat/ack.rs`
3. Developer extracts send pipeline into `chat/send.rs`
4. Developer extracts receive pipeline into `chat/receive.rs`
5. Developer updates `chat/mod.rs` to re-export submodule items and retain core struct/state
6. Developer verifies all tests pass and clippy is clean

## Extensions
- **6a. Extracted function has private dependency on ChatManager internals**:
  1. Adjust visibility or pass required state as parameters
  2. Return to step 6

## Agent Execution Notes
- **Verification Command**: `cargo test -p termchat --lib && cargo clippy --workspace -- -D warnings`
- **Test File**: Existing tests in `termchat/src/chat/mod.rs` (test module) and integration tests
- **Depends On**: UC-001, UC-002 (ChatManager must exist)
- **Blocks**: None
- **Estimated Complexity**: Small / ~500 tokens per agent turn
- **Agent Assignment**: Teammate:Builder-Proto

## Acceptance Criteria
- [x] `mod.rs` production code under 300 lines
- [x] `ack.rs`, `send.rs`, `receive.rs` created with extracted logic
- [x] All 695 tests pass
- [x] `cargo clippy --workspace -- -D warnings` passes
- [x] `cargo deny check` passes
- [x] No public API changes
