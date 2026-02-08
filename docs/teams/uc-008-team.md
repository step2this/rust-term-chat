# Agent Team Plan: UC-008 Share Task List

Generated on 2026-02-07.

## Design Rationale

UC-008 has 18 tasks across **three parallel tracks**: Track A (CRDT merge logic + TaskManager in `termchat/src/tasks/`), Track B (UI + App integration in `termchat/src/app.rs` + `ui/`), and Track C (agent bridge in `termchat/src/agent/`). Track C is assigned to Builder-UI since it shares file ownership with Track B (`agent/protocol.rs`, `agent/participant.rs`).

The shared dependency is `termchat-proto/src/task.rs` (TaskId, LwwRegister, Task, TaskSyncMessage types) + the `Envelope::TaskSync` variant (T-008-02), which Lead completes before spawning builders. Both builders code against these shared types.

**Key sync point**: T-008-14 (AgentParticipant task wiring) in Track C depends on T-008-04 (TaskManager) from Track A. Builder-CRDT should complete T-008-03→04 early so Builder-UI can proceed to T-008-14 without blocking. Builder-UI has 5 independent tasks (T-008-07→08→09→10→11→12→13) to work on while waiting.

**Team size: 4** (Lead + 2 Builders + 1 Reviewer). Fourth consecutive sprint with this proven pattern. Zero kills, zero merge conflicts across Sprints 4-6.

**Model selection**: Sonnet for all teammates. Tasks are well-specified with acceptance criteria. Proven across three prior sprints.

**Max turns**: 25 per builder, 20 for reviewer. Per retrospective: keeping under 25 prevents context kills.

**Execution strategy**: Lead handles prerequisites (T-008-01, T-008-02), then spawns both builders simultaneously. Reviewer starts after integration gate (T-008-15). Lead runs gate checks between phases.

## Team Composition

| Role | Agent Name | Model | Responsibilities |
|------|-----------|-------|-----------------|
| Lead | `lead` | (current session) | Prerequisites (T-008-01, T-008-02, T-008-15), task routing, review gates, commit |
| Builder-CRDT | `builder-crdt` | Sonnet | CRDT merge + TaskManager: `termchat/src/tasks/merge.rs`, `termchat/src/tasks/manager.rs`, `termchat/src/tasks/mod.rs` (T-008-03 through T-008-06) |
| Builder-UI | `builder-ui` | Sonnet | UI + App + Agent: `termchat/src/app.rs`, `termchat/src/ui/task_panel.rs`, `termchat/src/ui/mod.rs`, `termchat/src/ui/status_bar.rs`, `termchat/src/agent/protocol.rs`, `termchat/src/agent/participant.rs` (T-008-07 through T-008-14) |
| Reviewer | `reviewer` | Sonnet | Integration tests: `tests/integration/task_sync.rs` (T-008-16 through T-008-18) |

### File Ownership (strict — no overlap)

| Agent | Owns (exclusive write) |
|-------|----------------------|
| Lead | All `Cargo.toml` files, `*/lib.rs` module declarations, `termchat-proto/src/task.rs`, `termchat-proto/src/message.rs` (Envelope variant), `CLAUDE.md` |
| Builder-CRDT | `termchat/src/tasks/mod.rs` (TaskError, TaskEvent, re-exports), `termchat/src/tasks/merge.rs`, `termchat/src/tasks/manager.rs` |
| Builder-UI | `termchat/src/app.rs`, `termchat/src/ui/task_panel.rs`, `termchat/src/ui/mod.rs`, `termchat/src/ui/status_bar.rs`, `termchat/src/agent/protocol.rs`, `termchat/src/agent/participant.rs` |
| Reviewer | `tests/integration/task_sync.rs` |

## Task Assignment

