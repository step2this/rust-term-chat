# Use Case: UC-007 Join Room as Agent Participant

## Classification
- **Goal Level**: 🌊 User Goal
- **Scope**: System (black box)
- **Priority**: P2 Medium
- **Complexity**: 🔴 High

## Actors
- **Primary Actor**: Human User (terminal user who invites an agent into a room)
- **Supporting Actors**: Agent Process (external Claude Code agent or compatible process that connects via Unix socket), Transport Layer, Crypto Layer, Relay Server, Room Members (existing peers in the room)
- **Stakeholders & Interests**:
  - Human User: wants an AI agent to participate in a room conversation, providing assistance, answering questions, or performing tasks — with minimal setup friction
  - Agent Process: needs a clear protocol to connect, receive context, and exchange messages without managing encryption or transport details
  - Room Members: want to know when an agent is present (transparency), expect agent messages to be delivered like any other member's messages
  - System: agent participation must not bypass encryption invariants — agent messages are encrypted per-peer when fanned out to remote members, same as human messages
  - Security: the Unix socket must only accept local connections; agent identity must be clearly distinguishable from human peers

## Conditions
- **Preconditions** (must be true before starting):
  1. Human User has a valid identity keypair (from UC-005)
  2. At least one room exists that the Human User is a member of (from UC-006)
  3. At least one transport (P2P or relay) is available (from UC-003/UC-004)
  4. TUI is running and responsive (from Phase 1)
  5. An agent process is running externally and can connect to a Unix domain socket
- **Success Postconditions** (true when done right):
  1. A Unix domain socket is created and listening at a well-known path (e.g., `$XDG_RUNTIME_DIR/termchat/agent.sock` or `/tmp/termchat-<pid>/agent.sock`)
  2. The agent has connected to the socket and completed the bridge handshake (protocol version, agent identity, capabilities)
  3. The agent is registered as a member of the specified room with a distinct agent PeerId (prefixed `agent:`)
  4. The agent received recent message history (last 50 messages) from the room as initial context
  5. The agent can send messages to the room via the bridge, and those messages are delivered to all room members (encrypted per-peer for remote members)
  6. The agent receives new messages from the room in real-time via the bridge
  7. The agent appears in the room's member list with an agent badge/indicator visible to all members
  8. A `MembershipUpdate` (agent joined) was broadcast to all room members
  9. When the agent disconnects, a `MembershipUpdate` (agent left) is broadcast and the agent is removed from the member list
- **Failure Postconditions** (true when it fails gracefully):
  1. If the socket cannot be created, the Human User sees a clear error message
  2. If the agent fails to connect or handshake, no partial agent state is visible to room members
  3. If the agent crashes or disconnects unexpectedly, the system detects the broken connection and cleans up (removes from member list, broadcasts departure)
  4. If the specified room doesn't exist, the agent receives an error response over the bridge protocol
- **Invariants** (must remain true throughout):
  1. Agent messages entering the room are encrypted per-peer (via existing Noise sessions) before being sent to remote members — the agent bridge is local-only and unencrypted, but the outbound path is always encrypted
  2. The Unix socket only accepts connections from the local machine (filesystem permissions, no network exposure)
  3. Agent identity is always distinguishable from human peers (agent PeerId uses `agent:` prefix)
  4. The bridge protocol is JSON lines over the Unix socket — one JSON object per newline-terminated line

## Main Success Scenario

### Part A: Start Agent Bridge (Primary Actor: Human User)

1. Human User types `/invite-agent <room-name>` in the input box and presses Enter
2. System validates that `<room-name>` matches an existing room the user is a member of
3. System creates a Unix domain socket at a well-known path (e.g., `/tmp/termchat-<pid>/agent.sock`)
4. System begins listening for incoming connections on the socket
5. System renders a status message: "Agent bridge listening on `<socket-path>`. Waiting for agent to connect..."

### Part B: Agent Connects and Joins (Supporting Actor: Agent Process)

6. Agent process connects to the Unix domain socket
7. Agent sends a `Hello` message over the bridge: `{"type": "hello", "protocol_version": 1, "agent_id": "<unique-id>", "display_name": "<name>", "capabilities": ["chat"]}`
8. System validates the `Hello` message (protocol version supported, agent_id non-empty)
9. System assigns the agent a PeerId with `agent:` prefix (e.g., `agent:<agent_id>`)
10. System adds the agent to the room's member list as a non-admin member with agent flag
11. System sends a `Welcome` response to the agent: `{"type": "welcome", "room_id": "<id>", "room_name": "<name>", "members": [...], "history": [<last 50 messages>]}`
12. System broadcasts a `MembershipUpdate` (agent joined) to all room members
13. System renders in the room: "🤖 Agent '<name>' has joined the room"
14. Agent processes the `Welcome` message and is now ready to participate
15. System starts a background heartbeat task that sends `{"type": "ping"}` to the agent every 30 seconds and expects a `{"type": "pong"}` response within 30 seconds

