# Agent Team Plan: UC-007 Join Room as Agent Participant

Generated on 2026-02-07.

## Design Rationale

UC-007 has 20 tasks across **two parallel tracks**: Track A (agent bridge & protocol in `termchat/src/agent/bridge.rs` + `protocol.rs`) and Track B (room integration & participant in `termchat/src/agent/participant.rs` + `chat/room.rs` + `app.rs` + `ui/`). These tracks have zero file overlap, enabling genuine parallelism — the proven pattern from Sprints 4 and 5.

The shared dependency is `termchat-proto/src/agent.rs` (AgentInfo types) + the `is_agent` field on `MemberInfo` (T-007-02), which Lead completes before spawning builders. Both tracks code against these shared types.

**Key sync point**: T-007-07 (Hello/Welcome handshake) in Track A depends on T-007-09 (RoomManager extensions) from Track B. Builder-Integration should complete T-007-09 early in its sequence so Builder-Bridge can proceed to T-007-07 without blocking.

**Team size: 4** (Lead + 2 Builders + 1 Reviewer). Same composition as UC-004 and UC-006 — third consecutive sprint with this proven pattern. Zero kills, zero conflicts in both previous sprints.

**Model selection**: Sonnet for all teammates. Tasks are well-specified with acceptance criteria. UC-004 and UC-006 proved Sonnet handles this complexity well.

**Max turns**: 25 per builder (6-7 tasks each), 20 for reviewer (4 test tasks). Per retrospective: keeping under 25 prevents context kills.

**Execution strategy**: Lead handles prerequisites (T-007-01, T-007-02), then spawns both builders simultaneously. Reviewer starts after integration gate (T-007-16). Lead runs gate checks between phases.

## Team Composition

| Role | Agent Name | Model | Responsibilities |
|------|-----------|-------|-----------------|
| Lead | `lead` | (current session) | Prerequisites (T-007-01, T-007-02, T-007-16), task routing, review gates, commit |
| Builder-Bridge | `builder-bridge` | Sonnet | Bridge protocol + Unix socket: `termchat/src/agent/protocol.rs`, `termchat/src/agent/bridge.rs`, `termchat/src/agent/mod.rs` (T-007-03 through T-007-08) |
| Builder-Integration | `builder-integration` | Sonnet | Room extensions + participant + app + UI: `termchat/src/agent/participant.rs`, `termchat/src/chat/room.rs`, `termchat/src/app.rs`, `termchat/src/ui/` (T-007-09 through T-007-15) |
| Reviewer | `reviewer` | Sonnet | Integration tests: `tests/integration/agent_bridge.rs` (T-007-17 through T-007-20) |

### File Ownership (strict — no overlap)

| Agent | Owns (exclusive write) |
|-------|----------------------|
| Lead | All `Cargo.toml` files, `*/lib.rs` module declarations, `termchat-proto/src/agent.rs`, `termchat-proto/src/room.rs` (is_agent field only), `CLAUDE.md` |
| Builder-Bridge | `termchat/src/agent/mod.rs` (AgentError), `termchat/src/agent/protocol.rs`, `termchat/src/agent/bridge.rs` |
| Builder-Integration | `termchat/src/agent/participant.rs`, `termchat/src/chat/room.rs`, `termchat/src/app.rs`, `termchat/src/ui/sidebar.rs`, `termchat/src/ui/chat_panel.rs` |
| Reviewer | `tests/integration/agent_bridge.rs` |

## Task Assignment

| Task | Owner | Phase | Review Gate | Est. Turns |
|------|-------|-------|-------------|------------|
| T-007-01: Module stubs + deps | `lead` | 1 | — | 3-4 |
| T-007-02: AgentInfo proto types + is_agent | `lead` | 1 | Gate 1 | 4-6 |
| T-007-03: Bridge protocol types | `builder-bridge` | 2A | — | 5-8 |
| T-007-04: agent_id validation | `builder-bridge` | 2A | — | 3-4 |
| T-007-05: AgentBridge Unix socket I/O | `builder-bridge` | 2A | — | 8-12 |
| T-007-06: Connection lifecycle | `builder-bridge` | 2A | — | 5-8 |
| T-007-07: Hello/Welcome handshake | `builder-bridge` | 2A | — | 8-12 |
| T-007-08: Heartbeat ping/pong | `builder-bridge` | 2A | Gate 2 | 5-8 |
| T-007-09: RoomManager add/remove member | `builder-integration` | 2B | — | 5-8 |
| T-007-10: AgentParticipant fan-out | `builder-integration` | 2B | — | 10-15 |
| T-007-11: Send extensions | `builder-integration` | 2B | — | 3-4 |
| T-007-12: Fan-out extensions | `builder-integration` | 2B | — | 5-8 |
| T-007-13: Disconnect cleanup | `builder-integration` | 2B | — | 5-8 |
| T-007-14: /invite-agent command | `builder-integration` | 2B | — | 5-8 |
| T-007-15: Agent badge UI | `builder-integration` | 2B | Gate 3 | 3-4 |
| T-007-16: Integration build gate | `lead` | 2C | — | 2-3 |
| T-007-17: Stub integration test | `reviewer` | 3 | — | 3-4 |
| T-007-18: Bridge lifecycle tests | `reviewer` | 3 | — | 8-12 |
| T-007-19: Agent participation tests | `reviewer` | 3 | — | 8-12 |
| T-007-20: End-to-end tests | `reviewer` | 3 | Gate 4 | 5-8 |

