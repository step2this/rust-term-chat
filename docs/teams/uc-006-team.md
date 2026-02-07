# Agent Team Plan: UC-006 Create Room

Generated on 2026-02-07.

## Design Rationale

UC-006 has 18 tasks across **two parallel tracks**: Track A (room client in `termchat/src/chat/room.rs`) and Track B (relay room registry in `termchat-relay/src/rooms.rs`). These have zero file overlap, enabling genuine parallelism — the same pattern that produced zero conflicts in UC-004.

The shared dependency is `termchat-proto/src/room.rs` (RoomMessage types, T-006-02), which Lead completes before spawning builders. Both tracks code against these shared types.

**Team size: 4** (Lead + 2 Builders + 1 Reviewer). Same composition as UC-004, justified by the same two-crate parallelism pattern.

**Model selection**: Sonnet for all teammates. Tasks are well-specified with acceptance criteria. UC-004 proved Sonnet handles this complexity well.

**Max turns**: 25 per builder (7 tasks for Room, 4 tasks for Relay), 20 for reviewer (3 test tasks). Per retrospective: keeping under 25 prevents context kills.

**Execution strategy**: Lead handles prerequisites (T-006-01, T-006-02, T-006-13, T-006-15), then spawns both builders simultaneously. Reviewer starts after both tracks complete. Lead runs gate checks between phases.

## Team Composition

| Role | Agent Name | Model | Responsibilities |
|------|-----------|-------|-----------------|
| Lead | `lead` | (current session) | Prerequisites (T-006-01, T-006-02, T-006-13, T-006-15), task routing, review gates, commit |
| Builder-Room | `builder-room` | Sonnet | Room model + manager: `termchat/src/chat/room.rs` (T-006-03 through T-006-09) |
| Builder-Relay | `builder-relay` | Sonnet | Relay room registry: `termchat-relay/src/rooms.rs` + relay.rs wiring (T-006-10, T-006-11, T-006-12, T-006-14) |
| Reviewer | `reviewer` | Sonnet | Integration tests: `tests/integration/room_management.rs` (T-006-16, T-006-17, T-006-18) |

### File Ownership (strict — no overlap)

