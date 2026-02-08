# Use Case: UC-006 Create Room

## Classification
- **Goal Level**: 🌊 User Goal
- **Scope**: System (black box)
- **Priority**: P2 Medium
- **Complexity**: 🔴 High

## Actors
- **Primary Actor**: Room Creator (terminal user who initiates room creation)
- **Secondary Actor**: Joiner (terminal user who discovers and requests to join the room — takes over as active actor in steps 8-16)
- **Supporting Actors**: Transport Layer, Crypto Layer, Relay Server
- **Stakeholders & Interests**:
  - Room Creator: wants to create a named space for group conversation with minimal friction
  - Room Members: want to discover and join rooms easily without complex setup
  - System: room metadata and membership must be consistent across all participants
  - Security: only approved members should receive room messages; admin controls who enters

## Conditions
- **Preconditions** (must be true before starting):
  1. Creator has a valid identity keypair (from UC-005)
  2. At least one transport (P2P or relay) is available (from UC-003/UC-004)
  3. TUI is running and responsive (from Phase 1)
  4. Creator has established per-peer Noise sessions (UC-005) with any peers they want to invite, OR the relay is available for routing JoinRequests to peers the Creator hasn't yet connected with
- **Success Postconditions** (true when done right):
  1. A new Room exists with a unique RoomId, display name, and the Creator as admin
  2. Room appears in Creator's sidebar conversation list
  3. Room metadata (name, RoomId, admin list, member list) is serializable and can be shared with peers
  4. Room is discoverable by other peers via the relay server's room registry (via `ListRooms`/`RoomList` protocol messages)
  5. A join request from another peer can be approved or denied by the admin
  6. Approved members can send a message with the room's ConversationId and all other members receive it via their respective per-peer transports (fan-out at application layer)
  7. Room membership changes are broadcast to all current members
- **Failure Postconditions** (true when it fails gracefully):
  1. Creator sees a clear error message if room creation fails
  2. No partial room state is visible to other peers (creation is atomic)
  3. If a join request is denied, the requester sees "Join request denied" and no room data is leaked
- **Invariants** (must remain true throughout):
  1. Only admin(s) can approve join requests
  2. Room messages are only delivered to current members
  3. Plaintext room messages never leave the application boundary — group messages are encrypted per-peer using existing Noise sessions (fan-out encryption, see Implementation Notes)
  4. A peer cannot spoof membership — membership is tracked by the room admin and broadcast to members

## Main Success Scenario

### Part A: Create Room (Primary Actor: Creator)

1. Creator types `/create-room <room-name>` in the input box and presses Enter
2. System validates the room name (non-empty, max 64 characters, no control characters)
3. System generates a unique RoomId (UUID v7)
4. System creates a Room struct with: RoomId, display name, Creator as sole admin and member, creation timestamp
5. System registers the room with the relay server's room registry by sending a `RegisterRoom` message
6. System adds the room to Creator's sidebar conversation list
7. System renders the new room as the active conversation with a system message: "Room '<name>' created. Share the room ID or let peers discover it."

### Part B: Join Room (Primary Actor: Joiner, with Creator as approver)

8. Joiner discovers the room via relay room listing (`ListRooms` → `RoomList` response) or receives the RoomId out-of-band
9. Joiner sends a `JoinRequest` message to the room (routed via relay to the admin)
10. System delivers the JoinRequest to the admin's UI with Joiner's display name and PeerId
11. Admin approves the join request (via `/approve <peer>` or UI action)
12. System adds Joiner to the member list
13. System sends a `MembershipUpdate` (member added) to all current members
14. System sends a `JoinApproved` message to the Joiner with room metadata and current member list
15. Joiner's system adds the room to their sidebar and renders: "Joined room '<name>'"
16. Joiner can now send and receive messages in the room via UC-001/UC-002

## Extensions (What Can Go Wrong)
- **2a. Room name is empty**:
  1. System shows error: "Room name cannot be empty"
  2. Returns to step 1
- **2b. Room name exceeds 64 characters**:
  1. System shows error: "Room name too long (max 64 characters)"
  2. Returns to step 1
