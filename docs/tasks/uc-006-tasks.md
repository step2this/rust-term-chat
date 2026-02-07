# Tasks for UC-006: Create Room

Generated from use case on 2026-02-07.

## Summary
- **Total tasks**: 18
- **Implementation tasks**: 12
- **Test tasks**: 4
- **Prerequisite tasks**: 2
- **Critical path**: T-006-01 → T-006-02 → T-006-03 → T-006-05 → T-006-07 → T-006-09 → T-006-10 → T-006-15 → T-006-18
- **Estimated total size**: XL (collectively ~1500-2200 lines across 3 crates)

## Dependency Graph

```
T-006-01 (Lead: Add deps, wire up modules, stubs)
  ├── T-006-02 (Lead: RoomMessage protocol types in termchat-proto)
  │     │
  │     ├─── Track A: Room Client ───────────────────────
  │     │  T-006-03 (Room struct + RoomManager core)
  │     │    ├── T-006-04 (Room name validation + extensions 2a-3a)
  │     │    ├── T-006-05 (Create room flow: MSS steps 1-7)
  │     │    │     └── T-006-07 (Join request handling: MSS steps 9-14)
  │     │    │           ├── T-006-08 (Join extensions: deny, capacity, duplicate)
  │     │    │           └── T-006-09 (Fan-out send + MembershipUpdate broadcast)
  │     │    └── T-006-06 (Room limit + offline creation: ext 5a, 6a)
  │     │
  │     ├─── Track B: Relay Room Registry ──────────────
  │     │  T-006-10 (Relay room registry: RegisterRoom, ListRooms)
  │     │    └── T-006-11 (Relay JoinRequest routing to admin)
  │     │          └── T-006-12 (Relay room registry unit tests)
  │     │
  │     └─── Track C: Proto Tests ──────────────────────
  │        T-006-13 (RoomMessage round-trip unit tests)
  │
  │  After Track A + B complete:
  │  T-006-14 (Wire relay room messages into handle_binary_message)
  │
  └── T-006-01 also blocks:
        T-006-15 (Stub integration test file + Cargo.toml test entry)

After all tracks complete:
  T-006-15 → T-006-16 (Reviewer: Room creation + discovery integration tests)
  T-006-15 → T-006-17 (Reviewer: Join flow integration tests)
  T-006-16 + T-006-17 → T-006-18 (Reviewer: End-to-end room messaging tests)
```

## Tasks

### T-006-01: Add dependencies, wire up modules, create stubs
- **Type**: Prerequisite
- **Module**: `Cargo.toml` (workspace), `termchat/Cargo.toml`, `termchat-proto/src/lib.rs`, `termchat/src/chat/mod.rs`, `termchat-relay/src/lib.rs`
- **Description**:
  - Add `pub mod room;` to `termchat-proto/src/lib.rs`
  - Add `pub mod room;` to `termchat/src/chat/mod.rs`
  - Add `pub mod rooms;` to `termchat-relay/src/lib.rs`
  - Create stub files: `termchat-proto/src/room.rs`, `termchat/src/chat/room.rs`, `termchat-relay/src/rooms.rs`
  - Add `[[test]] name = "room_management"` entry to `termchat/Cargo.toml`
  - Create stub `tests/integration/room_management.rs`
  - Verify: `cargo build` succeeds across all 3 crates
- **From**: Implementation Notes (new files + modified files)
- **Depends On**: None
- **Blocks**: T-006-02, T-006-15
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Lead
- **Acceptance Test**: `cargo build` succeeds

---

### T-006-02: Define RoomMessage protocol types in termchat-proto
- **Type**: Implementation
- **Module**: `termchat-proto/src/room.rs`
- **Description**: Implement the room wire protocol types as specified in Implementation Notes:
  - `RoomMessage` enum with 8 variants: `RegisterRoom`, `UnregisterRoom`, `ListRooms`, `RoomList`, `JoinRequest`, `JoinApproved`, `JoinDenied`, `MembershipUpdate`
  - `RoomInfo` struct (room_id, name, member_count)
  - `MemberInfo` struct (peer_id, display_name, is_admin)
  - `MemberAction` enum (Joined, Left, Promoted, Demoted)
  - `encode()` and `decode()` functions (same pattern as `termchat-proto/src/relay.rs`)
  - All types derive `Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode`