## Execution Phases

### Phase 1: Prerequisites (Lead only)
- **Tasks**: T-007-01, T-007-02
- **Actions**:
  1. T-007-01: Add `serde_json` dep, create stub files (`agent/mod.rs`, `bridge.rs`, `protocol.rs`, `participant.rs`, proto `agent.rs`), add `pub mod agent;` to `lib.rs` files, add `[[test]]` entry, create integration test stub
  2. T-007-02: Implement `AgentInfo` + `AgentCapability` in proto, add `#[serde(default)] pub is_agent: bool` to `MemberInfo`, add round-trip tests, verify 340 existing tests still pass
- **Gate 1**: `cargo build && cargo test` passes (340+ tests, is_agent backward-compatible)
- **Output**: Both builders are unblocked

### Phase 2A: Bridge & Protocol Track (Builder-Bridge) — runs in parallel with 2B
- **Tasks**: T-007-03 → T-007-04 + T-007-05 → T-007-06 + T-007-07 → T-007-08
- **Actions**:
  1. T-007-03: `AgentMessage` and `BridgeMessage` enums with `#[serde(tag = "type")]`, `encode_line()`/`decode_line()`, helper structs, unit tests
  2. T-007-04: `validate_agent_id()` + `make_unique_agent_peer_id()` with sanitization and conflict resolution
  3. T-007-05: `AgentBridge` struct with Unix socket listener, `AgentConnection` with JSON line read/write, stale socket cleanup, directory creation, permissions
  4. T-007-06: Connection timeout (configurable Duration), multi-connect rejection, socket cleanup on timeout/shutdown
  5. T-007-07: Hello→Welcome handshake — validate Hello, generate PeerId, build Welcome with members + history, capacity check. **NOTE**: This task needs `RoomManager::get_room_members()` and `MessageStore::get_conversation()` — Builder-Integration's T-007-09 should complete first. Builder-Bridge can work on T-007-03→04→05→06 while waiting.
  6. T-007-08: Background heartbeat tokio task, ping/pong with configurable interval, cancellation on disconnect
- **TDD pattern**: Builder writes inline `#[cfg(test)]` unit tests alongside each component
- **Key guidance**:
  - Use `tokio::net::UnixListener` and `tokio::net::UnixStream` for async Unix socket I/O
  - Use `tokio::io::BufReader`/`BufWriter` for line-based JSON I/O
  - Use `serde_json` (not postcard) for bridge protocol — this is a local JSON lines protocol
  - Make timeouts configurable via parameters, not constants — tests need short values (100ms)
  - `AgentError` goes in `mod.rs`, not `bridge.rs` — shared across all agent submodules

### Phase 2B: Room Integration & Participant Track (Builder-Integration) — runs in parallel with 2A
- **Tasks**: T-007-09 → T-007-10 → T-007-11 + T-007-12 + T-007-13 → T-007-14 + T-007-15
- **Actions**:
  1. T-007-09: **DO THIS FIRST** — Add `add_member()`, `remove_member()`, `MemberLeft` event, `MemberNotFound` error to `RoomManager`. This unblocks Builder-Bridge's T-007-07.
  2. T-007-10: `AgentParticipant` with event loop (`tokio::select!`), send fan-out iterating room members, receive forwarding to bridge, readiness state tracking
  3. T-007-11: Size limit check, room-deleted check, not-ready check in `handle_send_message()`
  4. T-007-12: Transport failure handling (continue fan-out, log warning), missing Noise session handling (skip member, log)
  5. T-007-13: `cleanup_agent()` method — single convergence point for graceful, ungraceful, and heartbeat-timeout disconnects. Remove member, broadcast MemberLeft, close connection, cancel heartbeat, remove socket.
  6. T-007-14: Parse `/invite-agent <room-name>` in `app.rs`, validate room, spawn bridge flow, wire events
  7. T-007-15: Agent badge in sidebar (`[A]` prefix), agent message styling in chat panel, join/leave system messages
