# Agent Team Plan: UC-017 Connect TUI to Live Backend State

Generated on 2026-02-09.

## Design Rationale

UC-017 has 12 tasks across **two parallel tracks**: Track A (TUI state: app.rs, ui/*, main.rs) and Track B (Infra: net.rs, relay integration). The critical gate is T-017-01 (remove demo data, add connection state) which unblocks everything — Lead completes this before spawning builders.

**Key contention point**: `main.rs` is touched by both tracks. Resolution: Builder-TUI owns `main.rs` (drain_net_events, send_message_command). Builder-Infra owns `net.rs` internals and provides the NetEvent/NetCommand contract for Builder-TUI to consume. Builder-Infra does NOT edit main.rs — they edit net.rs and the Lead coordinates any main.rs changes needed for new NetEvent variants.

**Cross-track dependency**: T-017-09 (room commands in app.rs) needs the NetCommand variants from T-017-06 (in net.rs). Builder-Infra does T-017-03 + T-017-06 first, providing the contract. Builder-TUI works on T-017-02 → T-017-04 → T-017-07 → T-017-11 while waiting.

**Team size: 4** (Lead + 2 Builders + 1 Reviewer). Proven pattern from UC-006/007/008 — zero kills, zero merge conflicts.

**Model selection**: Sonnet for all teammates. Tasks are well-specified with acceptance criteria. Proven across prior sprints.

**Max turns**: 25 per builder, 20 for reviewer. Per retrospective: keeping under 25 prevents context kills.

**Execution strategy**: Lead handles T-017-01 (the demo data removal gate), then spawns both builders simultaneously. Reviewer starts after integration gate. Lead runs gate checks between phases.

## Team Composition

| Role | Agent Name | Model | Responsibilities |
|------|-----------|-------|-----------------|
| Lead | `lead` | (current session) | T-017-01 (prerequisites), task routing, review gates, main.rs coordination, commit |
| Builder-TUI | `builder-tui` | Sonnet | App state + UI: `app.rs`, `ui/status_bar.rs`, `ui/chat_panel.rs`, `ui/sidebar.rs` (T-017-02, T-017-04, T-017-05, T-017-07, T-017-08, T-017-09, T-017-11) |
| Builder-Infra | `builder-infra` | Sonnet | Net layer: `net.rs` (T-017-03, T-017-06, T-017-10) |
| Reviewer | `reviewer` | Sonnet | Integration tests: `tests/integration/tui_live_backend.rs` (T-017-12) |

### File Ownership (strict — no overlap)

| Agent | Owns (exclusive write) |
|-------|----------------------|
| Lead | All `Cargo.toml` files, `*/lib.rs` module declarations, `main.rs`, `CLAUDE.md`, docs/ |
| Builder-TUI | `termchat/src/app.rs`, `termchat/src/ui/status_bar.rs`, `termchat/src/ui/chat_panel.rs`, `termchat/src/ui/sidebar.rs` |
| Builder-Infra | `termchat/src/net.rs` |
| Reviewer | `tests/integration/tui_live_backend.rs` |

**Shared read-only**: `termchat-proto/src/` (both builders import types but don't modify), `termchat/src/chat/` (room.rs API consumed by Builder-Infra).

**Lead handles main.rs**: When Builder-TUI needs drain_net_events updated for new NetEvent variants, Lead applies the changes after Builder-Infra delivers the contract. This prevents merge conflicts.

## Task Assignment

| Task | Owner | Phase | Review Gate | Est. Turns |
|------|-------|-------|-------------|------------|
| T-017-01: Remove demo data, add connection state | `lead` | 1 | Gate 1 | 8-12 |
| T-017-02: Per-conversation message storage | `builder-tui` | 2A | — | 10-15 |
| T-017-04: Live status bar | `builder-tui` | 2A | — | 3-5 |
| T-017-07: Message delivery status tracking | `builder-tui` | 2A | — | 5-8 |
| T-017-11: Offline error handling | `builder-tui` | 2A | — | 3-5 |
| T-017-03: Wire presence/typing events | `builder-infra` | 2B | — | 5-8 |
| T-017-06: Expand NetCommand (room + typing) | `builder-infra` | 2B | Gate 2 | 10-15 |
| T-017-05: Chat panel conversation filter | `builder-tui` | 3A | — | 3-5 |
| T-017-09: Room commands | `builder-tui` | 3A | — | 5-8 |
| T-017-08: Unread counts + sidebar preview | `builder-tui` | 3A | — | 3-5 |
| T-017-10: Room event wiring (relay ↔ app) | `builder-infra` | 3B | Gate 3 | 10-15 |
| T-017-12: Integration test | `reviewer` | 4 | Gate 4 | 15-20 |

## Execution Phases

### Phase 1: Prerequisites (Lead only)
- **Tasks**: T-017-01
- **Actions**:
  1. Remove all hardcoded demo data from `App::new()`: 3 conversations, 10 messages, demo presence_map, demo typing_peers.
  2. Add `is_connected: bool` and `connection_info: String` fields to App struct.
  3. Add `set_connection_status()` method.
  4. Initialize App with empty state: no conversations, no messages, `is_connected: false`.
  5. In `main.rs`: when `--remote-peer` is provided, create initial DM conversation "@ {remote_peer}".
  6. Add `App::new_with_demo()` as test-only constructor for existing tests that depend on demo data.
  7. Update `drain_net_events()` in main.rs to call `app.set_connection_status()` for ConnectionStatus/Reconnecting/ReconnectFailed events (in addition to existing system messages).
- **Gate 1**: `cargo build && cargo test --lib -p termchat && cargo clippy -p termchat -- -D warnings`
- **Output**: Both builders are unblocked; App struct has connection state; demo data gone

### Phase 2A: TUI State Track (Builder-TUI) — runs in parallel with 2B
- **Tasks**: T-017-02 → T-017-04 + T-017-07 + T-017-11 (parallel small tasks)
- **Actions**:
  1. T-017-02: Refactor `messages: Vec<DisplayMessage>` → `messages: HashMap<String, Vec<DisplayMessage>>`. Add `push_message()`, `current_messages()`. Update `submit_message()` to use selected conversation. **This is the highest-priority TUI task.**
  2. T-017-04: Replace hardcoded "Connected (demo mode)" in status_bar.rs. Read `app.is_connected` and `app.connection_info`. Show green/gray/yellow dot.
  3. T-017-07: Add `message_id: Option<String>` to DisplayMessage. Generate UUID on submit. Update StatusChanged handler to find by ID instead of "You" sender search.
  4. T-017-11: Check `app.is_connected` before sending. Show "Failed" + error message when disconnected.
- **Key guidance**:
  - `current_messages()` returns `&[DisplayMessage]` for the selected conversation (empty slice if no conversation selected)
  - `push_message()` auto-creates conversation if it doesn't exist (extension 10a)
  - Run `cargo fmt` and `cargo clippy -p termchat -- -D warnings` before marking each task complete

### Phase 2B: Net Layer Track (Builder-Infra) — runs in parallel with 2A
- **Tasks**: T-017-03 → T-017-06
- **Actions**:
  1. T-017-03: Add `NetEvent::PresenceChanged` and `NetEvent::TypingChanged` variants. Wire in `chat_event_forwarder()` (replace the `None` mappings). **Do NOT edit main.rs — Lead will wire drain_net_events.**
  2. T-017-06: Add all new NetCommand variants (CreateRoom, ListRooms, JoinRoom, ApproveJoin, DenyJoin, SetTyping). Add conversation_id to SendMessage. Add all new NetEvent variants (RoomCreated, RoomList, JoinRequestReceived, JoinApproved, JoinDenied). Wire dispatch in `command_handler()`.
- **CRITICAL**: Builder-Infra does NOT edit main.rs. Builder-Infra delivers the NetEvent/NetCommand contract. Lead will update main.rs drain_net_events to handle new variants after Builder-Infra completes.
- **Key guidance**:
  - For room commands: use existing `RoomManager` API from `chat/room.rs` (create_room, approve_join, deny_join, list_rooms)
  - For relay room registration: send `RoomMessage::RegisterRoom` via relay transport
  - Room events from relay (incoming RoomMessage variants) map to NetEvent::RoomCreated/JoinRequestReceived/etc.
  - Run `cargo fmt` and `cargo clippy -p termchat -- -D warnings` before marking each task complete

### Phase 2C: Lead Integration (Lead)
- **After**: Both 2A and 2B complete
- **Actions**:
  1. Update `drain_net_events()` in main.rs to handle all new NetEvent variants (PresenceChanged → set_peer_presence, TypingChanged → set_peer_typing, RoomCreated → add conversation, etc.)
  2. Run integration build gate
- **Gate 2**: `cargo build && cargo test && cargo clippy -- -D warnings`

### Phase 3A: TUI Completion (Builder-TUI) — after Phase 2C
- **Tasks**: T-017-05 → T-017-08, T-017-09
- **Actions**:
  1. T-017-05: Update chat_panel to use `app.current_messages()`. Show conversation name in title. Add empty-state placeholder.
  2. T-017-08: In `push_message()`, increment unread_count for non-active conversations. Update last_message_preview. Reset unread on conversation select.
  3. T-017-09: Add `/create-room`, `/list-rooms`, `/join-room`, `/approve`, `/deny` to handle_command(). Return `Option<NetCommand>` for main.rs to dispatch.
- **Key guidance**:
  - For T-017-09: `handle_command()` should return `Option<NetCommand>` instead of directly sending — Lead will wire the dispatch in main.rs
  - Check `app.is_connected` before room commands; show "Not connected" error

### Phase 3B: Room Event Wiring (Builder-Infra) — after Phase 2C
- **Tasks**: T-017-10
- **Actions**:
  1. In `command_handler()`: wire CreateRoom → RoomMessage::RegisterRoom → relay
  2. Handle relay room confirmations/errors → emit appropriate NetEvent
  3. Handle incoming JoinRequest/JoinApproved/JoinDenied from relay → emit NetEvent
- **CRITICAL**: T-017-10 is the highest-risk task. It integrates relay protocol, room manager, and net events. Builder-Infra focuses exclusively on net.rs; Lead wires main.rs.
- **Gate 3**: `cargo build && cargo test && cargo clippy -- -D warnings`

### Phase 3C: Lead Integration (Lead)
- **After**: 3A and 3B complete
- **Actions**:
  1. Wire room commands: main.rs receives `Option<NetCommand>` from handle_command(), sends to net layer
  2. Wire remaining NetEvent handlers in drain_net_events (room events)
  3. Run full quality gate
- **Gate 3 Final**: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`

### Phase 4: Integration Tests (Reviewer)
- **Tasks**: T-017-12
- **Depends on**: Phase 3C passes
- **Actions**:
  1. Test per-conversation isolation (push to two conversations, verify separation)
  2. Test connection status wiring (simulate events, verify app state)
  3. Test presence/typing wiring (simulate events, verify app state)
  4. Test room creation flow (NetCommand → NetEvent → conversation added)
  5. Test unread counts (message to non-active conversation increments badge)
  6. Test offline mode (no network → empty UI, "Disconnected")
  7. Test delivery status (submit → ACK → Sending → Delivered)
  8. Verify zero regressions (all 699+ existing tests pass)
- **Test pattern**: Write against UC-017 postconditions, not implementation details
- **Gate 4 (Final)**: `cargo fmt --check && cargo clippy -- -D warnings && cargo test && cargo test --test tui_live_backend`

## Review Gates

### Gate 1: Prerequisites Complete
- **After**: T-017-01
- **Commands**: `cargo build && cargo test --lib -p termchat && cargo clippy -p termchat -- -D warnings`
- **Pass criteria**: App struct has connection state, demo data removed, existing tests pass (with new_with_demo helper if needed)
- **On failure**: Lead fixes directly

### Gate 2: Parallel Tracks Complete
- **After**: Phase 2A + 2B + 2C
- **Reviewer checks**: Per-conversation storage works, presence/typing events flow, NetCommand/NetEvent contracts match between tracks
- **Commands**: `cargo build && cargo test && cargo clippy -- -D warnings`
- **Pass criteria**: All crates compile, all tests pass, no clippy warnings
- **On failure**: Lead identifies cross-track issues and routes fixes to responsible builder

### Gate 3: Room Wiring Complete
- **After**: Phase 3A + 3B + 3C
- **Reviewer checks**: Room commands dispatch correctly, room events create conversations, error messages match UC spec
- **Commands**: `cargo build && cargo test && cargo clippy -- -D warnings`
- **Pass criteria**: All crates compile, all tests pass
- **On failure**: Lead routes fixes to responsible builder

### Gate 4: Final UC-017 Verification
- **After**: T-017-12
- **Commands**: `cargo fmt --check && cargo clippy -- -D warnings && cargo test && cargo test --test tui_live_backend && cargo deny check`
- **Pass criteria**: All commands exit 0, all 16 acceptance criteria met
- **On failure**: Specific rework tasks assigned to responsible builder

## Parallelization Opportunities

```
Timeline (phases →)

Phase:    1             2A+2B (parallel)           2C    3A+3B (parallel)   3C    4            Gate4
         ┌─────────────┐┌────────────────────────┐┌───┐┌──────────────────┐┌───┐┌────────────┐┌───┐
lead:    │T-017-01     ││monitoring              ││int││main.rs wiring    ││int││coord       ││chk│
         └─────────────┘└────────────────────────┘└───┘└──────────────────┘└───┘└────────────┘└───┘
                         ┌────────────────────────┐     ┌──────────────────┐
b-tui:                   │02 → 04+07+11          │     │05 → 08, 09       │ (done)
                         └────────────────────────┘     └──────────────────┘
                         ┌────────────────────────┐     ┌──────────────────┐
b-infra:                 │03 → 06                │     │10                │ (done)
                         └────────────────────────┘     └──────────────────┘
                                                                                ┌────────────┐
reviewer:                                                                       │12          │ (done)
                                                                                └────────────┘
```

**Phases 2A and 2B run simultaneously** — builders work on completely separate files (app.rs/ui/* vs net.rs). Zero merge conflicts guaranteed by file ownership.

**Phase 2C is mandatory**: Lead integrates main.rs after both tracks deliver their contracts. This is the proven pattern from Sprints 4-8.

**Phases 3A and 3B run simultaneously**: Builder-TUI works on UI completion while Builder-Infra wires room events. Different files, no conflict.

## Risk Mitigation

| Risk | Task(s) | Mitigation |
|------|---------|------------|
| main.rs contention (both tracks touch it) | All | Lead exclusively owns main.rs. Builders never edit it. Lead integrates after each parallel phase. |
| T-017-02 breaks existing message rendering | T-017-02, T-017-05 | Builder-TUI updates chat_panel.rs simultaneously. Lead provides App::new_with_demo() for existing tests. |
| T-017-10 is highest complexity (relay + room + app) | T-017-10 | Builder-Infra focuses only on net.rs side. Lead handles main.rs wiring. RoomManager API is stable from UC-006. |
| Demo data removal breaks existing unit tests | T-017-01 | Lead adds App::new_with_demo() test-only constructor. Migrates affected tests in Phase 1. |
| Room commands need NetCommand sender in App | T-017-09 | Use return-value pattern: handle_command() returns Option<NetCommand>. Lead dispatches in main.rs. No channel stored in App. |
| Cross-track NetEvent contract mismatch | T-017-03/06 vs T-017-05/09 | Builder-Infra delivers T-017-06 (full contract) first. Lead verifies types compile before Builder-TUI uses them. |

## Spawn Commands

```
# 1. Lead completes Phase 1 directly (T-017-01: remove demo data, add connection state)

# 2. Create the team
TeamCreate: team_name="uc-017-impl", description="UC-017 Connect TUI to Live Backend State"

# 3. Create tasks in shared task list (12 tasks via TaskCreate)

# 4. Spawn BOTH builders simultaneously (parallel Phase 2A + 2B)
Task tool: name="builder-tui", team_name="uc-017-impl", subagent_type="general-purpose", model="sonnet", max_turns=25, mode="plan"
  Prompt: "Claim task T-017-02 immediately. You own Track A: TUI state in termchat/src/app.rs, termchat/src/ui/. Do NOT edit main.rs or net.rs."

Task tool: name="builder-infra", team_name="uc-017-impl", subagent_type="general-purpose", model="sonnet", max_turns=25, mode="plan"
  Prompt: "Claim task T-017-03 immediately. You own Track B: net layer in termchat/src/net.rs. Do NOT edit main.rs, app.rs, or ui/."

# 5. After Phase 2C (Lead integrates main.rs), respawn builders for Phase 3A + 3B

# 6. After Phase 3C (Lead integrates), spawn reviewer
Task tool: name="reviewer", team_name="uc-017-impl", subagent_type="general-purpose", model="sonnet", max_turns=20, mode="plan"
  Prompt: "Claim task T-017-12 immediately. Write integration tests against UC-017 postconditions in tests/integration/tui_live_backend.rs."

# 7. Lead runs Gate 4, commits
```

## Coordination Notes

- **Strict file ownership**: Builder-TUI never touches `net.rs`. Builder-Infra never touches `app.rs`, `ui/*`. Lead exclusively owns `main.rs`. Zero merge conflicts guaranteed.
- **NetEvent/NetCommand contract**: Builder-Infra defines the contract (new enum variants) in net.rs. Builder-TUI writes against these types in app.rs. If types change, Lead coordinates.
- **Return-value pattern for commands**: `handle_command()` returns `Option<NetCommand>` instead of directly sending. Main.rs event loop dispatches the command. This avoids storing a channel in App.
- **Phase 2C/3C are mandatory integration points**: Lead applies main.rs changes after parallel phases, runs quality gate, then unblocks next phase. Proven pattern.
- **Builders must run `cargo fmt` and `cargo clippy -p termchat -- -D warnings` before marking each task complete** (Sprint 6 retro action).
- **Builders claim tasks immediately on spawn** (Sprint 5 retro action).
- **plan_mode_required: true for teammates** — they present a plan before implementing each task.
- **Commit strategy**: One commit after Gate 4 passes. Lead manages the commit and doc updates.