- **From**: MSS Steps 5, 8, 9, 13, 14 + Implementation Notes
- **Depends On**: T-006-01
- **Blocks**: T-006-03, T-006-10, T-006-13
- **Size**: M (100-150 lines)
- **Risk**: Low (follows exact pattern of relay.rs)
- **Agent Assignment**: Lead
- **Acceptance Test**: `cargo test -p termchat-proto -- room` passes

---

### T-006-03: Room struct and RoomManager core
- **Type**: Implementation
- **Module**: `termchat/src/chat/room.rs`
- **Description**: Implement the core room data model:
  - `Room` struct: `room_id: String`, `name: String`, `members: Vec<MemberInfo>`, `created_at: Timestamp`, `conversation_id: ConversationId` (derived deterministically from RoomId)
  - `RoomManager` struct: `rooms: HashMap<String, Room>` (keyed by room_id), max 64 rooms
  - `RoomManager::create_room(name, creator_peer_id, creator_display_name) -> Result<Room, RoomError>`
  - `RoomManager::get_room(room_id) -> Option<&Room>`
  - `RoomManager::list_rooms() -> Vec<&Room>`
  - `RoomManager::room_count() -> usize`
  - `RoomError` enum: `NameEmpty`, `NameTooLong`, `NameContainsControlChars`, `DuplicateName`, `MaxRoomsReached`, `RoomNotFound`, `NotAdmin`, `RoomFull`, `AlreadyMember`
  - Implement `Room::add_member()`, `Room::is_admin()`, `Room::is_member()`, `Room::member_count()`
  - Constants: `MAX_ROOM_NAME_LENGTH = 64`, `MAX_ROOMS = 64`, `MAX_MEMBERS = 256`
- **From**: MSS Steps 3-4, Postconditions 1-3
- **Depends On**: T-006-02
- **Blocks**: T-006-04, T-006-05, T-006-06, T-006-07
- **Size**: L (200-300 lines)
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder-Room
- **Acceptance Test**: Unit tests for Room and RoomManager CRUD operations pass

---

### T-006-04: Room name validation and sanitization
- **Type**: Implementation
- **Module**: `termchat/src/chat/room.rs`
- **Description**: Implement room name validation (extensions 2a-2c, 3a):
  - `validate_room_name(name: &str) -> Result<String, RoomError>`: validates and returns sanitized name
  - Empty check → `RoomError::NameEmpty`
  - Length check (>64 chars) → `RoomError::NameTooLong`
  - Control character stripping (ext 2c): remove chars where `c.is_control()`, return sanitized
  - After sanitization, re-check empty (all control chars)
  - Duplicate local name check in `RoomManager::create_room()` → `RoomError::DuplicateName`
  - Unit tests: empty, too long, control chars, duplicate name, valid name, edge cases
- **From**: Extensions 2a, 2b, 2c, 3a
- **Depends On**: T-006-03
- **Blocks**: T-006-05
- **Size**: S (40-60 lines impl + tests)
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder-Room
- **Acceptance Test**: All name validation unit tests pass

---

### T-006-05: Create room flow (MSS steps 1-7)
- **Type**: Implementation
- **Module**: `termchat/src/chat/room.rs`
- **Description**: Implement the full room creation pipeline:
  - `RoomManager::create_room_full(name, creator_peer_id, display_name, relay_transport) -> Result<Room, RoomError>`:
    1. Validate name (T-006-04)
    2. Check room count limit (ext 6a)
    3. Check duplicate local name (ext 3a)
    4. Generate RoomId (UUID v7)
    5. Create Room struct with creator as admin + member
    6. Send `RegisterRoom` to relay (if available, ext 5a handles offline)
    7. Handle relay response: success, name conflict (ext 5b), unavailable (ext 5a)
    8. Store room in RoomManager
    9. Return the Room
  - `RoomEvent` enum for notifying UI: `RoomCreated { room }`, `JoinRequestReceived { room_id, peer_id, display_name }`, `MemberJoined { room_id, member }`, `JoinDenied { room_id, reason }`, `RoomError { message }`
  - Event channel: `mpsc::Sender<RoomEvent>` in RoomManager
- **From**: MSS Steps 1-7, Extensions 5a, 5b, 6a
- **Depends On**: T-006-03, T-006-04
- **Blocks**: T-006-07, T-006-16
- **Size**: L (200-250 lines)
- **Risk**: Medium (relay interaction for registration)
- **Agent Assignment**: Teammate:Builder-Room
- **Acceptance Test**: Room creation with and without relay succeeds; events emitted

