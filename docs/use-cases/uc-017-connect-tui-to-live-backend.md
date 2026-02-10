# Use Case: UC-017 Connect TUI to Live Backend State

## Classification
- **Goal Level**: 🌊 User Goal
- **Scope**: System (black box)
- **Priority**: P0 Critical
- **Complexity**: 🔴 High

## Actors
- **Primary Actor**: Terminal User (person using TermChat TUI)
- **Supporting Actors**: Relay Server (termchat-relay), Remote Peer (another TermChat instance), Transport Layer (RelayTransport/HybridTransport), Crypto Layer (NoiseSession), ChatManager, RoomManager
- **Stakeholders & Interests**:
  - Terminal User: sees real data — messages routed to correct conversations, actual connection status, real typing indicators from peers, delivery confirmation on sent messages
  - Remote Peer: same experience from the other end — messages they send appear in the correct conversation on the receiver's screen
  - System: no hardcoded demo state remains in the TUI; all displayed data comes from the backend networking layer

## Conditions
- **Preconditions** (must be true before starting):
  1. Relay server URL is configured (via `--relay-url` CLI arg or config file)
  2. UC-001 through UC-016 are complete and passing (699 tests)
  3. TUI renders correctly with three-panel layout (Phase 1)
  4. `net::spawn_net()` connects to relay and exchanges messages (UC-010)
  5. ChatManager send/receive pipeline works end-to-end (UC-001, UC-002, UC-014)
  6. RoomManager and room protocol types exist (UC-006, UC-016)
  7. Presence and typing protocol types exist in termchat-proto (UC-009)
- **Success Postconditions** (true when done right):
  1. Messages sent by User A appear only in the correct conversation on User B's screen (per-conversation isolation)
  2. Conversation list is populated dynamically from network events — no hardcoded entries remain
  3. `/create-room <name>` creates a room registered at the relay; `/list-rooms` shows available rooms; `/join-room <id>` sends a join request
  4. Status bar shows actual connection state: "Connected via Relay" when connected, "Disconnected" when not, "Reconnecting..." during reconnect
  5. Typing indicator shows the actual remote peer's name (e.g., "bob is typing..."), not hardcoded "Alice is typing..."
  6. Presence dots in sidebar reflect actual remote peer status (online/away/offline)
  7. Message delivery status transitions: Sending → Delivered on ACK receipt (two-state model for MVP)
  8. Unread count badge increments on incoming messages for non-active conversations
  9. `cargo run` with no flags still launches in offline mode (backwards compatible) — but with an empty conversation list instead of hardcoded demo data
- **Failure Postconditions** (true when it fails gracefully):
  1. If relay is unreachable, TUI launches with empty conversation list and "Disconnected" status — no crash
  2. If room creation fails (duplicate name, capacity), user sees error message in chat panel
  3. If a received message references an unknown conversation, a new conversation entry is auto-created in the sidebar
  4. If typing/presence events fail to decode, they are silently dropped — UI continues normally
- **Invariants** (must remain true throughout):
  1. No hardcoded demo data remains in the TUI — zero fake messages, zero fake conversations, zero hardcoded names in typing indicators
  2. Messages never appear in the wrong conversation — per-conversation isolation is strict
  3. The TUI event loop never blocks for more than 50ms (existing poll timeout preserved)
  4. Messages are encrypted before transmission (existing crypto pipeline unchanged)
  5. All existing 699+ tests continue to pass — no regressions

## Main Success Scenario
1. User A starts TermChat with `--relay-url ws://127.0.0.1:9000/ws --peer-id alice --remote-peer bob`
2. System connects to relay, registers peer ID, and updates App state: `is_connected = true`, `connection_info = "Relay"`
3. Status bar renders "Connected via Relay" (reading live state from App)
4. System creates a DM conversation entry "@ bob" in sidebar from `--remote-peer` arg (presence dot defaults to unknown)
5. User B starts TermChat with `--peer-id bob --remote-peer alice` on the same relay
6. User B's system creates "@ alice" DM conversation from `--remote-peer` arg; both peers' typing/presence events will update these entries as they flow
7. User A selects the "@ bob" conversation and types a message, presses Enter
8. System sends `NetCommand::SendMessage { conversation_id, text }` — message is routed to bob via relay
9. Message appears in User A's chat panel with "Sending..." delivery status
10. User B's system receives the message; `drain_net_events()` routes it to the "@ alice" conversation (auto-created if needed)
11. Message appears in User B's "@ alice" conversation chat panel with correct sender name and timestamp
12. User B's system sends delivery ACK back through the pipeline
13. User A receives the ACK; message status updates from "Sending" to "Delivered"
14. User B starts typing a reply; system detects keystrokes and sends `NetCommand::SetTyping { conversation_id, is_typing: true }`
15. User A's system receives `NetEvent::TypingChanged { peer_id: "bob", is_typing: true }`; chat panel shows "bob is typing..."
16. User B stops typing (3s idle); typing indicator disappears on User A's screen
17. User A runs `/create-room dev-chat`; system sends `NetCommand::CreateRoom { name: "dev-chat" }`
18. Relay registers the room; User A sees "Room 'dev-chat' created" system message and a new "# dev-chat" conversation appears in sidebar
19. User B runs `/list-rooms`; sees "dev-chat" in the room list
20. User B runs `/join-room <room-id>`; join request is routed to User A (room admin)
21. User A sees "bob wants to join dev-chat" and runs `/approve bob`
22. User B receives JoinApproved; "# dev-chat" conversation appears in their sidebar
23. Both users can now exchange messages in the "# dev-chat" room conversation

