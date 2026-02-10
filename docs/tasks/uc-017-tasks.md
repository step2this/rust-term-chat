# Tasks for UC-017: Connect TUI to Live Backend State

Generated from use case on 2026-02-09.

## Summary
- **Total tasks**: 12
- **Implementation tasks**: 10
- **Test tasks**: 1
- **Refactor tasks**: 1
- **Critical path**: T-017-01 → T-017-02 → T-017-03 → T-017-05 → T-017-08 → T-017-12
- **Estimated total size**: XL

## Dependency Graph

```
T-017-01 (Remove demo data, add connection state) ─────────────────────────┐
    │                                                                       │
    ├── T-017-02 (Per-conversation message storage)                        │
    │       │                                                               │
    │       ├── T-017-03 (Wire NetEvent presence/typing)                   │
    │       │       │                                                       │
    │       │       └── T-017-05 (Chat panel conversation filter)          │
    │       │               │                                               │
    │       │               └── T-017-08 (Unread counts + sidebar preview) │
    │       │                                                               │
    │       └── T-017-07 (Message delivery status tracking)                │
    │                                                                       │
    ├── T-017-04 (Live status bar)                                         │
    │                                                                       │
    ├── T-017-06 (NetCommand expansion: room + typing)                     │
    │       │                                                               │
    │       └── T-017-09 (Room commands: /create-room, /join-room, etc.)   │
    │               │                                                       │
    │               └── T-017-10 (Room event wiring: relay ↔ app)          │
    │                                                                       │
    ├── T-017-11 (Offline/disconnected error handling)                     │
    │                                                                       │
    └── T-017-12 (Integration test: tui_live_backend)  ────────────────────┘
```

## Tasks

### T-017-01: Remove hardcoded demo data and add connection state to App
- **Type**: Refactor
- **Module**: `termchat/src/app.rs`
- **Description**:
  1. Remove all hardcoded demo data from `App::new()`: the 3 demo conversations, 10 demo messages, demo presence_map entries, demo typing_peers entries.
  2. Add connection state fields to App struct: `is_connected: bool`, `connection_info: String` (e.g., "Relay", "P2P", "").
  3. Add method `App::set_connection_status(connected: bool, info: &str)` that updates both fields.
  4. `App::new()` should initialize with empty conversations, empty messages, `is_connected: false`, `connection_info: ""`.
  5. When `--remote-peer` is provided (in main.rs), create initial DM conversation entry "@ {remote_peer}" with unknown presence.
- **From**: MSS Steps 1-4, Invariant 1, Postcondition 9
- **Depends On**: none
- **Blocks**: T-017-02, T-017-03, T-017-04, T-017-06, T-017-11, T-017-12
- **Size**: M (100-150 lines changed)
- **Risk**: Medium — many existing tests reference App::new() with demo data; need to update or provide App::new_demo() for backward compat
- **Agent Assignment**: Teammate:Builder-TUI
- **Acceptance Test**: `cargo test --lib -p termchat` passes; `cargo run` with no flags shows empty UI with "Disconnected" status

### T-017-02: Refactor to per-conversation message storage
- **Type**: Implementation
- **Module**: `termchat/src/app.rs`
- **Description**:
  1. Change `messages: Vec<DisplayMessage>` → `messages: HashMap<String, Vec<DisplayMessage>>` keyed by conversation name (e.g., "@ bob", "# dev-chat").
  2. Add method `App::push_message(conversation: &str, msg: DisplayMessage)` that inserts into the correct conversation bucket.
  3. Add method `App::current_messages() -> &[DisplayMessage]` that returns messages for the selected conversation.
  4. When a message arrives for a conversation that doesn't exist yet, auto-create the conversation entry (extension 10a).
  5. Update `submit_message()` to push messages into the selected conversation's bucket.
  6. Preserve backward compatibility: `app.messages` field access must be replaced with `app.current_messages()` in all call sites.
- **From**: MSS Steps 7-11, Postcondition 1, Invariant 2
- **Depends On**: T-017-01
- **Blocks**: T-017-05, T-017-07, T-017-08
- **Size**: L (200-300 lines changed across app.rs, main.rs, chat_panel.rs)
- **Risk**: Medium — touching core data structure; must update all message access patterns
- **Agent Assignment**: Teammate:Builder-TUI
- **Acceptance Test**: Messages pushed to "@ alice" don't appear in "@ bob" conversation; `current_messages()` returns correct subset