### Part C: Agent Participates in Room

16. Agent sends a message via the bridge: `{"type": "send_message", "content": "<text>"}`
17. System validates the message (non-empty, within size limit)
18. System injects the message into the room's fan-out pipeline: `AgentParticipant` iterates room members, calling per-peer encrypt (via existing Noise sessions) and send (via Transport) for each remote member
19. Remote room members receive and display the message with the agent badge indicator
20. When a room member sends a message, System forwards it to the agent via the bridge: `{"type": "room_message", "sender_id": "<peer_id>", "sender_name": "<name>", "content": "<text>", "timestamp": "<iso8601>"}`
21. Agent receives and processes the message

### Part D: Agent Disconnects

22. Agent sends a `Goodbye` message: `{"type": "goodbye"}` (or the socket connection closes)
23. System removes the agent from the room's member list
24. System broadcasts a `MembershipUpdate` (agent left) to all room members
25. System renders in the room: "🤖 Agent '<name>' has left the room"
26. System closes the Unix socket listener (or keeps it open for future agent connections, based on configuration)

## Extensions (What Can Go Wrong)

- **2a. Room name doesn't match any existing room**:
  1. System shows error: "Room '<name>' not found"
  2. Returns to step 1
- **2b. Human User is not a member of the specified room**:
  1. System shows error: "You are not a member of room '<name>'"
  2. Returns to step 1
- **3a. Socket path already exists (stale socket from previous session)**:
  1. System removes the stale socket file
  2. System creates a new socket at the same path
  3. Returns to step 4