| Task | Owner | Phase | Review Gate | Est. Turns |
|------|-------|-------|-------------|------------|
| T-008-01: Module stubs + deps | `lead` | 1 | — | 3-4 |
| T-008-02: Task proto types + Envelope | `lead` | 1 | Gate 1 | 5-8 |
| T-008-03: LWW merge functions | `builder-crdt` | 2A | — | 8-12 |
| T-008-04: TaskManager CRUD | `builder-crdt` | 2A | — | 8-12 |
| T-008-05: TaskManager extensions | `builder-crdt` | 2A | — | 4-6 |
| T-008-06: TaskManager FullState sync | `builder-crdt` | 2A | Gate 2 | 5-8 |
| T-008-07: PanelFocus::Tasks + DisplayTask | `builder-ui` | 2B | — | 5-8 |
| T-008-08: /task commands | `builder-ui` | 2B | — | 5-8 |
| T-008-09: Command validation extensions | `builder-ui` | 2B | — | 3-4 |
| T-008-10: Keyboard handling (j/k/Enter) | `builder-ui` | 2B | — | 3-5 |
| T-008-11: task_panel.rs rewrite | `builder-ui` | 2B | — | 5-8 |
| T-008-12: status_bar + mod.rs update | `builder-ui` | 2B | — | 2-3 |
| T-008-13: Agent protocol task types | `builder-ui` | 2C | — | 5-8 |
| T-008-14: AgentParticipant task wiring | `builder-ui` | 2C | Gate 3 | 5-8 |
| T-008-15: Integration build gate | `lead` | 3 | — | 2-3 |
| T-008-16: Stub integration test | `reviewer` | 4 | — | 3-4 |
| T-008-17: CRDT + TaskManager tests | `reviewer` | 4 | — | 8-12 |
| T-008-18: E2E task sync tests | `reviewer` | 4 | Gate 4 | 5-8 |

## Execution Phases

### Phase 1: Prerequisites (Lead only)
- **Tasks**: T-008-01, T-008-02
- **Actions**:
  1. T-008-01: Add `pub mod task;` to `termchat-proto/src/lib.rs`, add `pub mod tasks;` to `termchat/src/lib.rs`, create stub files (`termchat-proto/src/task.rs`, `termchat/src/tasks/mod.rs`, `merge.rs`, `manager.rs`), add `[[test]]` entry, create integration test stub
  2. T-008-02: Implement `TaskId`, `LwwRegister<T>`, `TaskStatus`, `Task`, `TaskFieldUpdate`, `TaskSyncMessage`, `encode()`/`decode()` in proto. Add `Envelope::TaskSync(Vec<u8>)` to `message.rs`. Add postcard round-trip tests (~15 tests). Verify all 475 existing tests pass.
- **Gate 1**: `cargo build && cargo test` passes (475+ tests, Envelope::TaskSync backward-compatible)
- **Output**: Both builders are unblocked

### Phase 2A: CRDT Logic Track (Builder-CRDT) — runs in parallel with 2B
- **Tasks**: T-008-03 → T-008-04 → T-008-05 + T-008-06
- **Actions**:
  1. T-008-03: Pure CRDT functions in `merge.rs`: `merge_lww()`, `merge_task()`, `merge_task_list()`, `apply_field_update()`. Extensive unit tests proving commutativity, associativity, idempotency (~20 tests).
  2. T-008-04: `TaskManager` struct with room-scoped task maps, `create_task()`, `update_status()`, `update_assignee()`, `delete_task()`, `apply_remote()`, `get_tasks()`, `build_full_state()`. Define `TaskError` and `TaskEvent` in `mod.rs`. Unit tests (~15 tests).
  3. T-008-05: Extension handling in `apply_remote()` — stale update rejection, unknown TaskId add-wins, malformed bytes. Validation in `create_task()` — empty title, oversized title. Unit tests (~8 tests).
  4. T-008-06: FullState handling in `apply_remote()` — merge task list on receive. `RequestFullState` handling — any member can respond. Unit tests (~5 tests).