- **CRITICAL**: T-007-09 must complete early — Builder-Bridge's T-007-07 depends on it. Start with T-007-09 immediately.
- **Key guidance**:
  - `AgentParticipant` does NOT use `ChatManager` directly (it's peer-scoped). Instead, it uses Transport and CryptoSession directly for fan-out, iterating room members from RoomManager.
  - For fan-out: `for member in room.members { if member.peer_id != agent_peer_id { crypto.encrypt(serialized); transport.send(&member_peer_id, &encrypted); } }`
  - Disconnect cleanup must handle all three trigger paths (Goodbye, broken pipe, heartbeat timeout) through a single `cleanup_agent()` to avoid duplication
  - Run `cargo fmt` before marking each task complete (Sprint 5 retro action)

### Phase 2C: Integration Build Gate (Lead)
- **After**: Both 2A and 2B complete
- **Actions**: Lead runs `cargo fmt --check && cargo build && cargo test && cargo clippy -- -D warnings`
- **Purpose**: Catches cross-track issues before spawning reviewer (lesson from Sprint 4 retrospective, proven in Sprint 5)
- **On failure**: Lead identifies issues and routes fixes to the responsible builder

### Phase 3: Integration Tests (Reviewer)
- **Tasks**: T-007-17 → T-007-18 + T-007-19 → T-007-20
- **Depends on**: Phase 2C passes
- **Actions**:
  1. T-007-17: Set up test helpers — `setup_agent_bridge()`, `connect_mock_agent()`, `send_json_line()`/`read_json_line()`
  2. T-007-18: Bridge lifecycle tests — socket creation, handshake, stale socket, timeout, multi-connect, malformed Hello, bad version, invalid agent_id, room full, graceful/ungraceful disconnect, heartbeat timeout
  3. T-007-19: Participation tests — agent sends/receives messages, PeerId prefix, MembershipUpdate, member list, size limit, not-ready, empty/populated history
  4. T-007-20: End-to-end — complete lifecycle, multi-member fan-out, disconnect mid-conversation, re-invite after disconnect
- **Test pattern**: Write against postconditions and acceptance criteria, NOT implementation details. Use short timeouts (100-500ms) for all timing-dependent tests.
- **Gate 4 (Final)**: Full quality gate

## Review Gates

### Gate 1: Proto Types + Stubs
- **After**: T-007-01, T-007-02
- **Commands**: `cargo build && cargo test`
- **Pass criteria**: All crates compile, 340+ tests pass, `is_agent` backward-compatible
- **On failure**: Lead fixes proto types directly

### Gate 2: Bridge Track Complete
- **After**: T-007-03 through T-007-08 (all Track A tasks)
- **Reviewer checks**: Protocol types serialize correctly, Unix socket works, handshake completes, heartbeat runs
- **Commands**: `cargo test -p termchat -- agent && cargo fmt --check && cargo clippy -p termchat -- -D warnings`
- **Pass criteria**: All agent module unit tests pass, fmt + clippy clean
- **On failure**: Lead identifies failures and messages builder-bridge with fix instructions

### Gate 3: Integration Track Complete
- **After**: T-007-09 through T-007-15 (all Track B tasks)
- **Reviewer checks**: RoomManager add/remove works, AgentParticipant fan-out works, /invite-agent command works, UI badges render
- **Commands**: `cargo test -p termchat && cargo fmt --check && cargo clippy -p termchat -- -D warnings`
- **Pass criteria**: All termchat tests pass (existing room tests + new agent tests), fmt + clippy clean
- **On failure**: Lead identifies failures and messages builder-integration with fix instructions

### Gate 4: Final UC-007 Verification
- **After**: T-007-17 through T-007-20
- **Commands**: `cargo fmt --check && cargo clippy -- -D warnings && cargo test && cargo test --test agent_bridge`
- **Pass criteria**: All commands exit 0, all 21 acceptance criteria met
- **On failure**: Specific rework tasks assigned to responsible builder

## Parallelization Opportunities

```
Timeline (phases →)

Phase:    1                    2A+2B (parallel)                              2C    3                      4
         ┌──────────────────┐ ┌──────────────────────────────────────────────┐┌───┐┌──────────────────────┐┌───┐
lead:    │01+02              │ │  monitoring, gates 2+3                      ││int││coord, gate 4         ││chk│
         └──────────────────┘ └──────────────────────────────────────────────┘└───┘└──────────────────────┘└───┘
                              ┌──────────────────────────────────────────────┐
b-bridge:                     │03 → 04+05 → 06 + 07(waits 09) → 08        │ (done)
                              └──────────────────────────────────────────────┘
                              ┌──────────────────────────────────────────────┐
b-integ:                      │09(first!) → 10 → 11+12+13 → 14+15         │ (done)
                              └──────────────────────────────────────────────┘
                                                                                   ┌──────────────────────┐
reviewer:                                                                          │17 → 18+19 → 20      │ (done)
                                                                                   └──────────────────────┘
```

**Phases 2A and 2B run simultaneously** — builders work on completely separate files. Zero merge conflicts guaranteed by file ownership.

**Phase 2C is mandatory** (proven in Sprint 5): explicit integration build after parallel tracks, before reviewer starts.

**Cross-track sync**: Builder-Bridge's T-007-07 needs RoomManager methods from T-007-09. Builder-Integration does T-007-09 first, then Builder-Bridge can proceed to T-007-07 once T-007-09 is marked complete in the task list. Builder-Bridge works on T-007-03→04→05→06 while waiting (~4-6 tasks of independent work).

## Risk Mitigation

| Risk | Task(s) | Mitigation |
|------|---------|------------|
| T-007-07 blocked by T-007-09 (cross-track dependency) | T-007-07, T-007-09 | Builder-Integration explicitly instructed to do T-007-09 first. Builder-Bridge has 4 independent tasks (03, 04, 05, 06) to work on while waiting. |
| AgentParticipant (T-007-10) is XL and high-risk | T-007-10 | If too large for one turn, split event loop setup from fan-out logic. Builder-Integration can message lead for guidance. |
| MemberInfo `is_agent` breaks wire format | T-007-02 | Use `#[serde(default)]` — verified by running all 340 existing tests. |
| Unix socket edge cases (stale sockets, permissions, races) | T-007-05, T-007-06 | Builder-Bridge uses configurable paths (not hardcoded `/tmp`). Tests use unique temp dirs per test. |
| Integration tests timing-sensitive (heartbeat, timeout) | T-007-18 | All timeouts are configurable parameters. Tests use 100-500ms values. Use `tokio::time::pause()` where possible. |
| Builder-Integration has 7 tasks (more than Bridge's 6) | T-007-09-15 | Tasks 11, 15 are S-sized. T-007-09 is straightforward (extends well-tested RoomManager). Effective workload is comparable. |

## Spawn Commands

```
# 1. Lead completes Phase 1 directly (T-007-01, T-007-02)

# 2. Create the team
TeamCreate: team_name="uc-007-impl", description="UC-007 Join Room as Agent Participant"

# 3. Create tasks in shared task list (20 tasks via TaskCreate)

# 4. Spawn BOTH builders simultaneously (parallel Phase 2A + 2B)
Task tool: name="builder-bridge", team_name="uc-007-impl", subagent_type="general-purpose", model="sonnet", max_turns=25
  Prompt: "Claim task T-007-03 immediately from the task list and begin. You own Track A: bridge protocol + Unix socket."

Task tool: name="builder-integration", team_name="uc-007-impl", subagent_type="general-purpose", model="sonnet", max_turns=25
  Prompt: "Claim task T-007-09 immediately from the task list and begin. You own Track B: room integration + participant. T-007-09 MUST complete first — it unblocks Builder-Bridge."

# 5. After Phase 2C (integration build), spawn reviewer
Task tool: name="reviewer", team_name="uc-007-impl", subagent_type="general-purpose", model="sonnet", max_turns=20
  Prompt: "Claim task T-007-17 immediately from the task list and begin. Write integration tests against postconditions."

# 6. Lead runs Gate 4, commits
```

## Coordination Notes

- **Strict file ownership**: Builder-Bridge never touches `participant.rs`, `room.rs`, `app.rs`, or `ui/`. Builder-Integration never touches `protocol.rs` or `bridge.rs`. Zero merge conflicts guaranteed.
- **Shared dependency**: Both builders consume `termchat_proto::agent::*` and `termchat_proto::room::MemberInfo` (read-only). Lead creates these in Phase 1.
- **Cross-track sync point**: T-007-09 (RoomManager extensions) must complete before T-007-07 (Hello/Welcome handshake). Builder-Integration is explicitly instructed to start with T-007-09. Builder-Bridge checks task list status before claiming T-007-07.
- **Phase 2C integration gate**: Mandatory checkpoint (from Sprint 4/5 retrospectives). Lead runs full quality gate after both builders finish, before spawning reviewer.
- **Builders must run `cargo fmt` before marking each task complete** (Sprint 5 retro action).
- **Builders claim tasks immediately on spawn** (Sprint 5 retro action — prevents idle-nudge issue).
- **Communication protocol**: Builders message lead after each task completion. Lead checks task list and runs gate checks.
- **Commit strategy**: One commit after Gate 4 passes. Lead manages the commit and doc updates.