---

### T-006-06: Room limit and offline creation extensions
- **Type**: Implementation
- **Module**: `termchat/src/chat/room.rs`
- **Description**: Implement extension handling in RoomManager:
  - Max room limit (64): `RoomManager::create_room()` checks `rooms.len() >= MAX_ROOMS` → `RoomError::MaxRoomsReached`
  - Offline room creation (ext 5a): if relay registration fails, create room locally and queue registration
  - `PendingRegistration` struct: stores room info for deferred relay registration
  - `RoomManager::flush_pending_registrations(relay) -> Vec<Result>`: retries queued registrations
- **From**: Extensions 5a, 6a
- **Depends On**: T-006-03
- **Blocks**: T-006-16
- **Size**: S (50-80 lines)
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder-Room
- **Acceptance Test**: Room created offline, pending registration stored; room limit enforced

---

### T-006-07: Join request handling (MSS steps 9-14)
- **Type**: Implementation
- **Module**: `termchat/src/chat/room.rs`
- **Description**: Implement the join request → approval → membership flow:
  - `RoomManager::handle_join_request(room_id, peer_id, display_name) -> Result<(), RoomError>`: validates room exists, queues request, emits `JoinRequestReceived` event
  - `RoomManager::approve_join(room_id, peer_id) -> Result<(MemberInfo, Vec<MemberInfo>), RoomError>`: checks admin permission, checks capacity, adds member, returns new member + full member list
  - `RoomManager::deny_join(room_id, peer_id, reason) -> Result<(), RoomError>`: checks admin permission
  - `JoinRequestQueue`: per-room `VecDeque<PendingJoinRequest>` for concurrent requests (ext 10a)
  - `PendingJoinRequest` struct: peer_id, display_name, timestamp
  - `RoomManager::pending_requests(room_id) -> Vec<&PendingJoinRequest>`
- **From**: MSS Steps 9-14, Extensions 10a
- **Depends On**: T-006-05
- **Blocks**: T-006-08, T-006-09, T-006-17
- **Size**: L (200-250 lines)
- **Risk**: Medium (state management for concurrent requests)
- **Agent Assignment**: Teammate:Builder-Room
- **Acceptance Test**: Join request → approve → member added; request queue works

---

### T-006-08: Join extensions (deny, capacity, duplicate, auth)
- **Type**: Implementation
- **Module**: `termchat/src/chat/room.rs`
- **Description**: Implement join-related extension handling:
  - Admin deny (ext 11a): `deny_join()` sends `JoinDenied` reason
  - Not admin (ext 11b): `approve_join()` checks `room.is_admin(approver_peer_id)` → `RoomError::NotAdmin`
  - Capacity limit (ext 12a): `approve_join()` checks `members.len() >= MAX_MEMBERS` → `RoomError::RoomFull`
  - Duplicate join (ext 12b): `approve_join()` checks `room.is_member(peer_id)` → idempotent success, re-send `JoinApproved`
  - Room not found (ext 9b): `handle_join_request()` checks room exists → `RoomError::RoomNotFound`
  - Unit tests for each extension path
- **From**: Extensions 9b, 11a, 11b, 12a, 12b
- **Depends On**: T-006-07
- **Blocks**: T-006-17
- **Size**: M (80-120 lines)
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder-Room
- **Acceptance Test**: All extension paths unit tested

---

### T-006-09: Fan-out send and MembershipUpdate broadcast
- **Type**: Implementation
- **Module**: `termchat/src/chat/room.rs`
- **Description**: Implement room message fan-out:
  - `RoomManager::broadcast_to_room(room_id, message, sender_peer_id, crypto_sessions, transport) -> Vec<Result>`: encrypts and sends to each member except sender
  - Uses existing `CryptoSession::encrypt()` + `Transport::send()` per member (fan-out encryption)
  - `RoomManager::send_membership_update(room_id, action, peer_id, display_name, ...)`: broadcasts `MembershipUpdate` to all current members (ext 13a: queue on failure)
  - `RoomManager::send_join_approved(peer_id, room, transport)`: sends `JoinApproved` with room metadata + member list to joiner (ext 14a: relay queues on failure)