| Agent | Owns (exclusive write) |
|-------|----------------------|
| Lead | All `Cargo.toml` files, `*/lib.rs` module declarations, `termchat-proto/src/room.rs`, `tests/integration/room_management.rs` (stub only) |
| Builder-Room | `termchat/src/chat/room.rs` |
| Builder-Relay | `termchat-relay/src/rooms.rs`, `termchat-relay/src/relay.rs` (room message handling additions only) |
| Reviewer | `tests/integration/room_management.rs` (replaces Lead's stub with full tests) |

## Task Assignment

| Task | Owner | Phase | Review Gate | Est. Turns |
|------|-------|-------|-------------|------------|
| T-006-01: Module stubs + deps | `lead` | 1 | — | 3-4 |
| T-006-02: RoomMessage proto types | `lead` | 1 | — | 5-8 |
| T-006-13: Proto round-trip tests | `lead` | 1 | Gate 1 | 3-4 |
| T-006-15: Integration test stub | `lead` | 1 | — | 2-3 |
| T-006-03: Room struct + RoomManager | `builder-room` | 2A | — | 8-12 |
| T-006-04: Name validation | `builder-room` | 2A | — | 3-5 |
| T-006-05: Create room flow | `builder-room` | 2A | — | 8-12 |
| T-006-06: Room limit + offline | `builder-room` | 2A | — | 3-5 |
| T-006-07: Join request handling | `builder-room` | 2A | — | 8-12 |
| T-006-08: Join extensions | `builder-room` | 2A | — | 5-8 |
| T-006-09: Fan-out + broadcast | `builder-room` | 2A | Gate 2 | 5-8 |
| T-006-10: Relay room registry | `builder-relay` | 2B | — | 8-12 |
| T-006-11: JoinRequest routing | `builder-relay` | 2B | — | 5-8 |
| T-006-12: Relay registry tests | `builder-relay` | 2B | — | 5-8 |
| T-006-14: Wire into relay handler | `builder-relay` | 2B | Gate 3 | 5-8 |
| T-006-16: Room creation tests | `reviewer` | 3 | — | 8-12 |
| T-006-17: Join flow tests | `reviewer` | 3 | — | 8-12 |
| T-006-18: E2E room messaging | `reviewer` | 3 | Gate 4 | 5-8 |

## Execution Phases

### Phase 1: Prerequisites (Lead only)
- **Tasks**: T-006-01, T-006-02, T-006-13, T-006-15
- **Actions**:
  1. T-006-01: Create stub files (`room.rs` in proto, chat, relay), add `pub mod` declarations, add `[[test]]` entry for `room_management`, create integration test stub
  2. T-006-02: Implement `RoomMessage` enum + supporting types + `encode()`/`decode()` in `termchat-proto/src/room.rs`
  3. T-006-13: Write round-trip unit tests for all RoomMessage variants (inline `#[cfg(test)]`)
  4. T-006-15: Create integration test stub with `start_relay()` helper
- **Gate 1**: `cargo build && cargo test -p termchat-proto -- room` passes
- **Output**: Both builders are unblocked

### Phase 2A: Room Client Track (Builder-Room) — runs in parallel with 2B
- **Tasks**: T-006-03 → T-006-04 → T-006-05 + T-006-06 → T-006-07 → T-006-08 + T-006-09
- **Actions**:
  1. T-006-03: `Room` struct, `RoomManager` with CRUD, `RoomError` enum, constants
  2. T-006-04: `validate_room_name()` with sanitization + unit tests
  3. T-006-05: `create_room_full()` pipeline with `RoomEvent` channel
  4. T-006-06: Room limit enforcement, offline creation with pending registrations
  5. T-006-07: Join request queue, `approve_join()`, `deny_join()`
  6. T-006-08: Extension paths — not admin, capacity, duplicate, room not found
  7. T-006-09: `broadcast_to_room()` fan-out, `send_membership_update()`, `send_join_approved()`
- **TDD pattern**: Builder writes inline `#[cfg(test)]` unit tests alongside each component
- **Key guidance**:
  - Follow `ChatManager` pattern for generics over Transport
  - `ConversationId` derivation from RoomId: `ConversationId::from_uuid(Uuid::parse_str(&room_id).unwrap())`
  - Use `termchat_proto::room::{RoomMessage, MemberInfo, RoomInfo, MemberAction}` for wire types
  - `RoomEvent` channel pattern: `mpsc::Sender<RoomEvent>` in RoomManager, returned alongside manager

### Phase 2B: Relay Room Registry Track (Builder-Relay) — runs in parallel with 2A
- **Tasks**: T-006-10 → T-006-11 → T-006-12 → T-006-14
- **Actions**:
  1. T-006-10: `RoomRegistry` struct with `register()`, `unregister()`, `list()`, `get_admin()`, name conflict detection, 1000-room cap
  2. T-006-11: JoinRequest routing to admin PeerId, offline admin queuing, room-not-found error, JoinApproved/JoinDenied forwarding
  3. T-006-12: Inline unit tests for registry CRUD, routing, capacity
  4. T-006-14: Add `RoomRegistry` to `RelayState`, add `RelayMessage::Room(Vec<u8>)` variant, handle in `handle_binary_message()`
- **CRITICAL**: T-006-14 modifies existing relay code. Builder-Relay MUST:
  - Run `cargo test -p termchat-relay` after changes to verify UC-004 tests still pass
  - Use additive changes only — do not restructure existing handler
- **Key guidance**:
  - Follow `RelayState` pattern: `RwLock<HashMap<String, RoomRegistryEntry>>`
  - Use existing `RelayState::get_sender()` for routing to admin
  - Use existing `MessageStore::enqueue()` for offline admin queuing
  - Decode `RoomMessage` from `RelayMessage::Room(bytes)` payload

### Phase 2C: Integration Build Gate (Lead)
- **After**: Both 2A and 2B complete
- **Actions**: Lead runs `cargo build && cargo test` to verify both tracks integrate cleanly
- **Purpose**: Catches cross-track issues before spawning reviewer (lesson from Sprint 4 retrospective)

### Phase 3: Integration Tests (Reviewer)
- **Tasks**: T-006-16, T-006-17, T-006-18
- **Depends on**: Phase 2C passes
- **Actions**:
  1. T-006-16: Room creation + discovery tests — create, list, relay registry, offline, validation, limits
  2. T-006-17: Join flow tests — request, approve, deny, capacity, duplicate, not-admin, offline admin
  3. T-006-18: End-to-end room messaging — fan-out, MembershipUpdate broadcast, ConversationId
- **Test pattern**: Write against postconditions and acceptance criteria, NOT implementation details
- **Gate 4 (Final)**: Full quality gate

## Review Gates

### Gate 1: Protocol Types
- **After**: T-006-02, T-006-13
- **Commands**: `cargo build && cargo test -p termchat-proto -- room`
- **Pass criteria**: All proto types compile, round-trip tests pass
- **On failure**: Lead fixes proto types directly

### Gate 2: Room Client Complete
- **After**: T-006-03 through T-006-09 (all Track A tasks)
- **Reviewer checks**: RoomManager has all methods, validation works, join flow works, fan-out sends
- **Commands**: `cargo test -p termchat -- room && cargo clippy -p termchat -- -D warnings`
- **Pass criteria**: All room unit tests pass, clippy clean
- **On failure**: Lead identifies failures and messages builder-room with fix instructions

### Gate 3: Relay Registry Complete
- **After**: T-006-10 through T-006-14 (all Track B tasks)
- **Reviewer checks**: Room registry works, routing works, existing UC-004 tests still pass
- **Commands**: `cargo test -p termchat-relay && cargo clippy -p termchat-relay -- -D warnings`
- **Pass criteria**: All relay tests pass (existing + new), clippy clean
- **On failure**: Lead identifies failures and messages builder-relay with fix instructions

### Gate 4: Final UC-006 Verification
- **After**: T-006-16, T-006-17, T-006-18
- **Commands**: `cargo fmt --check && cargo clippy -- -D warnings && cargo test && cargo test --test room_management`
- **Pass criteria**: All commands exit 0
- **On failure**: Specific rework tasks assigned to responsible builder

## Parallelization Opportunities

```
Timeline (phases →)

Phase:    1                    2A+2B (parallel)                         2C    3                  4
         ┌──────────────────┐ ┌────────────────────────────────────────┐┌───┐┌────────────────┐ ┌───┐
lead:    │01+02+13+15       │ │  monitoring, gates 2+3                ││int││coord, gate 4   │ │chk│
         └──────────────────┘ └────────────────────────────────────────┘└───┘└────────────────┘ └───┘
                              ┌────────────────────────────────────────┐
b-room:                       │03 → 04 → 05+06 → 07 → 08+09         │ (done)
                              └────────────────────────────────────────┘
                              ┌────────────────────────────────────────┐
b-relay:                      │10 → 11 → 12 → 14                     │ (done)
                              └────────────────────────────────────────┘
                                                                             ┌────────────────┐
reviewer:                                                                    │16 + 17 → 18   │ (done)
                                                                             └────────────────┘
```

**Phases 2A and 2B run simultaneously** — builders work on completely separate files in separate crates. This cuts ~35% off wall-clock time vs. sequential.

**Phase 2C is new** (lesson from Sprint 4 retrospective): explicit integration build after parallel tracks, before reviewer starts. Catches cross-track compile issues early.

## Risk Mitigation

| Risk | Task(s) | Mitigation |
|------|---------|------------|
| Modifying existing relay handler breaks UC-004 | T-006-14 | Builder-Relay runs `cargo test -p termchat-relay` as acceptance check. Use additive `RelayMessage::Room(Vec<u8>)` variant to avoid changing existing dispatch. |
| Fan-out encryption needs Transport generics | T-006-09 | Builder-Room follows `ChatManager<C, T, S>` generic pattern. Accept `&dyn Transport` or make `RoomManager` generic over `T: Transport`. |
| RoomRegistry routing needs RelayState access | T-006-11 | `RoomRegistry` is added as a field on `RelayState` (T-006-14). Builder-Relay can use `Arc<RelayState>` which already holds connections and store. |
| ConversationId derivation inconsistency | T-006-03 | Define deterministic derivation in T-006-03: `ConversationId::from_uuid(Uuid::parse_str(&room_id).unwrap())`. Used consistently by both client and tests. |
| Builder-Room has 7 tasks (more than Builder-Relay's 4) | T-006-03-09 | Tasks 04, 06, 08 are all S/M size. Builder-Room's total LOC (~700-900) is comparable to Builder-Relay's (~400-600). If Builder-Room falls behind, Lead can assist with T-006-06 (simplest extension task). |

## Spawn Commands

```
# 1. Lead completes Phase 1 directly (T-006-01, T-006-02, T-006-13, T-006-15)

# 2. Create the team
TeamCreate: team_name="uc-006-impl", description="UC-006 Create Room"

# 3. Create tasks in shared task list (18 tasks via TaskCreate)

# 4. Spawn BOTH builders simultaneously (parallel Phase 2A + 2B)
Task tool: name="builder-room", team_name="uc-006-impl", subagent_type="general-purpose", model="sonnet", max_turns=25
Task tool: name="builder-relay", team_name="uc-006-impl", subagent_type="general-purpose", model="sonnet", max_turns=25

# 5. After Phase 2C (integration build), spawn reviewer
Task tool: name="reviewer", team_name="uc-006-impl", subagent_type="general-purpose", model="sonnet", max_turns=20

# 6. Lead runs Gate 4, commits
```

## Coordination Notes

- **Strict file ownership**: Builder-Room never touches `termchat-relay/`, Builder-Relay never touches `termchat/src/chat/`. Zero merge conflicts guaranteed.
- **Shared dependency**: Both builders consume `termchat_proto::room::*` (read-only). Lead creates this in Phase 1.
- **Phase 2C integration gate**: New checkpoint (from Sprint 4 retrospective). Lead runs `cargo build && cargo test` after both builders finish, before spawning reviewer. Catches issues like missing `lib.rs` exports.
- **Communication protocol**: Builders message lead after each task completion. Lead runs gate checks and spawns next phase.
- **Commit strategy**: One commit after Gate 4 passes. Lead manages the commit and doc updates.