- **CRITICAL**: T-008-03→04 must complete early — Builder-UI's T-008-14 depends on TaskManager.
- **TDD pattern**: Builder writes inline `#[cfg(test)]` unit tests alongside each component.
- **Key guidance**:
  - `LwwRegister::merge()`: higher timestamp wins, equal timestamps → higher peer_id (lexicographic) wins, equal everything → local wins (idempotent)
  - `TaskManager` uses `HashMap<String, HashMap<TaskId, Task>>` (room_id → task_id → task)
  - `TaskId` wraps `uuid::Uuid` with UUID v7 for time-ordering
  - `TaskStatus::Deleted` is soft-delete — `get_tasks()` filters these out
  - Use `std::time::SystemTime` for timestamps (milliseconds since epoch)
  - Use `parking_lot::Mutex` if needed (not `std::sync::Mutex`)
  - Run `cargo fmt` and `cargo clippy -p termchat -- -D warnings` before marking each task complete

### Phase 2B: UI & App Track (Builder-UI) — runs in parallel with 2A
- **Tasks**: T-008-07 → T-008-08 + T-008-10 + T-008-11 → T-008-09 + T-008-12 → T-008-13 → T-008-14
- **Actions**:
  1. T-008-07: Add `Tasks` to `PanelFocus`, `DisplayTask` struct, `TaskDisplayStatus` enum, task state fields to `App`, update focus cycling. Unit tests (~4 tests).
  2. T-008-08: `/task add|done|assign|delete|list` commands in `handle_command()`. System messages for each. Unit tests (~10 tests).
  3. T-008-10: `handle_tasks_key()` for j/k/Up/Down navigation and Enter status toggle. Empty panel no-op. Unit tests (~6 tests).
  4. T-008-11: Rewrite `task_panel.rs` — `render(frame, area, app)`, status indicators `[ ]`/`[~]`/`[x]`, task numbers, assignees, focus highlighting, empty state placeholder.
  5. T-008-09: Validation error messages in `handle_task_command()`. Unit tests (~5 tests).
  6. T-008-12: Add Tasks focus help text in `status_bar.rs`. Update `task_panel::render()` call in `ui/mod.rs` to pass `&app`.
  7. T-008-13: Add `CreateTask`, `UpdateTaskStatus`, `AssignTask`, `ListTasks` to `AgentMessage`. Add `TaskList`, `TaskUpdate`, `TaskDeleted` to `BridgeMessage`. Add `BridgeTaskInfo` struct. Serialization tests (~8 tests).
  8. T-008-14: Wire task message handling in `AgentParticipant` — capability check, task CRUD via outbound channel, `TaskEvent` forwarding to agent. Unit tests (~6 tests).
- **CRITICAL**: T-008-14 requires TaskManager from Builder-CRDT's T-008-04. Builder-UI has 6 tasks (07→08→09→10→11→12→13) to work on before needing T-008-04.
- **Key guidance**:
  - `PanelFocus` cycle order: Input → Sidebar → Chat → Tasks → Input
  - `DisplayTask` is a UI-layer struct (in `app.rs`), decoupled from proto `Task` — conversion happens at the manager boundary
  - Task panel rewrite changes signature from `render(frame, area)` to `render(frame, area, app)` — both files (`task_panel.rs` and `ui/mod.rs`) are Builder-UI's responsibility
  - Follow `/invite-agent` command pattern for `/task` command dispatch
  - `AgentMessage`/`BridgeMessage` use `#[serde(tag = "type", rename_all = "snake_case")]`
  - For T-008-14: use `AgentCapability::TaskManagement` (already exists in proto) for capability gate
  - Run `cargo fmt` and `cargo clippy -p termchat -- -D warnings` before marking each task complete

### Phase 3: Integration Build Gate (Lead)
- **After**: Both 2A and 2B complete (all T-008-03 through T-008-14)
- **Actions**: Lead runs `cargo fmt --check && cargo build && cargo test && cargo clippy -- -D warnings`
- **Purpose**: Catches cross-track issues before spawning reviewer (proven mandatory in Sprints 4-6)
- **On failure**: Lead identifies issues and routes fixes to the responsible builder