- **From**: MSS Steps 13-14, Postcondition 6, Invariant 3, Extensions 13a, 14a
- **Depends On**: T-006-07
- **Blocks**: T-006-18
- **Size**: M (100-150 lines)
- **Risk**: Medium (fan-out across multiple transports, error handling per-member)
- **Agent Assignment**: Teammate:Builder-Room
- **Acceptance Test**: Message sent to room reaches all members; failed sends queued

---

### T-006-10: Relay room registry (RegisterRoom, ListRooms, UnregisterRoom)
- **Type**: Implementation
- **Module**: `termchat-relay/src/rooms.rs`
- **Description**: Implement the relay server's room registry:
  - `RoomRegistry` struct: `RwLock<HashMap<String, RoomRegistryEntry>>` mapping room_id to metadata
  - `RoomRegistryEntry` struct: room_id, name, admin_peer_id, member_count
  - `RoomRegistry::register(room_id, name, admin_peer_id) -> Result<(), RegistryError>`: adds room, checks name uniqueness
  - `RoomRegistry::unregister(room_id)`: removes room
  - `RoomRegistry::list() -> Vec<RoomInfo>`: returns all registered rooms
  - `RoomRegistry::get_admin(room_id) -> Option<String>`: returns admin PeerId for routing join requests
  - `RegistryError::NameConflict`: relay name already taken (ext 5b)
  - Max 1000 rooms on relay server (prevent abuse)
- **From**: MSS Step 5, Extensions 5b, Implementation Notes (Relay Server Extensions)
- **Depends On**: T-006-02
- **Blocks**: T-006-11, T-006-12, T-006-14
- **Size**: M (100-150 lines)
- **Risk**: Low (follows RelayState pattern)
- **Agent Assignment**: Teammate:Builder-Relay
- **Acceptance Test**: Register, list, unregister rooms on relay; name conflict detected

---

### T-006-11: Relay JoinRequest routing to admin
- **Type**: Implementation
- **Module**: `termchat-relay/src/rooms.rs`
- **Description**: Implement relay-side join request routing:
  - When relay receives a `JoinRequest`, look up room's admin PeerId in registry
  - Route the `JoinRequest` to the admin's WebSocket connection (using existing peer routing from UC-004)
  - If admin is offline, queue via store-and-forward (existing MessageStore from UC-004)
  - If room not found, send `JoinDenied { reason: "Room not found" }` back to requester
  - Handle `JoinApproved` and `JoinDenied` routing: forward from admin to joiner's PeerId
  - Handle `MembershipUpdate` routing: admin broadcasts, relay forwards to each member PeerId
- **From**: MSS Steps 9-10, Extensions 9a, 9b
- **Depends On**: T-006-10
- **Blocks**: T-006-12, T-006-14
- **Size**: M (100-150 lines)
- **Risk**: Medium (routing logic, integration with existing handle_binary_message)
- **Agent Assignment**: Teammate:Builder-Relay
- **Acceptance Test**: JoinRequest routed to admin; offline admin gets queued message

---

### T-006-12: Relay room registry unit tests
- **Type**: Test
- **Module**: `termchat-relay/src/rooms.rs` (inline `#[cfg(test)]`)
- **Description**: Unit tests for the room registry and routing:
  - Register a room, verify it appears in list
  - Unregister a room, verify removed
  - Name conflict detection
  - Max registry capacity (1000)
  - JoinRequest routing to admin PeerId
  - JoinRequest routing when admin offline (queued)
  - Room not found error for unknown room_id
  - JoinApproved/JoinDenied forwarding
- **From**: Postconditions 4-5, Extensions 5b, 9a, 9b
- **Depends On**: T-006-10, T-006-11
- **Blocks**: T-006-14
- **Size**: M (100-150 lines)
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder-Relay
- **Acceptance Test**: All relay room unit tests pass

---

### T-006-13: RoomMessage round-trip unit tests
- **Type**: Test
- **Module**: `termchat-proto/src/room.rs` (inline `#[cfg(test)]`)
- **Description**: Bincode encode/decode round-trip tests for all RoomMessage variants:
  - RegisterRoom, UnregisterRoom, ListRooms, RoomList, JoinRequest, JoinApproved, JoinDenied, MembershipUpdate
  - Edge cases: empty room list, empty members, long names
  - Corrupted bytes decode failure
  - Empty bytes decode failure
- **From**: Postcondition 3 (serializable)
- **Depends On**: T-006-02
- **Blocks**: None
- **Size**: S (60-80 lines)
- **Risk**: Low
- **Agent Assignment**: Lead (can run in parallel as T-006-02 follow-up)
- **Acceptance Test**: `cargo test -p termchat-proto -- room` passes