- **2c. Room name contains control characters**:
  1. System strips control characters and proceeds with sanitized name
  2. Returns to step 3
- **3a. Creator already has a room with this name locally**:
  1. System shows error: "You already have a room named '<name>'. Choose a different name."
  2. Returns to step 1
- **5a. Relay server is unavailable**:
  1. System creates the room locally (offline-capable)
  2. System shows warning: "Room created locally. Will register when relay is available."
  3. System queues registration for retry when relay reconnects
  4. Continues to step 6 (room is usable for P2P-connected peers)
- **5b. Room name already exists on relay**:
  1. System appends a short random suffix to make the name unique in the registry
  2. System shows: "Room registered as '<name>-abc'"
  3. Returns to step 6
- **6a. Maximum local rooms reached (64 rooms)**:
  1. System shows error: "Maximum rooms reached (64). Leave a room before creating a new one."
  2. Use case fails
- **9a. Joiner sends JoinRequest but admin is offline**:
  1. Relay server queues the JoinRequest (store-and-forward from UC-004)
  2. When admin comes online, JoinRequest is delivered
  3. Returns to step 10
- **9b. Joiner sends JoinRequest to a room that doesn't exist**:
  1. System responds with error: "Room not found"
  2. Use case ends for Joiner
- **10a. Multiple join requests arrive concurrently**:
  1. System queues all pending requests and displays them in order
  2. Admin approves or denies each individually
  3. Returns to step 11 for each request
- **11a. Admin denies the join request**:
  1. System sends `JoinDenied` message to Joiner
  2. Joiner sees: "Join request denied by room admin"
  3. Use case ends for Joiner (they may retry later)
- **11b. Admin is no longer admin (was removed/demoted)**:
  1. System rejects the approve action: "You are not an admin of this room"
  2. Returns to step 10 (another admin must approve)
- **12a. Member list is at capacity (max 256 members)**:
  1. System shows error: "Room is full (max 256 members)"
  2. `JoinDenied` sent to Joiner with reason "room full"
  3. Use case ends for Joiner
- **12b. Joiner is already a member (duplicate join request)**:
  1. System treats as idempotent — no state change
  2. System sends `JoinApproved` to Joiner (re-sync)
  3. Continues to step 14
- **13a. MembershipUpdate fails to reach some members (network error)**:
  1. System queues the update for retry
  2. Members will receive the update when transport is restored
  3. Continues to step 14
- **14a. JoinApproved fails to reach Joiner**:
  1. Relay queues the message (store-and-forward)
  2. Joiner receives it when they reconnect

## Variations
- **1a.** Creator may use a UI button or keyboard shortcut instead of `/create-room` command
- **8a.** Joiner may receive a direct invite link/RoomId from Creator via DM or external channel, bypassing discovery
- **8b.** Joiner may browse a room directory listing from the relay server
- **11c-var.** Admin may configure the room as "auto-approve" — all join requests are automatically approved (no manual gate)

## Out of Scope

The following are NOT part of this use case and will be addressed in future work:
- **Member leave**: Voluntary departure from a room
- **Member kick/ban**: Admin removing a member
- **Admin transfer**: Transferring admin role to another member
- **Room deletion**: Destroying a room and cleaning up state
- **Room editing**: Changing room name or settings after creation

## Agent Execution Notes
- **Verification Command**: `cargo test --test room_management`
- **Test File**: `tests/integration/room_management.rs`
- **Depends On**: UC-001 (Send), UC-002 (Receive), UC-004 (Relay — needed for room registry and store-and-forward of join requests)
- **Blocks**: UC-007 (Agent Join Chat), UC-008 (Share Task List — tasks are room-scoped)
- **Estimated Complexity**: L / ~3000 tokens per agent turn
- **Agent Assignment**: Teammate:Builder (2 builders — room model + relay registry)

## Implementation Notes

### Group Encryption Strategy

Room messages use **fan-out encryption**: the sender encrypts the message once per member using their existing per-peer Noise sessions (from UC-005). This reuses the 1:1 crypto infrastructure — no group key management needed.

