# Tasks for UC-007: Join Room as Agent Participant

Generated from use case on 2026-02-07.

## Summary
- **Total tasks**: 20
- **Implementation tasks**: 13
- **Test tasks**: 4
- **Prerequisite tasks**: 2
- **Refactor tasks**: 1
- **Critical path**: T-007-01 → T-007-02 → T-007-03 → T-007-05 → T-007-07 → T-007-09 → T-007-10 → T-007-16 → T-007-20
- **Estimated total size**: XL (collectively ~1800-2500 lines across 2 crates)

## Dependency Graph

```
T-007-01 (Lead: Add deps, wire up modules, stubs)
  ├── T-007-02 (Lead: AgentInfo proto types + MemberInfo is_agent field)
  │     │
  │     ├─── Track A: Bridge & Protocol ───────────────────
  │     │  T-007-03 (Bridge protocol types: AgentMessage + BridgeMessage)
  │     │    ├── T-007-04 (agent_id validation + sanitization: ext 8b)
  │     │    ├── T-007-05 (AgentBridge: Unix socket listener + JSON line I/O)
  │     │       ├── T-007-06 (Connection lifecycle: timeout, multi-connect, stale socket: ext 3a,6a,6b)
  │     │       └── T-007-07 (Hello/Welcome handshake: MSS 7-14, ext 7a,7b,8a,10a,11a)
  │     │             └── T-007-08 (Heartbeat: ping/pong background task, ext 22b)
  │     │
  │     ├─── Track B: Room Integration & Participant ──────
  │     │  T-007-09 (RoomManager: add_member, remove_member, is_agent support)
  │     │    └── T-007-10 (AgentParticipant: send fan-out + receive forwarding)
  │     │          ├── T-007-11 (Send extensions: size limit, room deleted, not ready: ext 16a-c)
  │     │          ├── T-007-12 (Fan-out extensions: transport fail, no Noise session: ext 18a,18b)
  │     │          └── T-007-13 (Disconnect cleanup: graceful + ungraceful: MSS 22-26, ext 20a,22a)
  │     │
  │     └─── Track C: UI & App Integration ────────────────
  │        T-007-14 (App: /invite-agent command handling: MSS 1-2)
  │        T-007-15 (UI: agent badge in sidebar + chat panel: MSS 13,19,25)
  │
  │  After Track A + B complete:
  │  T-007-16 (Lead: Integration build gate — cargo build && cargo test)
  │
  └── T-007-01 also blocks:
        T-007-17 (Reviewer: Stub integration test file)

After all tracks + gate complete:
  T-007-17 → T-007-18 (Reviewer: Bridge lifecycle integration tests)
  T-007-17 → T-007-19 (Reviewer: Agent participation integration tests)
  T-007-18 + T-007-19 → T-007-20 (Reviewer: End-to-end agent room messaging tests)
```

## Tasks

### T-007-01: Add dependencies, wire up modules, create stubs
- **Type**: Prerequisite
- **Module**: `Cargo.toml` (workspace), `termchat/Cargo.toml`, `termchat-proto/src/lib.rs`, `termchat/src/lib.rs`
- **Description**:
  - Add `serde_json` dependency to `termchat/Cargo.toml` (for JSON lines bridge protocol)
  - Add `pub mod agent;` to `termchat/src/lib.rs`
  - Add `pub mod agent;` to `termchat-proto/src/lib.rs`
  - Create stub files: `termchat/src/agent/mod.rs`, `termchat/src/agent/bridge.rs`, `termchat/src/agent/protocol.rs`, `termchat/src/agent/participant.rs`, `termchat-proto/src/agent.rs`
  - Add `[[test]] name = "agent_bridge"` entry to `termchat/Cargo.toml`
  - Create stub `tests/integration/agent_bridge.rs`
  - Verify: `cargo build` succeeds across all crates
- **From**: Implementation Notes (new files + modified files)
- **Depends On**: None
- **Blocks**: T-007-02, T-007-17
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Lead
- **Acceptance Test**: `cargo build` succeeds, `cargo test` still passes (340 tests)

---