---

### T-006-14: Wire relay room messages into handle_binary_message
- **Type**: Implementation
- **Module**: `termchat-relay/src/relay.rs`
- **Description**: Extend the relay server's `handle_binary_message()` to handle `RoomMessage` variants alongside existing `RelayMessage` variants:
  - Try decoding as `RelayMessage` first (existing), then as `RoomMessage` (new)
  - OR: extend `RelayMessage` with a `RoomOp(RoomMessage)` variant to keep single enum dispatch
  - Decision: **use a wrapper variant** `RelayMessage::Room(Vec<u8>)` that carries bincode-encoded `RoomMessage` bytes, decoded by the room handler. This avoids modifying the existing RelayMessage enum shape.
  - Route decoded `RoomMessage` to `RoomRegistry` methods
  - Add `RoomRegistry` as a field of `RelayState`
- **From**: Implementation Notes (Relay Server Extensions, Modified Files)
- **Depends On**: T-006-10, T-006-11, T-006-12
- **Blocks**: T-006-16
- **Size**: M (80-120 lines)
- **Risk**: Medium (modifying existing relay code, must not break UC-004 tests)
- **Agent Assignment**: Teammate:Builder-Relay
- **Acceptance Test**: `cargo test -p termchat-relay` passes (existing + new); room messages routed correctly

---

### T-006-15: Stub integration test file and test entry
- **Type**: Prerequisite
- **Module**: `tests/integration/room_management.rs`, `termchat/Cargo.toml`
- **Description**: Create the integration test file with helpers:
  - `start_relay()` helper (reuse pattern from relay_fallback.rs)
  - `create_relay_transport()` helper
  - Skeleton test functions with `#[tokio::test]` for reviewer to fill in
  - Verify: `cargo test --test room_management` compiles (tests may be empty/ignored)
- **From**: Agent Execution Notes (Test File)
- **Depends On**: T-006-01
- **Blocks**: T-006-16, T-006-17, T-006-18
- **Size**: S (40-60 lines skeleton)
- **Risk**: Low
- **Agent Assignment**: Lead
- **Acceptance Test**: `cargo test --test room_management` compiles

---

### T-006-16: Integration tests — room creation and discovery
- **Type**: Test
- **Module**: `tests/integration/room_management.rs`
- **Description**: Integration tests for Part A (Create Room) + relay discovery:
  - Create room via RoomManager, verify RoomId and admin set
  - Room appears in local room list
  - Room registered on relay (RegisterRoom → relay has it)
  - ListRooms from another peer returns the room
  - Offline room creation (relay unavailable), room works locally
  - Deferred registration when relay reconnects
  - Room name validation: empty, too long, control chars, duplicate
  - Room limit (64) enforced
  - Relay name conflict appends suffix
- **From**: Postconditions 1-4, Extensions 2a-3a, 5a-5b, 6a, Acceptance Criteria
- **Depends On**: T-006-05, T-006-06, T-006-14, T-006-15
- **Blocks**: T-006-18
- **Size**: L (200-250 lines)
- **Risk**: Low
- **Agent Assignment**: Teammate:Reviewer
- **Acceptance Test**: All room creation integration tests pass

---

### T-006-17: Integration tests — join flow
- **Type**: Test
- **Module**: `tests/integration/room_management.rs`
- **Description**: Integration tests for Part B (Join Room):
  - JoinRequest sent, admin receives it
  - Admin approves, joiner receives JoinApproved with member list
  - Admin denies, joiner receives JoinDenied
  - Room capacity limit (256) enforced on approve
  - Duplicate join request handled idempotently
  - Not-admin cannot approve
  - Concurrent join requests queued and processed individually
  - Admin offline: join request queued via relay, delivered when admin reconnects
  - Room not found: appropriate error
- **From**: Postconditions 5-7, Extensions 9a-12b, Acceptance Criteria
- **Depends On**: T-006-07, T-006-08, T-006-14, T-006-15
- **Blocks**: T-006-18
- **Size**: L (200-250 lines)
- **Risk**: Medium (multi-peer coordination in tests)
- **Agent Assignment**: Teammate:Reviewer
- **Acceptance Test**: All join flow integration tests pass

---