### Phase 4: Integration Tests (Reviewer)
- **Tasks**: T-008-16 → T-008-17 → T-008-18
- **Depends on**: Phase 3 passes
- **Actions**:
  1. T-008-16: Set up test helpers — `create_task_manager(peer_id)`, `create_test_task(manager, room_id, title)`, placeholder test
  2. T-008-17: CRDT merge tests (~12) — LWW convergence, add-wins, concurrent title edit, independent field merge, stale rejection, delete propagation, FullState reconciliation, empty room, task ordering
  3. T-008-18: E2E tests (~8) — complete lifecycle, two-peer sync, agent task CRUD, capability gate, concurrent edit resolution, delete + concurrent edit
- **Test pattern**: Write against postconditions and acceptance criteria, NOT implementation details. Import from `termchat::tasks` and `termchat_proto::task`.
- **Gate 4 (Final)**: Full quality gate

## Review Gates

### Gate 1: Proto Types + Stubs
- **After**: T-008-01, T-008-02
- **Commands**: `cargo build && cargo test`
- **Pass criteria**: All crates compile, 475+ tests pass, Envelope::TaskSync variant added
- **On failure**: Lead fixes proto types directly

### Gate 2: CRDT Track Complete
- **After**: T-008-03 through T-008-06 (all Track A tasks)
- **Reviewer checks**: LWW merge correctness (commutativity, associativity, idempotency), TaskManager CRUD, extension handling, FullState sync
- **Commands**: `cargo test -p termchat -- tasks && cargo fmt --check && cargo clippy -p termchat -- -D warnings`
- **Pass criteria**: All tasks module unit tests pass, fmt + clippy clean
- **On failure**: Lead identifies failures and messages builder-crdt with fix instructions

### Gate 3: UI + Agent Track Complete
- **After**: T-008-07 through T-008-14 (all Track B + C tasks)
- **Reviewer checks**: Focus cycling includes Tasks, /task commands work, keyboard navigation works, task panel renders, agent bridge task messages
- **Commands**: `cargo test -p termchat && cargo fmt --check && cargo clippy -p termchat -- -D warnings`
- **Pass criteria**: All termchat tests pass (existing + new), fmt + clippy clean
- **On failure**: Lead identifies failures and messages builder-ui with fix instructions

### Gate 4: Final UC-008 Verification
- **After**: T-008-16 through T-008-18
- **Commands**: `cargo fmt --check && cargo clippy -- -D warnings && cargo test && cargo test --test task_sync`
- **Pass criteria**: All commands exit 0, all 13 acceptance criteria met
- **On failure**: Specific rework tasks assigned to responsible builder

## Parallelization Opportunities

```
Timeline (phases →)

Phase:    1                    2A+2B (parallel)                              3     4                      Gate4
         ┌──────────────────┐ ┌──────────────────────────────────────────────┐┌───┐┌──────────────────────┐┌───┐
lead:    │01+02              │ │  monitoring, gates 2+3                      ││int││coord, gate 4         ││chk│
         └──────────────────┘ └──────────────────────────────────────────────┘└───┘└──────────────────────┘└───┘
                              ┌──────────────────────────────────────────────┐
b-crdt:                       │03 → 04 → 05 + 06                           │ (done)
                              └──────────────────────────────────────────────┘
                              ┌──────────────────────────────────────────────┐
b-ui:                         │07 → 08+10+11 → 09+12 → 13 → 14(waits 04) │ (done)
                              └──────────────────────────────────────────────┘
                                                                                   ┌──────────────────────┐
reviewer:                                                                          │16 → 17 → 18         │ (done)
                                                                                   └──────────────────────┘
```

**Phases 2A and 2B run simultaneously** — builders work on completely separate files. Zero merge conflicts guaranteed by file ownership.

**Phase 3 is mandatory** (proven across Sprints 4-6): explicit integration build after parallel tracks, before reviewer starts.