### T-017-03: Wire presence and typing events through net layer
- **Type**: Implementation
- **Module**: `termchat/src/net.rs`, `termchat/src/main.rs`
- **Description**:
  1. Add `NetEvent::PresenceChanged { peer_id: String, status: String }` variant.
  2. Add `NetEvent::TypingChanged { peer_id: String, room_id: String, is_typing: bool }` variant.
  3. In `chat_event_forwarder()`: map `ChatEvent::PresenceChanged` → `NetEvent::PresenceChanged` (currently returns None).
  4. In `chat_event_forwarder()`: map `ChatEvent::TypingChanged` → `NetEvent::TypingChanged` (currently returns None).
  5. In `drain_net_events()` (main.rs): handle `NetEvent::PresenceChanged` by calling `app.set_peer_presence()`.
  6. In `drain_net_events()` (main.rs): handle `NetEvent::TypingChanged` by calling `app.set_peer_typing()`.
  7. Extension 6a: wrap decode in try/catch, log and drop malformed events.
  8. Extension 14a: typing send failures are fire-and-forget (already handled by transport layer).
- **From**: MSS Steps 14-16, Postconditions 5-6, Extensions 6a, 14a
- **Depends On**: T-017-01
- **Blocks**: T-017-05
- **Size**: M (80-120 lines)
- **Risk**: Low — infrastructure exists (set_peer_presence, set_peer_typing already work), just needs wiring
- **Agent Assignment**: Teammate:Builder-Infra
- **Acceptance Test**: When remote peer types, `app.typing_peers` updates; when remote peer goes away, `app.presence_map` updates

### T-017-04: Live connection status in status bar
- **Type**: Implementation
- **Module**: `termchat/src/ui/status_bar.rs`, `termchat/src/main.rs`
- **Description**:
  1. Replace hardcoded `Span::raw(" Connected (demo mode)")` in status_bar.rs with dynamic rendering based on `app.is_connected` and `app.connection_info`.
  2. When `is_connected == true`: show `"● Connected via {connection_info}"` with green dot.
  3. When `is_connected == false` and connection_info is empty: show `"○ Disconnected"` with gray dot.
  4. When reconnecting: show `"◐ Reconnecting..."` with yellow dot (use a `reconnecting` state field or derive from connection_info).
  5. In `drain_net_events()`: handle `NetEvent::ConnectionStatus` by calling `app.set_connection_status()` (not just pushing system message).
  6. Handle `NetEvent::Reconnecting` by setting a reconnecting state.
  7. Handle `NetEvent::ReconnectFailed` by setting disconnected state.
  8. Extension 2a: on startup failure, status shows "Disconnected".
  9. Extension 2b: on mid-session drop, status shows "Reconnecting..." then "Connected via Relay" on success.
- **From**: MSS Steps 2-3, Postcondition 4, Extensions 2a, 2b
- **Depends On**: T-017-01
- **Blocks**: T-017-12
- **Size**: S (40-60 lines)
- **Risk**: Low — straightforward UI change
- **Agent Assignment**: Teammate:Builder-TUI
- **Acceptance Test**: Status bar shows "Connected via Relay" when connected, "Disconnected" when not

### T-017-05: Filter chat panel messages by selected conversation
- **Type**: Implementation
- **Module**: `termchat/src/ui/chat_panel.rs`, `termchat/src/app.rs`
- **Description**:
  1. Update `chat_panel::render()` to use `app.current_messages()` instead of `app.messages`.
  2. Update chat panel title to show selected conversation name (e.g., "Chat: # general" or "Chat: @ bob").
  3. Extension 4a: when no conversation is selected (empty list), show placeholder text "No conversations — connect with --remote-peer or /join-room".
  4. Typing indicator should only show for the selected conversation (filter `current_typing_peers()` by selected conversation name).
- **From**: MSS Steps 7-11, Postcondition 1, Extension 4a
- **Depends On**: T-017-02, T-017-03
- **Blocks**: T-017-08
- **Size**: S (30-50 lines)
- **Risk**: Low — rendering change, well-contained
- **Agent Assignment**: Teammate:Builder-TUI
- **Acceptance Test**: Switching conversations changes displayed messages; typing indicator shows only for active conversation