### T-006-18: Integration tests — end-to-end room messaging
- **Type**: Test
- **Module**: `tests/integration/room_management.rs`
- **Description**: End-to-end tests verifying rooms work with existing UC-001/UC-002 pipeline:
  - Create room, add 2 members, send message → both members receive
  - Fan-out encryption: wire capture shows each member gets individually encrypted payload
  - 3-member room: message from A received by B and C but not A
  - MembershipUpdate received by all existing members when new member joins
  - Room message uses correct ConversationId (derived from RoomId)
- **From**: Postcondition 6, Invariant 2-3, Acceptance Criteria (fan-out, broadcast)
- **Depends On**: T-006-09, T-006-16, T-006-17
- **Blocks**: None
- **Size**: M (100-150 lines)
- **Risk**: Medium (complex multi-peer setup)
- **Agent Assignment**: Teammate:Reviewer
- **Acceptance Test**: `cargo test --test room_management` — all tests pass

---

## Implementation Order

| Order | Task | Type | Size | Track | Depends On |
|-------|------|------|------|-------|------------|
| 1 | T-006-01 | Prerequisite | S | Lead | none |
| 2 | T-006-02 | Implementation | M | Lead | T-006-01 |
| 3 | T-006-15 | Prerequisite | S | Lead | T-006-01 |
| 4 | T-006-13 | Test | S | Lead | T-006-02 |
| 5 | T-006-03 | Implementation | L | Track A | T-006-02 |
| 5 | T-006-10 | Implementation | M | Track B | T-006-02 |
| 6 | T-006-04 | Implementation | S | Track A | T-006-03 |
| 6 | T-006-11 | Implementation | M | Track B | T-006-10 |
| 7 | T-006-05 | Implementation | L | Track A | T-006-03, T-006-04 |
| 7 | T-006-06 | Implementation | S | Track A | T-006-03 |
| 7 | T-006-12 | Test | M | Track B | T-006-10, T-006-11 |
| 8 | T-006-07 | Implementation | L | Track A | T-006-05 |
| 8 | T-006-14 | Implementation | M | Track B | T-006-10, T-006-11, T-006-12 |
| 9 | T-006-08 | Implementation | M | Track A | T-006-07 |
| 9 | T-006-09 | Implementation | M | Track A | T-006-07 |
| 10 | T-006-16 | Test | L | Reviewer | T-006-05, T-006-06, T-006-14, T-006-15 |
| 10 | T-006-17 | Test | L | Reviewer | T-006-07, T-006-08, T-006-14, T-006-15 |
| 11 | T-006-18 | Test | M | Reviewer | T-006-09, T-006-16, T-006-17 |

## Notes for Agent Team

### File Ownership (strict — no cross-track edits)
- **Lead**: All `Cargo.toml` files, `*/lib.rs` module declarations, `termchat-proto/src/room.rs`
- **Builder-Room (Track A)**: `termchat/src/chat/room.rs` — sole owner
- **Builder-Relay (Track B)**: `termchat-relay/src/rooms.rs`, `termchat-relay/src/relay.rs` (room message handling only)
- **Reviewer**: `tests/integration/room_management.rs` — sole owner

### Coordination Points
1. **T-006-02 is the shared contract**: Both tracks code against `RoomMessage` types. Lead must complete this before spawning builders.
2. **T-006-14 touches existing relay code**: Builder-Relay modifies `relay.rs` — must NOT break existing UC-004 relay tests. Run `cargo test -p termchat-relay` as gate before proceeding.
3. **Fan-out in T-006-09 needs Transport trait access**: Builder-Room will need to make `RoomManager` generic over `Transport` (or accept `&dyn Transport`). Coordinate with existing `ChatManager` pattern.
4. **ConversationId derivation**: `Room::conversation_id` should deterministically derive from `RoomId` (e.g., `ConversationId::from_uuid(Uuid::parse_str(&room_id))`). Define this in T-006-03 so it's consistent.

### Review Gates
- **Gate 1**: After T-006-02 + T-006-13: `cargo test -p termchat-proto` (proto types + round-trip tests)
- **Gate 2**: After Track A completes (T-006-03 through T-006-09): `cargo test -p termchat -- room` (room manager unit tests)
- **Gate 3**: After Track B completes (T-006-10 through T-006-14): `cargo test -p termchat-relay` (relay room registry + existing relay tests)
- **Gate 4**: After all tests (T-006-16 through T-006-18): `cargo fmt --check && cargo clippy -- -D warnings && cargo test` (full quality gate)