- Sender calls `CryptoSession::encrypt()` + `Transport::send()` for each member in the room
- O(N) per message send where N = member count (acceptable for max 256 members)
- Each member decrypts using their own Noise session with the sender
- The relay never sees plaintext — it only forwards per-peer encrypted blobs

### Room Protocol Wire Types

New file: `termchat-proto/src/room.rs`

```rust
/// Messages for room management, bincode-encoded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub enum RoomMessage {
    /// Create a room (sent by creator to relay for registry).
    RegisterRoom { room_id: String, name: String, admin_peer_id: String },

    /// Remove a room from the relay registry.
    UnregisterRoom { room_id: String },

    /// Request a list of available rooms from the relay.
    ListRooms,

    /// Relay responds with the room directory.
    RoomList { rooms: Vec<RoomInfo> },

    /// Peer requests to join a room (routed to admin via relay).
    JoinRequest { room_id: String, peer_id: String, display_name: String },

    /// Admin approves a join request (sent to joiner).
    JoinApproved { room_id: String, name: String, members: Vec<MemberInfo> },

    /// Admin denies a join request (sent to joiner).
    JoinDenied { room_id: String, reason: String },

    /// Broadcast to all members when membership changes.
    MembershipUpdate { room_id: String, action: MemberAction, peer_id: String, display_name: String },
}

/// Summary info for room discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub struct RoomInfo {
    pub room_id: String,
    pub name: String,
    pub member_count: u32,
}

/// Info about a room member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub struct MemberInfo {
    pub peer_id: String,
    pub display_name: String,
    pub is_admin: bool,
}

/// What changed in a membership update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub enum MemberAction {
    Joined,
    Left,
    Promoted,
    Demoted,
}
```

### Relay Server Extensions

The relay server (`termchat-relay`) gets a room registry:
- In-memory `HashMap<String, RoomRegistryEntry>` mapping RoomId to room metadata
- Handles `RegisterRoom`, `UnregisterRoom`, `ListRooms` relay messages
- Routes `JoinRequest` messages to the admin's PeerId (using existing peer routing from UC-004)
- Room entries are ephemeral (lost on relay restart, same as peer registry)

### Client-Side Room State

New file: `termchat/src/chat/room.rs`
- `Room` struct: RoomId, name, members (Vec<MemberInfo>), admin list, creation timestamp
- `RoomManager`: tracks local rooms, handles join request queue, fan-out send logic
- Room's `ConversationId` is derived from its `RoomId` (deterministic mapping)

### New Files

| File | Purpose |
|------|---------|
| `termchat-proto/src/room.rs` | Room protocol wire types |
| `termchat/src/chat/room.rs` | Room state management, RoomManager |
| `termchat-relay/src/rooms.rs` | Relay room registry |
| `tests/integration/room_management.rs` | Integration tests |

### Modified Files

| File | Change |
|------|--------|
| `termchat-proto/src/lib.rs` | Add `pub mod room;` |
| `termchat/src/chat/mod.rs` | Add `pub mod room;` |
| `termchat-relay/src/relay.rs` | Handle room protocol messages |
| `termchat-relay/src/lib.rs` | Add `pub mod rooms;` |
| `termchat/src/app.rs` | Handle `/create-room` and `/approve` commands |

## Acceptance Criteria (for grading)
- [ ] All success postconditions verified by automated test
- [ ] All extension paths have explicit handling
- [ ] No invariant violations detected
- [ ] Code passes lint + clippy (`cargo clippy -- -D warnings`)
- [ ] Reviewer agent approves
- [ ] Room creation produces a valid RoomId and adds to sidebar
- [ ] Room is discoverable via relay room registry (`ListRooms` → `RoomList`)
- [ ] Join request → admin approval → member added flow works end-to-end
- [ ] Join denial sends appropriate feedback to requester
- [ ] Membership updates broadcast to all current members
- [ ] Room works offline (P2P-only, no relay) with degraded discovery
- [ ] Max member limit (256) is enforced
- [ ] Max room limit (64) is enforced
- [ ] Room name validation catches empty, oversized, and control characters
- [ ] Duplicate room name locally is rejected
- [ ] Duplicate join request is handled idempotently
- [ ] Fan-out encryption sends to each member individually (no plaintext on wire)