**Cross-track sync**: Builder-UI's T-008-14 needs TaskManager from T-008-04. Builder-CRDT does T-008-03→04 as its first two tasks. Builder-UI works on T-008-07→08→10→11→09→12→13 (7 independent tasks) before needing T-008-04 — plenty of buffer.

## Risk Mitigation

| Risk | Task(s) | Mitigation |
|------|---------|------------|
| T-008-14 blocked by T-008-04 (cross-track dependency) | T-008-14, T-008-04 | Builder-CRDT explicitly instructed to do T-008-03→04 first. Builder-UI has 7 independent tasks to work on while waiting. |
| LWW merge correctness (T-008-03) is the highest-risk task | T-008-03 | Extensive unit tests (~20) proving commutativity, associativity, idempotency. Reviewer writes independent convergence tests. |
| Builder-UI has 8 tasks vs Builder-CRDT's 4 | T-008-07-14 | Tasks 09, 10, 12 are S-sized (2-4 turns each). Effective workload is comparable (~25 turns each). |
| task_panel.rs signature change breaks ui/mod.rs | T-008-11, T-008-12 | Both files are Builder-UI's responsibility — no cross-agent impact. |
| Envelope::TaskSync may break existing envelope tests | T-008-02 | Lead adds variant with test coverage. Postcard serialization handles new variants backward-compatibly. |
| AgentParticipant changes may conflict with UC-007 code | T-008-14 | Builder-UI only adds match arms for new AgentMessage variants — doesn't modify existing UC-007 logic. |

## Spawn Commands

```
# 1. Lead completes Phase 1 directly (T-008-01, T-008-02)

# 2. Create the team
TeamCreate: team_name="uc-008-impl", description="UC-008 Share Task List"

# 3. Create tasks in shared task list (18 tasks via TaskCreate)

# 4. Spawn BOTH builders simultaneously (parallel Phase 2A + 2B)
Task tool: name="builder-crdt", team_name="uc-008-impl", subagent_type="general-purpose", model="sonnet", max_turns=25
  Prompt: "Claim task #3 (T-008-03) immediately from the task list and begin. You own Track A: CRDT merge logic + TaskManager in termchat/src/tasks/."

Task tool: name="builder-ui", team_name="uc-008-impl", subagent_type="general-purpose", model="sonnet", max_turns=25
  Prompt: "Claim task #7 (T-008-07) immediately from the task list and begin. You own Track B+C: UI, App, and Agent in termchat/src/app.rs, ui/, agent/."

# 5. After Phase 3 (integration build), spawn reviewer
Task tool: name="reviewer", team_name="uc-008-impl", subagent_type="general-purpose", model="sonnet", max_turns=20
  Prompt: "Claim task #16 (T-008-16) immediately from the task list and begin. Write integration tests against postconditions."

# 6. Lead runs Gate 4, commits
```

## Coordination Notes

- **Strict file ownership**: Builder-CRDT never touches `app.rs`, `ui/`, or `agent/`. Builder-UI never touches `tasks/merge.rs` or `tasks/manager.rs`. Zero merge conflicts guaranteed.
- **Shared dependency**: Both builders consume `termchat_proto::task::*` types (read-only). Lead creates these in Phase 1.
- **Cross-track sync point**: T-008-04 (TaskManager) must complete before T-008-14 (AgentParticipant wiring). Builder-CRDT is instructed to do T-008-03→04 first. Builder-UI checks task list status before claiming T-008-14.
- **Phase 3 integration gate**: Mandatory checkpoint (proven across Sprints 4-6). Lead runs full quality gate after both builders finish, before spawning reviewer.
- **Builders must run `cargo fmt` and `cargo clippy -p <crate> -- -D warnings` before marking each task complete** (Sprint 6 retro action).
- **Builders claim tasks immediately on spawn** (Sprint 5 retro action — prevents idle-nudge issue).
- **Communication protocol**: Builders message lead after each task completion. Lead checks task list and runs gate checks.
- **Commit strategy**: One commit after Gate 4 passes. Lead manages the commit and doc updates.