## Extensions (What Can Go Wrong)
- **2a. Relay server is unreachable at startup**:
  1. `spawn_net()` fails to connect
  2. App state set to `is_connected = false`
  3. Status bar shows "Disconnected"
  4. Sidebar is empty; user can still browse UI but cannot send messages
  5. System message: "Could not connect to relay server"
- **2b. Relay connection drops mid-session**:
  1. Transport layer detects disconnect
  2. `NetEvent::ConnectionStatus(false)` fired; App state updated
  3. Status bar changes to "Reconnecting..."
  4. Existing conversations remain visible; new messages show "Failed" status
  5. On reconnect, status bar returns to "Connected via Relay"
- **7a. User sends message while disconnected**:
  1. Message appears locally with "Failed" delivery status
  2. System message: "Not connected — message not sent"
- **8a. NetCommand channel is full (back-pressure)**:
  1. `try_send()` returns error
  2. Message appears locally with "Failed" status
  3. System message: "Network busy — try again"
- **10a. Received message references unknown peer (no existing conversation)**:
  1. System auto-creates a new DM conversation entry in sidebar
  2. Message is stored in the new conversation
  3. Unread count badge shows 1
- **11a. Received message fails to decrypt/decode**:
  1. Error logged; message dropped silently
  2. No conversation impact; UI continues normally
- **14a. Typing event send fails (transport error)**:
  1. Fire-and-forget: error logged, no user-visible impact
  2. Remote peer simply doesn't see typing indicator
- **17a. Room creation fails — duplicate name**:
  1. Relay returns error
  2. System message: "Room creation failed: a room with that name already exists"
  3. No conversation created
- **17b. Room creation fails — registry full**:
  1. Relay returns error
  2. System message: "Room creation failed: server room limit reached"
- **20a. Join request sent but admin is offline**:
  1. Relay queues the join request via store-and-forward
  2. System message: "Join request sent — waiting for admin approval"
  3. User B waits; approval arrives when admin reconnects
- **21a. Admin denies the join request**:
  1. User A runs `/deny bob`
  2. User B receives JoinDenied
  3. System message on User B: "Join request denied by room admin"
  4. No room conversation created for User B
- **4a. User types before any conversation exists (no `--remote-peer` specified)**:
  1. Chat panel shows "No conversations — connect with --remote-peer or /join-room" placeholder
  2. Input is accepted but message is not sent; system message: "No conversation selected"
- **6a. Presence event decode fails**:
  1. Malformed presence data logged and dropped
  2. Sidebar shows no presence dot for that peer (defaults to unknown)
- **9a. Message exceeds size limit**:
  1. System shows error: "Message too long (max 64KB)"
  2. Message is not sent; user edits and retries
- **13a. No ACK received within timeout (10s)**:
  1. Message status stays at "Sending" (not "Delivered")
  2. No automatic retry — user can resend manually
- **19a. Relay is unreachable when listing rooms**:
  1. System message: "Cannot list rooms — not connected to relay"
- **23a. Room message delivery fails**:
  1. Same handling as DM failure — message shows "Failed" status
  2. System message: "Message not sent — check connection"

## Variations
- **1b.** User starts with no flags → offline mode with empty UI and "Disconnected" status (no demo data)
- **7b.** User sends a `/command` → routed to `handle_command()`, not sent to network
- **17c.** User runs `/list-rooms` before creating any rooms → shows "No rooms available"
- **22b.** Room admin auto-approves if room is set to open (future extension — not implemented in UC-017)

## Out of Scope
- **Real Noise XX handshake in TUI flow**: StubNoiseSession used for integration; real handshake already works in lower layers
- **Persistent message history**: In-memory only; SQLite persistence is a future UC
- **Multi-peer P2P rooms**: Room messages go through relay; direct P2P fan-out is deferred
- **File sharing or rich media**: Text messages only
- **Message editing or deletion**: Send-once semantics
- **Room permissions beyond admin approve/deny**: No roles, no moderation tools
- **Peer discovery UI**: User must know peer ID or room ID; no search/browse

## Agent Execution Notes
- **Verification Command**: `cargo test --test tui_live_backend && cargo test`
- **Test File**: `tests/integration/tui_live_backend.rs`
- **Depends On**: UC-001 (Send), UC-002 (Receive), UC-006 (Rooms), UC-009 (Typing/Presence), UC-010 (Relay Wiring), UC-014 (ChatManager refactor), UC-016 (Join Routing)
- **Blocks**: None (this is the current end-of-chain for TUI work)
- **Estimated Complexity**: XL / High token budget — touches 7+ files, 8 major changes
- **Agent Assignment**: Multi-agent team (Builder-TUI for app/ui, Builder-Infra for net.rs, Reviewer for testing)

## Acceptance Criteria (for grading)
- [ ] All success postconditions verified by automated test
- [ ] All extension paths have explicit handling (error messages, graceful fallback)
- [ ] No invariant violations detected
- [ ] Code passes `cargo fmt --check && cargo clippy -- -D warnings && cargo test && cargo deny check`
- [ ] Reviewer agent approves
- [ ] Two TUI instances exchange messages via relay with per-conversation isolation
- [ ] Room creation via `/create-room` works end-to-end
- [ ] Room joining via `/join-room` + `/approve` works end-to-end
- [ ] `/list-rooms` shows relay-registered rooms
- [ ] Status bar shows real connection state (not hardcoded "demo mode")
- [ ] Typing indicator shows actual remote peer name (not hardcoded "Alice")
- [ ] Presence dots update from network events (not hardcoded)
- [ ] Message delivery status transitions: Sending → Delivered on ACK (two-state model)
- [ ] Unread count increments for non-active conversations
- [ ] `cargo run` with no flags shows empty UI with "Disconnected" — no demo data
- [ ] All 699+ existing tests continue to pass (zero regressions)