- **3b. Socket creation fails (permission denied, directory doesn't exist)**:
  1. System creates the parent directory if missing
  2. If still fails, shows error: "Cannot create agent socket: <reason>"
  3. Use case fails
- **6a. No agent connects within timeout (60 seconds)**:
  1. System shows: "No agent connected within timeout. Socket closed."
  2. System removes the socket file
  3. Use case fails (user can retry with `/invite-agent`)
- **6b. Multiple agents try to connect simultaneously**:
  1. System accepts the first connection and rejects subsequent ones with an error: `{"type": "error", "code": "already_connected", "message": "An agent is already connected to this room"}`
  2. First agent continues at step 7
- **7a. Agent sends malformed JSON or missing required fields**:
  1. System sends error: `{"type": "error", "code": "invalid_hello", "message": "Malformed Hello message: <details>"}`
  2. System closes the connection
  3. Returns to step 4 (listening for a new agent)
- **7b. Agent sends unsupported protocol version**:
  1. System sends error: `{"type": "error", "code": "unsupported_version", "message": "Supported versions: [1]"}`
  2. System closes the connection
  3. Returns to step 4
- **8a. Agent ID conflicts with an existing agent in the room**:
  1. System appends a numeric suffix to make the PeerId unique (e.g., `agent:claude-1` → `agent:claude-2`)
  2. Returns to step 9
- **8b. Agent ID contains invalid characters (whitespace, control chars, or exceeds 64 chars)**:
  1. System sanitizes the agent_id: strips control characters and whitespace, truncates to 64 characters
  2. If sanitized ID is empty, sends error: `{"type": "error", "code": "invalid_agent_id", "message": "Agent ID is empty or contains only invalid characters"}`
  3. If empty: system closes connection, returns to step 4
  4. Otherwise: continues to step 9 with sanitized ID
- **10a. Room member capacity reached (max 256 members)**:
  1. System sends error to agent: `{"type": "error", "code": "room_full", "message": "Room is at capacity (256 members)"}`
  2. System closes the connection
  3. System shows to Human User: "Cannot add agent — room is full"
  4. Use case fails
- **11a. Room has no message history (newly created, empty room)**:
  1. System sends `Welcome` with an empty `history` array
  2. Continues to step 12
- **12a. MembershipUpdate broadcast fails for some remote members**:
  1. System queues the update for retry (same as UC-006 extension 13a)
  2. Members will receive the update when transport is restored
  3. Continues to step 13
- **16a. Agent sends message exceeding size limit (64KB)**:
  1. System sends error to agent: `{"type": "error", "code": "message_too_long", "message": "Message exceeds 64KB limit"}`
  2. Agent can retry with a shorter message
- **16b. Agent sends message while room no longer exists (was deleted)**:
  1. System sends error to agent: `{"type": "error", "code": "room_not_found", "message": "Room no longer exists"}`
  2. System closes the connection
  3. Agent is removed from the room
- **16c. Agent sends message before processing Welcome (race condition)**:
  1. System sends error: `{"type": "error", "code": "not_ready", "message": "Agent must wait for Welcome before sending messages"}`
  2. Agent should wait for Welcome, then retry
- **18a. Transport fails for one or more remote members**:
  1. System queues the message for retry (same as UC-001 extension 5b — offline queue)
  2. Continues to step 19 for reachable members
- **18b. No Noise session exists with one or more remote room members**:
  1. System skips those members for this message with a debug log warning
  2. System queues a Noise handshake initiation (UC-005) for the unconnected peers
  3. Future messages will be delivered once sessions are established
  4. Continues to step 19 for members with active sessions
- **20a. Bridge socket write fails (agent process crashed)**:
  1. System detects the broken pipe / connection reset
  2. Jumps to step 23 (cleanup — remove agent, broadcast departure)
- **22a. Agent disconnects without sending Goodbye (crash, kill signal)**:
  1. System detects the closed socket via read error
  2. Proceeds to step 23 (same cleanup path)
- **22b. Socket connection becomes idle for extended period (no heartbeat)**:
  1. System sends a `Ping` message: `{"type": "ping"}`
  2. If no `Pong` response within 30 seconds, system treats agent as disconnected
  3. Proceeds to step 23

## Variations
- **1a.** Human User may use `/agent-bridge` or a keyboard shortcut instead of `/invite-agent`
- **5a.** System may print the socket path to a well-known location (e.g., a file) so automated scripts can discover it
- **11a.** History depth may be configurable (default 50, overridable via `/invite-agent --context 100`)
- **16c-var.** Agent may send structured content (e.g., code blocks, task updates) — the bridge protocol supports a `content_type` field for future extension
- **26a.** System keeps the socket open after agent disconnects, accepting a new agent connection (persistent bridge mode)

## Out of Scope

The following are NOT part of this use case and will be addressed in future work:
- **Agent-specific slash commands** (`/ask-agent`, `/agent-status`): Will be a separate UC or enhancement
- **Multiple simultaneous agents per room**: UC-007 supports one agent per bridge invocation. Multiple agents would require multiple `/invite-agent` calls (one per agent), each creating its own socket
- **Agent spawning by TermChat**: TermChat does not launch agent processes — agents are external. Child process management may be added later
- **Agent-to-agent communication**: Agents in the same room communicate via room messages like any member
- **CRDT task synchronization**: Part of UC-008

## Agent Execution Notes
- **Verification Command**: `cargo test --test agent_bridge`
- **Test File**: `tests/integration/agent_bridge.rs`
- **Depends On**: UC-001 (Send), UC-002 (Receive), UC-005 (E2E Handshake — agent messages encrypted per-peer for remote delivery), UC-006 (Rooms — agent joins a room)
- **Blocks**: UC-008 (Shared Task List — agents need to participate to coordinate tasks)
- **Estimated Complexity**: L / ~3000 tokens per agent turn
- **Agent Assignment**: Teammate:Builder (2 builders — agent bridge/protocol + client integration/UI)

## Implementation Notes

### Bridge Protocol (JSON Lines over Unix Socket)

Each message is a single JSON object followed by `\n`. The protocol is asymmetric: the agent sends **Agent Messages** and the system sends **Bridge Messages**.

#### Agent → System Messages

```json
{"type": "hello", "protocol_version": 1, "agent_id": "claude-opus", "display_name": "Claude", "capabilities": ["chat"]}
{"type": "send_message", "content": "Hello, I'm here to help!"}
{"type": "goodbye"}
{"type": "pong"}
```

#### System → Agent Messages

```json
{"type": "welcome", "room_id": "uuid", "room_name": "dev-team", "members": [{"peer_id": "abc", "display_name": "Alice", "is_admin": true, "is_agent": false}], "history": [{"sender_id": "abc", "sender_name": "Alice", "content": "Hello", "timestamp": "2026-02-07T12:00:00Z"}]}
{"type": "room_message", "sender_id": "abc", "sender_name": "Alice", "content": "Can you review this?", "timestamp": "2026-02-07T12:01:00Z"}
{"type": "membership_update", "action": "joined", "peer_id": "def", "display_name": "Bob", "is_agent": false}
{"type": "error", "code": "invalid_hello", "message": "Missing required field: agent_id"}
{"type": "ping"}
```

### New Module: `termchat/src/agent/`

| File | Purpose |
|------|---------|
| `termchat/src/agent/mod.rs` | Module root, AgentError type, public API |
| `termchat/src/agent/bridge.rs` | AgentBridge: Unix socket listener, connection management, JSON line read/write, bridge lifecycle |
| `termchat/src/agent/protocol.rs` | AgentMessage and BridgeMessage enums (serde), protocol version negotiation |
| `termchat/src/agent/participant.rs` | AgentParticipant: adapts bridge messages to/from the chat pipeline (ChatManager integration), handles room context injection |

### New Proto Types: `termchat-proto/src/agent.rs`

Shared agent identity types used by both client and (potentially) relay:

```rust
/// Identifies an agent participant in the system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInfo {
    pub agent_id: String,
    pub display_name: String,
    pub capabilities: Vec<AgentCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentCapability {
    Chat,
    // Future: TaskManagement, CodeReview, etc.
}
```

### Modified Files

| File | Change |
|------|--------|
| `termchat/src/lib.rs` | Add `pub mod agent;` |
| `termchat/src/app.rs` | Handle `/invite-agent` command, wire agent events into event loop |
| `termchat/src/chat/mod.rs` | Extend ChatManager to accept messages from agent bridge (inject into send pipeline) |
| `termchat/src/chat/room.rs` | Extend Room/RoomManager: add `remove_member()` method, add `is_agent` flag tracking, add `get_room_by_name()` lookup for `/invite-agent` validation |
| `termchat/src/ui/sidebar.rs` | Render agent members with badge indicator |
| `termchat/src/ui/chat_panel.rs` | Render agent messages with visual distinction |
| `termchat-proto/src/lib.rs` | Add `pub mod agent;` |
| `termchat-proto/src/room.rs` | Add `is_agent: bool` field to `MemberInfo` with `#[serde(default)]` for backward compatibility with existing serialized data |

### Integration with Existing Systems

- **Chat Pipeline (IMPORTANT — `ChatManager` is peer-scoped)**: The current `ChatManager<C, T, S>` holds a single `peer_id` and encrypts/sends to one peer. There is no `send_to_room()` method. Agent messages must be fanned out at the `AgentParticipant` level: iterate room members from `RoomManager`, skip the agent itself, and for each remote member call the per-peer `ChatManager::send_message()` (or directly encrypt + transport send). This matches UC-006's fan-out approach where the application layer orchestrates per-peer delivery. The `AgentParticipant` owns this fan-out loop.
- **Room Membership**: Agents are added to the room via `RoomManager::approve_join()` (or a new direct `add_member()` method that bypasses the join-request queue, since agents are invited by the room admin directly). A new `RoomManager::remove_member()` method is needed for agent departure cleanup (step 23). The existing `MembershipUpdate` broadcast mechanism handles notifying all members.
- **Message History**: The `Welcome` message includes recent history from the in-memory `MessageStore` via `get_conversation()` keyed by the room's `ConversationId`. **Note**: Since SQLite persistence is not yet implemented, message history is only available for the current session. Messages from before the TUI process started are not available. The history array may be empty for newly created rooms.
- **Heartbeat**: A background tokio task sends `Ping` every 30 seconds. If no `Pong` within 30 seconds, the agent is considered disconnected.
- **Wire Format Compatibility**: Adding `is_agent: bool` to `MemberInfo` in `termchat-proto/src/room.rs` is a breaking change to the postcard wire format. Use `#[serde(default)]` on the field so that existing serialized `MemberInfo` (without `is_agent`) deserializes with `is_agent = false`. All existing tests will continue to pass without modification.

## Acceptance Criteria (for grading)
- [ ] All success postconditions verified by automated test
- [ ] All extension paths have explicit handling
- [ ] No invariant violations detected
- [ ] Code passes lint + clippy (`cargo clippy -- -D warnings`)
- [ ] Reviewer agent approves
- [ ] Unix domain socket is created and agent can connect via bridge protocol
- [ ] Agent Hello → Welcome handshake completes successfully
- [ ] Agent receives recent message history (last 50 messages) on join
- [ ] Agent can send messages to room and all members receive them
- [ ] Agent receives real-time messages from room members
- [ ] Agent appears in room member list with agent badge
- [ ] MembershipUpdate broadcast on agent join and leave
- [ ] Graceful disconnect (Goodbye) cleans up agent state
- [ ] Ungraceful disconnect (crash/broken pipe) also cleans up agent state
- [ ] Stale socket file is cleaned up on re-invocation
- [ ] Malformed Hello messages are rejected with clear error
- [ ] Room capacity limit (256) is enforced for agents
- [ ] Message size limit (64KB) is enforced for agent messages
- [ ] Heartbeat/ping detects unresponsive agents
- [ ] Agent PeerId uses `agent:` prefix and is distinguishable from human peers
- [ ] Bridge protocol is JSON lines (one JSON object per `\n`-terminated line)