### T-017-06: Expand NetCommand with room and typing variants
- **Type**: Implementation
- **Module**: `termchat/src/net.rs`
- **Description**:
  1. Add `NetCommand::SendMessage { conversation_id: String, text: String }` — add conversation_id to existing variant.
  2. Add `NetCommand::SetTyping { conversation_id: String, is_typing: bool }`.
  3. Add `NetCommand::CreateRoom { name: String }`.
  4. Add `NetCommand::ListRooms`.
  5. Add `NetCommand::JoinRoom { room_id: String }`.
  6. Add `NetCommand::ApproveJoin { room_id: String, peer_id: String }`.
  7. Add `NetCommand::DenyJoin { room_id: String, peer_id: String }`.
  8. In `command_handler()`: dispatch each new variant to the appropriate backend call (RoomManager, ChatManager typing).
  9. Add `NetEvent::RoomCreated { room_id: String, name: String }`.
  10. Add `NetEvent::RoomList { rooms: Vec<(String, String, u32)> }` (room_id, name, member_count).
  11. Add `NetEvent::JoinRequestReceived { room_id: String, peer_id: String, display_name: String }`.
  12. Add `NetEvent::JoinApproved { room_id: String, name: String }`.
  13. Add `NetEvent::JoinDenied { room_id: String, reason: String }`.
- **From**: MSS Steps 8, 14, 17-22
- **Depends On**: T-017-01
- **Blocks**: T-017-09, T-017-10
- **Size**: L (200-300 lines)
- **Risk**: Medium — needs to integrate with existing command_handler; must handle room protocol messages from relay
- **Agent Assignment**: Teammate:Builder-Infra
- **Acceptance Test**: `NetCommand::CreateRoom { name }` triggers room registration at relay; room events flow back as NetEvent variants