### T-007-02: Define AgentInfo proto types and add is_agent to MemberInfo
- **Type**: Implementation
- **Module**: `termchat-proto/src/agent.rs`, `termchat-proto/src/room.rs`
- **Description**:
  - Implement `AgentInfo` struct (`agent_id: String`, `display_name: String`, `capabilities: Vec<AgentCapability>`)
  - Implement `AgentCapability` enum (`Chat` variant, future: `TaskManagement`, `CodeReview`)
  - Add `#[serde(default)] pub is_agent: bool` field to `MemberInfo` in `termchat-proto/src/room.rs`
  - Use `#[serde(default)]` for backward compatibility — existing serialized MemberInfo (without `is_agent`) deserializes as `is_agent = false`
  - Add serde round-trip unit tests for `AgentInfo`, `AgentCapability`
  - Verify all 340 existing tests still pass (is_agent default doesn't break anything)
- **From**: Implementation Notes (proto types), Review action #2
- **Depends On**: T-007-01
- **Blocks**: T-007-03, T-007-09, T-007-14, T-007-15
- **Size**: S
- **Risk**: Medium (wire format change — must verify backward compat)
- **Agent Assignment**: Lead
- **Acceptance Test**: `cargo test -p termchat-proto` passes, existing room tests pass unchanged

---

### T-007-03: Implement bridge protocol types (AgentMessage + BridgeMessage)
- **Type**: Implementation
- **Module**: `termchat/src/agent/protocol.rs`
- **Description**:
  - Define `AgentMessage` enum (JSON, serde-tagged):
    - `Hello { protocol_version: u32, agent_id: String, display_name: String, capabilities: Vec<String> }`
    - `SendMessage { content: String }`
    - `Goodbye`
    - `Pong`
  - Define `BridgeMessage` enum (JSON, serde-tagged):
    - `Welcome { room_id: String, room_name: String, members: Vec<BridgeMemberInfo>, history: Vec<BridgeHistoryEntry> }`
    - `RoomMessage { sender_id: String, sender_name: String, content: String, timestamp: String }`
    - `MembershipUpdate { action: String, peer_id: String, display_name: String, is_agent: bool }`
    - `Error { code: String, message: String }`
    - `Ping`
  - Define helper structs: `BridgeMemberInfo`, `BridgeHistoryEntry`
  - Use `#[serde(tag = "type", rename_all = "snake_case")]` for JSON lines format
  - Add `encode_line()` → `serde_json::to_string() + "\n"` and `decode_line()` → `serde_json::from_str()`
  - Unit tests for serialization round-trips of all variants
- **From**: MSS steps 7, 11, 16, 19-20, 22; Bridge Protocol spec
- **Depends On**: T-007-02
- **Blocks**: T-007-04, T-007-05
- **Size**: M
- **Risk**: Low (straightforward serde_json)
- **Agent Assignment**: Builder-Bridge
- **Acceptance Test**: All protocol types serialize to expected JSON format; round-trip tests pass

---

### T-007-04: Implement agent_id validation and sanitization
- **Type**: Implementation
- **Module**: `termchat/src/agent/protocol.rs`
- **Description**:
  - Implement `validate_agent_id(id: &str) -> Result<String, AgentError>`:
    - Strip control characters and whitespace
    - Truncate to 64 characters
    - If empty after sanitization, return `Err(AgentError::InvalidAgentId)`
  - Implement `make_unique_agent_peer_id(base_id: &str, existing: &[String]) -> String`:
    - Prefix with `agent:`
    - If `agent:<base_id>` already exists, append numeric suffix (`-2`, `-3`, etc.)
  - Unit tests: empty ID, control chars, max length, conflict resolution
- **From**: Extension 8b (agent_id invalid chars), Extension 8a (ID conflicts)
- **Depends On**: T-007-03
- **Blocks**: T-007-07
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Builder-Bridge
- **Acceptance Test**: Validation rejects empty/invalid IDs, conflicts are resolved with suffix

---

### T-007-05: Implement AgentBridge (Unix socket listener + JSON line I/O)
- **Type**: Implementation
- **Module**: `termchat/src/agent/bridge.rs`
- **Description**:
  - Implement `AgentBridge` struct:
    - `start(socket_path: &Path, room_id: &str) -> Result<Self, AgentError>` — creates Unix socket, binds, listens
    - `accept_connection(&self) -> Result<AgentConnection, AgentError>` — accepts one connection
    - `AgentConnection` wrapper: holds `tokio::net::UnixStream` + `BufReader`/`BufWriter`
    - `AgentConnection::read_message(&mut self) -> Result<AgentMessage, AgentError>` — reads one JSON line
    - `AgentConnection::write_message(&mut self, msg: &BridgeMessage) -> Result<(), AgentError>` — writes one JSON line + flush
    - `AgentConnection::close(&mut self)` — graceful shutdown
  - Define `AgentError` in `mod.rs`:
    - `SocketCreationFailed(io::Error)`, `ConnectionClosed`, `InvalidMessage(String)`, `InvalidAgentId(String)`, `ProtocolError(String)`, `RoomNotFound(String)`, `RoomFull`, `Timeout`, `AlreadyConnected`
  - Socket path: `/tmp/termchat-<pid>/agent.sock`
  - Handle stale socket: if path exists, attempt removal before bind (Extension 3a)
  - Create parent directory if missing (Extension 3b)
  - Set socket permissions to owner-only (0o700 on parent dir) (Invariant 2)
  - Unit tests: socket creation, stale socket cleanup, directory creation, message read/write round-trip
- **From**: MSS steps 3-4, 6; Extensions 3a, 3b; Invariants 2, 4
- **Depends On**: T-007-03
- **Blocks**: T-007-06, T-007-07
- **Size**: L
- **Risk**: Medium (Unix socket async I/O, edge cases)
- **Agent Assignment**: Builder-Bridge
- **Acceptance Test**: Socket created, accepts connection, reads/writes JSON lines, cleans up stale sockets

---

### T-007-06: Implement connection lifecycle (timeout, multi-connect, cleanup)
- **Type**: Implementation
- **Module**: `termchat/src/agent/bridge.rs`
- **Description**:
  - Add connection timeout: `accept_connection_with_timeout(duration: Duration)` — wraps accept with `tokio::time::timeout(60s)` (Extension 6a)
  - Handle multiple simultaneous connections: accept first, reject subsequent with `AlreadyConnected` error (Extension 6b)
  - Socket cleanup on timeout: remove socket file if no agent connects
  - Add `shutdown(&mut self)` — closes listener, removes socket file
  - Unit tests: timeout fires, second connection rejected, cleanup on drop
- **From**: Extensions 6a, 6b; MSS step 26
- **Depends On**: T-007-05
- **Blocks**: T-007-16
- **Size**: M
- **Risk**: Medium (concurrent connection handling)
- **Agent Assignment**: Builder-Bridge
- **Acceptance Test**: Timeout produces correct error; second connection gets `already_connected`; socket file cleaned up

---

### T-007-07: Implement Hello/Welcome handshake
- **Type**: Implementation
- **Module**: `termchat/src/agent/bridge.rs`, `termchat/src/agent/participant.rs`
- **Description**:
  - In `AgentBridge` or new `AgentSession` type:
    - After accept, read first message — must be `Hello`
    - Validate: `protocol_version == 1`, `agent_id` non-empty (delegate to `validate_agent_id`)
    - On malformed JSON: send `Error(invalid_hello)`, close connection, return to listening (Extension 7a)
    - On unsupported version: send `Error(unsupported_version)`, close connection (Extension 7b)
    - Generate unique agent PeerId via `make_unique_agent_peer_id` (Extension 8a)
    - Build `Welcome` message:
      - Populate `members` from `RoomManager::get_room_members()`
      - Populate `history` from `MessageStore::get_conversation(room.conversation_id, 50)` (Extension 11a: empty if no history)
      - Include room_id, room_name
    - Send `Welcome` to agent
    - Check room capacity before adding (Extension 10a: send `room_full` error if 256 members)
  - Unit tests: successful handshake, malformed hello, bad version, room full, empty history
- **From**: MSS steps 7-14; Extensions 7a, 7b, 8a, 10a, 11a
- **Depends On**: T-007-04, T-007-05, T-007-09
- **Blocks**: T-007-08, T-007-10
- **Size**: L
- **Risk**: Medium (coordinates across bridge, room manager, and history store)
- **Agent Assignment**: Builder-Bridge
- **Acceptance Test**: Hello→Welcome round-trip works; all error paths return correct error codes

---

### T-007-08: Implement heartbeat (ping/pong background task)
- **Type**: Implementation
- **Module**: `termchat/src/agent/bridge.rs`
- **Description**:
  - Spawn a tokio task after successful handshake that:
    - Every 30 seconds, sends `Ping` to agent via `AgentConnection::write_message()`
    - Expects `Pong` response within 30 seconds
    - If no Pong received within timeout, triggers disconnect cleanup (Extension 22b)
  - Heartbeat task is cancelled when agent disconnects (graceful or ungraceful)
  - Use `tokio::select!` to race ping interval against connection close signal
  - Unit test: pong received resets timer; missing pong triggers disconnect
- **From**: MSS step 15; Extension 22b
- **Depends On**: T-007-07
- **Blocks**: T-007-16
- **Size**: M
- **Risk**: Medium (async task lifecycle, cancellation)
- **Agent Assignment**: Builder-Bridge
- **Acceptance Test**: Heartbeat sends pings at interval; missing pong disconnects agent

---

### T-007-09: Extend RoomManager with add_member, remove_member, is_agent support
- **Type**: Refactor
- **Module**: `termchat/src/chat/room.rs`
- **Description**:
  - Add `add_member(&mut self, room_id: &str, member: MemberInfo) -> Result<Vec<MemberInfo>, RoomError>`:
    - Directly adds a member (bypasses join-request queue — used for agent invites)
    - Checks room capacity (max 256)
    - Checks for duplicate membership (idempotent)
    - Emits `RoomEvent::MemberJoined`
  - Add `remove_member(&mut self, room_id: &str, peer_id: &str) -> Result<MemberInfo, RoomError>`:
    - Removes member from room's member list
    - Returns the removed `MemberInfo`
    - Emits a new `RoomEvent::MemberLeft { room_id, peer_id, display_name }`
    - Returns `RoomError::RoomNotFound` or a new `RoomError::MemberNotFound` if applicable
  - Add `MemberLeft` variant to `RoomEvent`
  - Add `MemberNotFound(String)` variant to `RoomError`
  - Existing `MemberInfo` now has `is_agent` (from T-007-02) — no code changes needed here since field is in proto
  - Unit tests: add_member success, capacity, duplicate, remove_member success, not found, event emission
- **From**: Review action #3 (remove_member), Review action #1 (add_member bypass), MSS steps 10, 23
- **Depends On**: T-007-02
- **Blocks**: T-007-07, T-007-10, T-007-13
- **Size**: M
- **Risk**: Low (extends existing well-tested RoomManager)
- **Agent Assignment**: Builder-Integration
- **Acceptance Test**: `add_member` and `remove_member` work correctly; capacity enforced; events emitted; existing room tests still pass

---

### T-007-10: Implement AgentParticipant (send fan-out + receive forwarding)
- **Type**: Implementation
- **Module**: `termchat/src/agent/participant.rs`
- **Description**:
  - Implement `AgentParticipant` struct — the core adapter between bridge protocol and chat pipeline:
    - Holds: `AgentConnection`, room_id, agent PeerId, reference to `RoomManager`
    - `handle_send_message(content: String)` — MSS step 18 fan-out:
      1. Validate message (non-empty, ≤64KB)
      2. Get room members from RoomManager
      3. For each remote member (skip agent itself), encrypt per-peer and send via Transport
      4. This is the fan-out loop described in the UC review
    - `forward_room_message(sender_id, sender_name, content, timestamp)` — MSS step 20:
      1. Build `BridgeMessage::RoomMessage`
      2. Write to agent connection
    - `forward_membership_update(action, peer_id, display_name, is_agent)` — forward member changes to agent
  - The participant runs an event loop (via `tokio::select!`):
    - Branch 1: Read from agent connection → dispatch (SendMessage → fan-out, Goodbye → cleanup, Pong → heartbeat ack)
    - Branch 2: Receive room events (via mpsc channel) → forward to agent
  - Track agent readiness state: before Welcome is sent, reject SendMessage with `not_ready` error (Extension 16c)
  - Unit tests with LoopbackTransport: send fan-out delivers to all members, receive forwarding works
- **From**: MSS steps 16-21; Integration Notes (ChatManager is peer-scoped)
- **Depends On**: T-007-07, T-007-09
- **Blocks**: T-007-11, T-007-12, T-007-13
- **Size**: XL
- **Risk**: High (core integration point; coordinates bridge, room, crypto, transport)
- **Agent Assignment**: Builder-Integration
- **Acceptance Test**: Agent can send messages that fan out to all room members; agent receives room messages

---

### T-007-11: Implement send extensions (size limit, room deleted, not ready)
- **Type**: Implementation
- **Module**: `termchat/src/agent/participant.rs`
- **Description**:
  - In `handle_send_message()`:
    - Extension 16a: Check content length ≤ 64KB, send `Error(message_too_long)` if exceeded
    - Extension 16b: Check room still exists in RoomManager before fan-out, send `Error(room_not_found)` + close if deleted
    - Extension 16c: Check agent readiness flag, send `Error(not_ready)` if Welcome not yet sent
  - Unit tests: oversized message rejected, deleted room detected, premature send rejected
- **From**: Extensions 16a, 16b, 16c
- **Depends On**: T-007-10
- **Blocks**: T-007-16
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Builder-Integration
- **Acceptance Test**: All three error conditions produce correct error codes

---

### T-007-12: Implement fan-out extensions (transport fail, no Noise session)
- **Type**: Implementation
- **Module**: `termchat/src/agent/participant.rs`
- **Description**:
  - In `handle_send_message()` fan-out loop:
    - Extension 18a: If `Transport::send()` fails for a member, log warning, continue fan-out to remaining members. Queue message for retry (reuse existing offline queue from UC-001)
    - Extension 18b: If no Noise session exists for a member (CryptoSession not established), skip that member with debug log. Queue handshake initiation for later.
  - The fan-out is best-effort per member — partial delivery is acceptable
  - Unit tests: transport failure for one member doesn't block others; missing session skips gracefully
- **From**: Extensions 18a, 18b; Review action #4
- **Depends On**: T-007-10
- **Blocks**: T-007-16
- **Size**: M
- **Risk**: Medium (error handling in async loop, interaction with crypto/transport layers)
- **Agent Assignment**: Builder-Integration
- **Acceptance Test**: Fan-out continues past failed members; no panic on missing Noise sessions

---

### T-007-13: Implement disconnect cleanup (graceful + ungraceful)
- **Type**: Implementation
- **Module**: `termchat/src/agent/participant.rs`, `termchat/src/agent/bridge.rs`
- **Description**:
  - Graceful: Agent sends `Goodbye` (MSS step 22):
    1. Call `RoomManager::remove_member()` to remove agent from room
    2. Build `MembershipUpdate(Left)` and fan-out to remaining room members (via existing broadcast)
    3. Close agent connection
    4. Cancel heartbeat task
    5. Remove socket file
  - Ungraceful: Agent crashes or broken pipe (Extension 22a):
    1. Detect via read error (`UnexpectedEof` or `BrokenPipe`) on `AgentConnection::read_message()`
    2. Same cleanup as graceful path
  - Ungraceful via heartbeat timeout (Extension 22b):
    1. Heartbeat task triggers disconnect
    2. Same cleanup path
  - Extension 20a: Bridge write fails (sending room message to agent) → detect broken pipe → cleanup
  - All cleanup paths converge to a single `cleanup_agent()` method to avoid duplication
  - Unit tests: graceful goodbye cleans up, broken pipe cleans up, both emit MemberLeft event
- **From**: MSS steps 22-26; Extensions 20a, 22a, 22b; Failure Postconditions 3
- **Depends On**: T-007-09, T-007-10
- **Blocks**: T-007-16
- **Size**: M
- **Risk**: Medium (multiple trigger paths must converge cleanly)
- **Agent Assignment**: Builder-Integration
- **Acceptance Test**: Agent removed from member list after disconnect; MembershipUpdate broadcast; socket file cleaned up

---

### T-007-14: Implement /invite-agent command in app.rs
- **Type**: Implementation
- **Module**: `termchat/src/app.rs`
- **Description**:
  - Parse `/invite-agent <room-name>` from input (MSS step 1)
  - Validate room-name:
    - Extension 2a: Room not found → show error "Room '<name>' not found"
    - Extension 2b: Not a member → show error "You are not a member of room '<name>'"
    - Use `RoomManager::get_room_by_name()` for lookup
  - On success: spawn the agent bridge flow (create socket, listen, await connection)
  - Display status message: "Agent bridge listening on <path>. Waiting for agent to connect..." (MSS step 5)
  - Wire agent events (join, leave, messages) into the main event loop
  - This is the entry point that orchestrates T-007-05 through T-007-13
- **From**: MSS steps 1-2, 5; Extensions 2a, 2b
- **Depends On**: T-007-02
- **Blocks**: T-007-16
- **Size**: M
- **Risk**: Medium (event loop integration)
- **Agent Assignment**: Builder-Integration
- **Acceptance Test**: `/invite-agent` with valid room starts bridge; invalid room shows error

---

### T-007-15: Implement agent badge in sidebar and chat panel
- **Type**: Implementation
- **Module**: `termchat/src/ui/sidebar.rs`, `termchat/src/ui/chat_panel.rs`
- **Description**:
  - **Sidebar**: When rendering room member list, check `is_agent` flag on `MemberInfo`. If true, prepend robot indicator to display name (e.g., `[A] Claude` or similar ratatui styling)
  - **Chat panel**: When rendering messages, check if sender PeerId starts with `agent:`. If so, render with distinct style (different color, or `[Agent]` prefix before sender name)
  - Render system messages for agent join/leave: "Agent '<name>' has joined the room" / "Agent '<name>' has left the room" (MSS steps 13, 25)
  - Keep changes minimal — only visual indicators, no structural changes to rendering pipeline
- **From**: MSS steps 13, 19, 25; Postcondition 7; Invariant 3
- **Depends On**: T-007-02
- **Blocks**: T-007-16
- **Size**: S
- **Risk**: Low (cosmetic changes to existing UI code)
- **Agent Assignment**: Builder-Integration
- **Acceptance Test**: Agent members visually distinct in sidebar; agent messages visually distinct in chat

---

### T-007-16: Integration build gate
- **Type**: Prerequisite
- **Module**: Workspace root
- **Description**:
  - Run `cargo fmt` on all crates
  - Run `cargo build` — verify no compilation errors across all 3 crates
  - Run `cargo test` — verify all existing tests still pass (340+)
  - Run `cargo clippy -- -D warnings` — verify no new warnings
  - This is the Phase 2C integration checkpoint from Sprint 5 retrospective
  - Fix any cross-track compilation issues before reviewer proceeds
- **From**: Sprint 5 retrospective action item (Phase 2C gate)
- **Depends On**: T-007-06, T-007-08, T-007-11, T-007-12, T-007-13, T-007-14, T-007-15
- **Blocks**: T-007-18, T-007-19
- **Size**: S
- **Risk**: Low (gate, not implementation)
- **Agent Assignment**: Lead
- **Acceptance Test**: `cargo fmt --check && cargo clippy -- -D warnings && cargo test` all pass

---

### T-007-17: Create stub integration test file
- **Type**: Test
- **Module**: `tests/integration/agent_bridge.rs`
- **Description**:
  - Set up the integration test file with:
    - Imports for agent bridge types, room manager, transport (loopback), crypto (stub)
    - Helper function `setup_agent_bridge()` that creates a room, starts a bridge, returns handles
    - Helper function `connect_mock_agent(socket_path)` that connects a Unix socket client and returns read/write handles
    - Helper function `send_json_line(writer, msg)` / `read_json_line(reader)` for JSON line I/O
    - Placeholder test that compiles: `#[tokio::test] async fn placeholder() {}`
  - Verify `cargo test --test agent_bridge` compiles and placeholder passes
- **From**: Agent Execution Notes (Test File)
- **Depends On**: T-007-01
- **Blocks**: T-007-18, T-007-19
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Reviewer
- **Acceptance Test**: `cargo test --test agent_bridge` compiles and runs

---

### T-007-18: Bridge lifecycle integration tests
- **Type**: Test
- **Module**: `tests/integration/agent_bridge.rs`
- **Description**: Test against postconditions — blind testing, not reading implementation details:
  - **Socket creation**: Bridge creates socket at expected path (Postcondition 1)
  - **Hello → Welcome handshake**: Connect, send Hello, receive Welcome with room info and history (Postconditions 2, 4)
  - **Stale socket cleanup**: Create a file at socket path, then start bridge — should succeed (Extension 3a)
  - **Connection timeout**: Start bridge, don't connect, verify timeout error after 60s (Extension 6a) — use shorter timeout in test
  - **Multiple connection rejection**: Connect two clients, second gets `already_connected` error (Extension 6b)
  - **Malformed Hello**: Send garbage JSON, verify `invalid_hello` error (Extension 7a)
  - **Bad protocol version**: Send Hello with version 99, verify `unsupported_version` error (Extension 7b)
  - **Invalid agent_id**: Send Hello with empty/control-char ID, verify `invalid_agent_id` error (Extension 8b)
  - **Agent ID conflict**: Connect two agents with same ID (sequentially), verify suffix added (Extension 8a)
  - **Room full**: Fill room to 256, then connect agent, verify `room_full` error (Extension 10a)
  - **Graceful disconnect**: Send Goodbye, verify cleanup (Postcondition 9)
  - **Ungraceful disconnect**: Drop connection, verify cleanup (Failure Postcondition 3)
  - **Heartbeat timeout**: Connect, don't respond to Ping, verify disconnect (Extension 22b) — use short interval in test
- **From**: Success Postconditions 1-2, 4, 9; Failure Postconditions 1-3; Extensions 3a, 6a, 6b, 7a, 7b, 8a, 8b, 10a, 22a, 22b
- **Depends On**: T-007-16, T-007-17
- **Blocks**: T-007-20
- **Size**: L
- **Risk**: Medium (async test timing, Unix socket cleanup between tests)
- **Agent Assignment**: Reviewer
- **Acceptance Test**: All lifecycle tests pass

---

### T-007-19: Agent participation integration tests
- **Type**: Test
- **Module**: `tests/integration/agent_bridge.rs`
- **Description**: Test agent message send and receive:
  - **Agent sends message**: Connect agent, send `send_message`, verify message fanned out to room members (Postcondition 5)
  - **Agent receives message**: Room member sends message, verify agent receives `room_message` via bridge (Postcondition 6)
  - **Agent PeerId prefix**: Verify agent's PeerId starts with `agent:` (Invariant 3, Postcondition 3)
  - **MembershipUpdate on join**: Connect agent, verify room members receive MembershipUpdate(Joined) (Postcondition 8)
  - **MembershipUpdate on leave**: Disconnect agent, verify room members receive MembershipUpdate(Left) (Postcondition 9)
  - **Agent in member list**: After join, verify agent appears in room member list with is_agent flag (Postcondition 7)
  - **Message size limit**: Send message >64KB, verify `message_too_long` error (Extension 16a)
  - **Send before Welcome**: Send `send_message` before Hello/Welcome, verify `not_ready` error (Extension 16c)
  - **Empty room history**: Create new room, connect agent, verify Welcome has empty history (Extension 11a)
  - **Room with history**: Send some messages, then connect agent, verify Welcome includes history
- **From**: Success Postconditions 3, 5-8; Invariant 3; Extensions 11a, 16a, 16c
- **Depends On**: T-007-16, T-007-17
- **Blocks**: T-007-20
- **Size**: L
- **Risk**: Medium (fan-out testing requires multi-peer setup)
- **Agent Assignment**: Reviewer
- **Acceptance Test**: All participation tests pass

---

### T-007-20: End-to-end agent room messaging tests
- **Type**: Test
- **Module**: `tests/integration/agent_bridge.rs`
- **Description**: Full-lifecycle integration tests:
  - **Complete lifecycle**: Create room → invite agent → agent joins → agent sends message → room member sends message → agent receives → agent disconnects → verify cleanup. This is the MSS happy path end-to-end.
  - **Agent join + existing member interaction**: Agent joins room with 2 human members. Human A sends message — both Human B and Agent receive it. Agent sends message — both Human A and Human B receive it. Verify fan-out correctness.
  - **Agent disconnect mid-conversation**: Agent sends several messages, then abruptly disconnects. Verify remaining members receive MemberLeft, no orphaned state.
  - **Re-invite after disconnect**: Agent disconnects, then user runs `/invite-agent` again. New agent connects successfully. Verify fresh handshake, no stale state.
- **From**: Full MSS (steps 1-26); Acceptance Criteria (end-to-end)
- **Depends On**: T-007-18, T-007-19
- **Blocks**: None
- **Size**: L
- **Risk**: High (complex multi-actor async scenarios)
- **Agent Assignment**: Reviewer
- **Acceptance Test**: All end-to-end tests pass; `cargo test --test agent_bridge` fully green

---

## Implementation Order

| Order | Task | Type | Size | Depends On | Track |
|-------|------|------|------|------------|-------|
| 1 | T-007-01 | Prerequisite | S | none | Lead |
| 2 | T-007-02 | Implementation | S | T-007-01 | Lead |
| 3a | T-007-03 | Implementation | M | T-007-02 | Bridge |
| 3b | T-007-09 | Refactor | M | T-007-02 | Integration |
| 3c | T-007-17 | Test | S | T-007-01 | Reviewer |
| 4a | T-007-04 | Implementation | S | T-007-03 | Bridge |
| 4b | T-007-05 | Implementation | L | T-007-03 | Bridge |
| 4c | T-007-14 | Implementation | M | T-007-02 | Integration |
| 4d | T-007-15 | Implementation | S | T-007-02 | Integration |
| 5a | T-007-06 | Implementation | M | T-007-05 | Bridge |
| 5b | T-007-07 | Implementation | L | T-007-04, T-007-05, T-007-09 | Bridge |
| 6 | T-007-08 | Implementation | M | T-007-07 | Bridge |
| 7 | T-007-10 | Implementation | XL | T-007-07, T-007-09 | Integration |
| 8a | T-007-11 | Implementation | S | T-007-10 | Integration |
| 8b | T-007-12 | Implementation | M | T-007-10 | Integration |
| 8c | T-007-13 | Implementation | M | T-007-09, T-007-10 | Integration |
| 9 | T-007-16 | Prerequisite | S | T-007-06..T-007-15 | Lead |
| 10a | T-007-18 | Test | L | T-007-16, T-007-17 | Reviewer |
| 10b | T-007-19 | Test | L | T-007-16, T-007-17 | Reviewer |
| 11 | T-007-20 | Test | L | T-007-18, T-007-19 | Reviewer |

## Notes for Agent Team

### Module Ownership (prevent merge conflicts)
- **Lead**: T-007-01, T-007-02, T-007-16 — Root Cargo.toml, `termchat/Cargo.toml`, `termchat-proto/src/agent.rs`, `termchat-proto/src/room.rs` (is_agent field only), `CLAUDE.md`
- **Builder-Bridge**: T-007-03, T-007-04, T-007-05, T-007-06, T-007-07, T-007-08 — `termchat/src/agent/protocol.rs`, `termchat/src/agent/bridge.rs`, `termchat/src/agent/mod.rs`
- **Builder-Integration**: T-007-09, T-007-10, T-007-11, T-007-12, T-007-13, T-007-14, T-007-15 — `termchat/src/agent/participant.rs`, `termchat/src/chat/room.rs`, `termchat/src/app.rs`, `termchat/src/ui/`
- **Reviewer**: T-007-17, T-007-18, T-007-19, T-007-20 — `tests/integration/agent_bridge.rs`

### Coordination Points
1. **T-007-07 is the sync point between tracks** — Builder-Bridge needs RoomManager methods from Builder-Integration's T-007-09 to build the Welcome message. T-007-09 must complete before T-007-07.
2. **Builder-Integration has the heavier load** (7 tasks vs 6 for Builder-Bridge) but several are small (S). Builder-Bridge has the deeper dependency chain. Timing should balance out.
3. **Builders must run `cargo fmt` before marking any task complete** (Sprint 5 retro action item).
4. **Include explicit "claim task #N immediately" in builder spawn prompts** (Sprint 5 retro action item).
5. **T-007-10 (AgentParticipant) is the highest-risk task** — it's the core integration point where bridge, room, crypto, and transport all meet. If it's too large, consider splitting the event loop from the fan-out logic.
6. **Integration tests need test-specific timeouts** — Don't use 60s connection timeout or 30s heartbeat in tests. Make these configurable and use short values (100ms-500ms) in tests.
