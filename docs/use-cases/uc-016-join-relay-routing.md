# Use Case: UC-016 Route JoinApproved/JoinDenied via Relay

## Classification
- **Goal Level**: 🌊 User Goal
- **Scope**: System (black box)
- **Priority**: P2 Medium
- **Complexity**: 🟡 Medium

## Actors
- **Primary Actor**: Room Admin (terminal user who owns the room)
- **Supporting Actors**: Relay Server, Joining Peer
- **Stakeholders & Interests**:
  - Room Admin: approval/denial is delivered to the joining peer
  - Joining Peer: receives the join response so they can enter the room or show a denial reason
  - Relay Server: routes responses correctly based on `target_peer_id`

## Conditions
- **Preconditions**:
  1. Room is registered on the relay server
  2. Joining peer has sent a `JoinRequest` that was delivered to the admin
  3. Admin is connected to the relay and has decided to approve or deny
- **Success Postconditions**:
  1. Joining peer receives `JoinApproved` with room metadata and member list, OR `JoinDenied` with reason
  2. Relay correctly routes based on `target_peer_id` field
  3. If joining peer is offline, the message is queued for later delivery
- **Failure Postconditions**:
  1. Admin receives a `RelayMessage::Error` if the target peer cannot be reached and queuing fails
  2. No message is silently dropped
- **Invariants**:
  1. The relay never modifies the room message content (opaque forwarding)
  2. Messages are routed only to the intended `target_peer_id`

## Main Success Scenario
1. Admin approves a join request in the UI
2. Client constructs a `JoinApproved` message with `target_peer_id` set to the requester
3. Client sends the message wrapped in `RelayMessage::Room` to the relay
4. Relay decodes the `RoomMessage`, extracts `target_peer_id`
5. Relay looks up the target peer in the connection registry
6. Relay forwards the `JoinApproved` to the target peer as a `RelayMessage::Room`
7. Joining peer receives the approval and enters the room

## Extensions (What Can Go Wrong)
- **5a. Target peer is offline**:
  1. Relay queues the room message bytes for later delivery
  2. When target peer reconnects, queued messages are drained
- **5b. Target peer is unknown (never registered)**:
  1. Relay queues the message (same as offline)
- **3a. Admin sends JoinDenied instead**:
  1. Same routing path as JoinApproved, using `target_peer_id` from JoinDenied
  2. Joining peer receives denial with reason string
- **4a. Room message decode fails**:
  1. Relay logs a warning and drops the message (existing behavior)

## Variations
- **1a.** Admin denies the join request -> `JoinDenied` with reason follows the same routing path

## Agent Execution Notes
- **Verification Command**: `cargo test --test room_management`
- **Test File**: `tests/integration/room_management.rs`
- **Depends On**: UC-006 (Create Room)
- **Blocks**: None
- **Estimated Complexity**: Medium / ~15 tool calls
- **Agent Assignment**: Teammate:Builder-Relay

## Acceptance Criteria
- [ ] `target_peer_id` field added to `JoinApproved` and `JoinDenied` variants
- [ ] Relay routes `JoinApproved` to `target_peer_id` via `route_room_message()`
- [ ] Relay routes `JoinDenied` to `target_peer_id` via `route_room_message()`
- [ ] If target peer is offline, message is queued for later delivery
- [ ] Integration test: join approved reaches requester via relay
- [ ] Integration test: join denied reaches requester via relay
- [ ] All existing room management tests still pass
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo fmt --check` passes