### T-017-07: Message delivery status tracking
- **Type**: Implementation
- **Module**: `termchat/src/app.rs`, `termchat/src/main.rs`
- **Description**:
  1. Add `message_id: Option<String>` field to `DisplayMessage`.
  2. When submitting a message, generate a UUID and store it in the DisplayMessage.
  3. In `drain_net_events()`: handle `NetEvent::StatusChanged` by finding the message by ID (not by searching for "You" sender) and updating its status.
  4. Initial status is `Sending`; on ACK, transition to `Delivered`.
  5. Extension 13a: if no ACK within 10 seconds, status stays at `Sending` (no timeout implementation needed for MVP — just don't auto-transition).
  6. Extension 9a: validate message size before sending; show error if > 64KB.
- **From**: MSS Steps 9, 12-13, Postcondition 7, Extensions 9a, 13a
- **Depends On**: T-017-02
- **Blocks**: T-017-12
- **Size**: M (80-120 lines)
- **Risk**: Low — StatusChanged event already flows; just needs proper ID-based lookup
- **Agent Assignment**: Teammate:Builder-TUI
- **Acceptance Test**: Message starts as "Sending", transitions to "Delivered" when ACK arrives

### T-017-08: Unread counts and sidebar preview updates
- **Type**: Implementation
- **Module**: `termchat/src/app.rs`, `termchat/src/ui/sidebar.rs`
- **Description**:
  1. In `App::push_message()`: if the message's conversation is NOT the currently selected conversation, increment `unread_count` on the ConversationItem.
  2. Update `last_message_preview` on the ConversationItem when a new message arrives.
  3. When user selects a conversation, reset its `unread_count` to 0.
  4. Sidebar already renders unread_count and last_message_preview — just need to ensure the data is live.
- **From**: MSS Steps 10-11, Postcondition 8
- **Depends On**: T-017-02, T-017-05
- **Blocks**: T-017-12
- **Size**: S (40-60 lines)
- **Risk**: Low — ConversationItem fields already exist
- **Agent Assignment**: Teammate:Builder-TUI
- **Acceptance Test**: Incoming message to non-active conversation shows unread badge; selecting conversation resets badge to 0

### T-017-09: Room commands in App::handle_command()
- **Type**: Implementation
- **Module**: `termchat/src/app.rs`, `termchat/src/main.rs`
- **Description**:
  1. Add `/create-room <name>` → sends `NetCommand::CreateRoom { name }`.
  2. Add `/list-rooms` → sends `NetCommand::ListRooms`.
  3. Add `/join-room <room-id>` → sends `NetCommand::JoinRoom { room_id }`.
  4. Add `/approve <peer-id>` → sends `NetCommand::ApproveJoin` (uses current room context).
  5. Add `/deny <peer-id>` → sends `NetCommand::DenyJoin` (uses current room context).
  6. Extension 7a: check `app.is_connected` before sending; show "Not connected" error if disconnected.
  7. Need to pass `cmd_tx` (NetCommand sender) to `handle_command()` or return a NetCommand for main.rs to dispatch.
  8. Variation 17c: `/list-rooms` with no rooms shows "No rooms available".
- **From**: MSS Steps 17-22, Postcondition 3, Extensions 17a, 17b, 19a, Variations 7b, 17c
- **Depends On**: T-017-06
- **Blocks**: T-017-10
- **Size**: M (100-150 lines)
- **Risk**: Medium — needs to plumb NetCommand sender into App (currently App doesn't hold channel reference)
- **Agent Assignment**: Teammate:Builder-TUI
- **Acceptance Test**: `/create-room test` sends NetCommand::CreateRoom; `/list-rooms` sends NetCommand::ListRooms

### T-017-10: Wire room events from relay to App conversations
- **Type**: Implementation
- **Module**: `termchat/src/net.rs`, `termchat/src/main.rs`, `termchat/src/app.rs`
- **Description**:
  1. In `command_handler()`: handle `NetCommand::CreateRoom` by sending `RoomMessage::RegisterRoom` to relay.
  2. In receive path: handle incoming `RoomMessage::RegisterRoom` confirmation → emit `NetEvent::RoomCreated`.
  3. In `drain_net_events()`: handle `NetEvent::RoomCreated` → add "# {name}" to `app.conversations`.
  4. Handle `NetEvent::RoomList` → display room list in chat panel as system messages.
  5. Handle `NetEvent::JoinRequestReceived` → show "peer wants to join room" system message.
  6. Handle `NetEvent::JoinApproved` → add "# {name}" to `app.conversations`.
  7. Handle `NetEvent::JoinDenied` → show denial system message.
  8. Extension 17a/17b: relay errors for room creation → emit `NetEvent::Error` with descriptive message.
  9. Extension 20a: join request queued when admin offline → system message about waiting.
  10. Extension 21a: admin denies → JoinDenied event flows to joiner.
- **From**: MSS Steps 17-23, Extensions 17a, 17b, 20a, 21a
- **Depends On**: T-017-06, T-017-09
- **Blocks**: T-017-12
- **Size**: L (200-300 lines)
- **Risk**: High — integrates relay protocol, room manager, and app state; most complex task
- **Agent Assignment**: Teammate:Builder-Infra
- **Acceptance Test**: `/create-room test` creates room at relay and adds conversation; `/join-room` + `/approve` adds room to joiner's sidebar

### T-017-11: Offline and disconnected error handling
- **Type**: Implementation
- **Module**: `termchat/src/app.rs`, `termchat/src/main.rs`
- **Description**:
  1. Extension 7a: in `send_message_command()`, check `app.is_connected` before sending. If not connected, add message with "Failed" status and show system message "Not connected — message not sent".
  2. Extension 8a: `try_send()` already handles backpressure; just ensure the error message matches spec ("Network busy — try again").
  3. Extension 2a: on startup failure, already handled by spawn_net error path; just ensure App state is set correctly.
  4. Extension 23a: room message failure → same as DM failure handling.
  5. Variation 1b: `cargo run` with no flags → empty UI, "Disconnected" (already handled by T-017-01).
- **From**: Extensions 2a, 7a, 8a, 23a, Variation 1b
- **Depends On**: T-017-01
- **Blocks**: T-017-12
- **Size**: S (30-50 lines)
- **Risk**: Low — mostly adding conditional checks to existing code paths
- **Agent Assignment**: Teammate:Builder-TUI
- **Acceptance Test**: Sending message while disconnected shows "Failed" status and error message

### T-017-12: Integration test — tui_live_backend
- **Type**: Test
- **Module**: `tests/integration/tui_live_backend.rs`
- **Description**:
  1. Test per-conversation message isolation: push messages to two conversations, verify each only contains its own messages.
  2. Test connection status wiring: simulate ConnectionStatus event, verify app.is_connected updates.
  3. Test presence wiring: simulate PresenceChanged event, verify app.presence_map updates.
  4. Test typing wiring: simulate TypingChanged event, verify app.typing_peers updates.
  5. Test room creation flow: send CreateRoom command, verify RoomCreated event creates conversation.
  6. Test unread counts: push message to non-active conversation, verify unread_count increments.
  7. Test offline mode: app with no network connection shows empty conversations and "Disconnected".
  8. Test message delivery status: submit message, simulate ACK, verify Sending → Delivered transition.
  9. Verify all 699+ existing tests still pass (zero regressions).
- **From**: All postconditions, all acceptance criteria
- **Depends On**: T-017-01 through T-017-11 (all implementation tasks)
- **Blocks**: none
- **Size**: L (200-300 lines)
- **Risk**: Medium — needs to set up relay server for some tests; may use mock/stub transport for unit-level tests
- **Agent Assignment**: Teammate:Reviewer
- **Acceptance Test**: `cargo test --test tui_live_backend` passes; `cargo test` passes (zero regressions)

## Implementation Order

| Order | Task | Type | Size | Depends On | Track |
|-------|------|------|------|------------|-------|
| 1 | T-017-01: Remove demo data, add connection state | Refactor | M | none | TUI |
| 2a | T-017-02: Per-conversation message storage | Impl | L | T-017-01 | TUI |
| 2b | T-017-03: Wire presence/typing events | Impl | M | T-017-01 | Infra |
| 2c | T-017-04: Live status bar | Impl | S | T-017-01 | TUI |
| 2d | T-017-06: Expand NetCommand (room + typing) | Impl | L | T-017-01 | Infra |
| 2e | T-017-11: Offline error handling | Impl | S | T-017-01 | TUI |
| 3a | T-017-05: Chat panel conversation filter | Impl | S | T-017-02, T-017-03 | TUI |
| 3b | T-017-07: Message delivery status tracking | Impl | M | T-017-02 | TUI |
| 3c | T-017-09: Room commands | Impl | M | T-017-06 | TUI |
| 4a | T-017-08: Unread counts + sidebar preview | Impl | S | T-017-02, T-017-05 | TUI |
| 4b | T-017-10: Room event wiring (relay ↔ app) | Impl | L | T-017-06, T-017-09 | Infra |
| 5 | T-017-12: Integration test | Test | L | all | Reviewer |

## Parallel Tracks

**Track A (Builder-TUI)**: T-017-01 → T-017-02 → T-017-05 → T-017-08 (per-conversation pipeline)
**Track B (Builder-Infra)**: T-017-01 → T-017-06 → T-017-10 (room/network pipeline)
**Track C (parallel with A)**: T-017-04, T-017-07, T-017-11 (can start after T-017-01)

After T-017-01, Tracks A and B can run in parallel since they touch different files:
- **Builder-TUI** owns: `app.rs`, `ui/status_bar.rs`, `ui/chat_panel.rs`, `ui/sidebar.rs`
- **Builder-Infra** owns: `net.rs`, relay integration
- **Shared**: `main.rs` (coordinate carefully — TUI does drain_net_events, Infra does command dispatch)

## Notes for Agent Team

1. **T-017-01 is the critical gate** — nothing else can start until demo data is removed and connection state is added. Prioritize this task.
2. **main.rs is a contention point** — both Builder-TUI (drain_net_events) and Builder-Infra (command handler wiring) touch it. Assign main.rs primarily to Builder-TUI; Builder-Infra focuses on net.rs internals and provides the NetEvent/NetCommand contracts for TUI to consume.
3. **Existing tests may break** when demo data is removed from App::new(). Consider adding `App::new_with_demo()` as a test-only constructor, or update tests to set up their own data.
4. **Room commands need NetCommand sender in App** — currently App doesn't hold a channel reference. Options: (a) return a `Vec<NetCommand>` from handle_command(), (b) pass sender as parameter, (c) store sender in App. Option (a) is cleanest.
5. **Keep tasks under 20 tool calls each** — if a task grows, split it.
6. **Quality gate per task**: `cargo fmt && cargo clippy -p termchat -- -D warnings` before marking complete.
