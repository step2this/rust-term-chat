# Tasks for UC-008: Share Task List

Generated from use case on 2026-02-07.

## Summary
- **Total tasks**: 18
- **Implementation tasks**: 12
- **Test tasks**: 3
- **Prerequisite tasks**: 2
- **Refactor tasks**: 1
- **Critical path**: T-008-01 → T-008-02 → T-008-03 → T-008-05 → T-008-06 → T-008-07 → T-008-14 → T-008-18
- **Estimated total size**: XL (collectively ~1500-2000 lines across 2 crates)

## Dependency Graph

```
T-008-01 (Lead: Add deps, wire up modules, stubs)
  ├── T-008-02 (Lead: Task proto types — TaskId, LwwRegister, Task, TaskSyncMessage, Envelope variant)
  │     │
  │     ├─── Track A: CRDT Logic & TaskManager ──────────
  │     │  T-008-03 (LWW merge functions: merge_lww, merge_task, merge_task_list, apply_field_update)
  │     │    └── T-008-04 (TaskManager: create, update_status, update_assignee, delete, apply_remote, get_tasks, build_full_state)
  │     │          ├── T-008-05 (TaskManager extensions: stale update rejection, unknown TaskId add-wins, malformed bytes)
  │     │          └── T-008-06 (TaskManager FullState sync: request, respond, merge on join)
  │     │
  │     ├─── Track B: UI & App Integration ──────────────
  │     │  T-008-07 (App: PanelFocus::Tasks, DisplayTask, task state, focus cycling)
  │     │    ├── T-008-08 (App: /task commands — add, done, assign, delete, list)
  │     │    │     └── T-008-09 (App: command extensions — empty title, oversized, not found, invalid assignee)
  │     │    ├── T-008-10 (App: keyboard handling — j/k navigate, Enter toggle status in Task Panel)
  │     │    └── T-008-11 (UI: task_panel.rs rewrite — interactive rendering from &App)
  │     │          └── T-008-12 (UI: status_bar Tasks help text + ui/mod.rs pass &app)
  │     │
  │     └─── Track C: Agent Bridge ──────────────────────
  │        T-008-13 (Agent protocol: task message types in AgentMessage + BridgeMessage)
  │          └── T-008-14 (AgentParticipant: handle task messages, capability check)
  │
  │  After Track A + B + C complete:
  │  T-008-15 (Lead: Integration build gate — cargo build && cargo test && cargo clippy)
  │
  └── T-008-01 also blocks:
        T-008-16 (Reviewer: Stub integration test file + helpers)

After all tracks + gate complete:
  T-008-16 → T-008-17 (Reviewer: CRDT merge + TaskManager integration tests)
  T-008-17 → T-008-18 (Reviewer: End-to-end task sync + agent task management tests)
```

## Tasks

### T-008-01: Add dependencies, wire up modules, create stubs
- **Type**: Prerequisite
- **Module**: `Cargo.toml` files, `termchat-proto/src/lib.rs`, `termchat/src/lib.rs`
- **Description**:
  - Add `pub mod task;` to `termchat-proto/src/lib.rs`
  - Add `pub mod tasks;` to `termchat/src/lib.rs`
  - Create stub files: `termchat-proto/src/task.rs`, `termchat/src/tasks/mod.rs`, `termchat/src/tasks/merge.rs`, `termchat/src/tasks/manager.rs`
  - Add `[[test]] name = "task_sync"` entry to `termchat/Cargo.toml`
  - Create stub `tests/integration/task_sync.rs`
  - Verify: `cargo build` succeeds across all crates
- **From**: Implementation Notes (new files + modified files)
- **Depends On**: None
- **Blocks**: T-008-02, T-008-16
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Lead
- **Acceptance Test**: `cargo build` succeeds, `cargo test` still passes (475 tests)

---

### T-008-02: Define task proto types and add Envelope::TaskSync
- **Type**: Implementation
- **Module**: `termchat-proto/src/task.rs`, `termchat-proto/src/message.rs`
- **Description**:
  - Implement `TaskId` newtype wrapping `Uuid` (same pattern as `MessageId` in message.rs)
  - Implement `LwwRegister<T>` generic struct with `value: T`, `timestamp: u64`, `author: String`
  - Implement `TaskStatus` enum: `Open`, `InProgress`, `Completed`, `Deleted`
  - Implement `Task` struct with `id: TaskId`, `room_id: String`, `title: LwwRegister<String>`, `status: LwwRegister<TaskStatus>`, `assignee: LwwRegister<Option<String>>`, `created_at: u64`, `created_by: String`
  - Implement `TaskFieldUpdate` enum with variants for each field
  - Implement `TaskSyncMessage` enum: `FieldUpdate`, `FullState`, `RequestFullState`
  - Add `encode()` / `decode()` functions following `room.rs` pattern
  - Add `Envelope::TaskSync(Vec<u8>)` variant to `message.rs`
  - Add postcard round-trip unit tests for all types (~15 tests)
  - Verify all 475 existing tests still pass
- **From**: Implementation Notes (proto types), MSS steps 3, 5-6, 9-10
- **Depends On**: T-008-01
- **Blocks**: T-008-03, T-008-07, T-008-13
- **Size**: M
- **Risk**: Low (follows established proto pattern)
- **Agent Assignment**: Lead
- **Acceptance Test**: `cargo test -p termchat-proto` passes, existing tests pass unchanged

---

### T-008-03: Implement LWW merge functions
- **Type**: Implementation
- **Module**: `termchat/src/tasks/merge.rs`
- **Description**:
  - Implement `merge_lww<T: Clone>(local: &LwwRegister<T>, remote: &LwwRegister<T>) -> LwwRegister<T>`:
    - Higher timestamp wins
    - Equal timestamps: higher author (lexicographic) wins
    - Equal everything: local wins (idempotent)
  - Implement `merge_task(local: &mut Task, remote: &Task)`:
    - Merge each field independently using `merge_lww`
  - Implement `merge_task_list(local: &mut HashMap<TaskId, Task>, remote: &[Task])`:
    - For each remote task: if exists locally, merge fields; if new, add (add-wins)
  - Implement `apply_field_update(task: &mut Task, update: &TaskFieldUpdate) -> bool`:
    - Returns true if update was applied (newer), false if rejected (stale)
  - Extensive unit tests (~20 tests):
    - Later timestamp wins, earlier loses
    - Equal timestamp tie-break by peer_id
    - Idempotent merge (same data)
    - Independent field merge (different fields both survive)
    - Add-wins for new tasks
    - Empty local + full remote = remote state
    - Full local + empty remote = local unchanged
    - apply_field_update newer wins, stale rejected
- **From**: MSS Part H (steps 39-42), Invariants 2-3, Extensions 11a, 11b
- **Depends On**: T-008-02
- **Blocks**: T-008-04
- **Size**: L
- **Risk**: Medium (core CRDT logic — must be mathematically correct)
- **Agent Assignment**: Builder-CRDT
- **Acceptance Test**: All merge tests pass; commutativity/associativity/idempotency proven by tests

---

### T-008-04: Implement TaskManager
- **Type**: Implementation
- **Module**: `termchat/src/tasks/manager.rs`, `termchat/src/tasks/mod.rs`
- **Description**:
  - Define `TaskError` in `mod.rs`: `TitleEmpty`, `TitleTooLong`, `TaskNotFound(String)`, `RoomNotFound(String)`, `InvalidAssignee(String)`
  - Define `TaskEvent` in `mod.rs`: `TaskCreated`, `TaskUpdated`, `TaskDeleted`, `FullSyncCompleted`
  - Implement `TaskManager` struct:
    - `tasks: HashMap<String, HashMap<TaskId, Task>>` (room_id → task map)
    - `local_peer_id: String`
    - `event_tx: mpsc::Sender<TaskEvent>`
  - Methods:
    - `create_task(room_id, title) -> Result<(Task, TaskSyncMessage), TaskError>` — validates title, creates Task, returns sync message
    - `update_status(room_id, task_id, new_status) -> Result<TaskSyncMessage, TaskError>`
    - `update_assignee(room_id, task_id, assignee) -> Result<TaskSyncMessage, TaskError>`
    - `delete_task(room_id, task_id) -> Result<TaskSyncMessage, TaskError>` — sets status to Deleted
    - `apply_remote(msg: &TaskSyncMessage)` — dispatches to merge logic
    - `get_tasks(room_id) -> Vec<&Task>` — returns tasks sorted by created_at, excludes Deleted
    - `build_full_state(room_id) -> Option<TaskSyncMessage>` — builds FullState for sync
  - Unit tests (~15 tests)
- **From**: MSS Parts A-E, TaskManager in Implementation Notes
- **Depends On**: T-008-03
- **Blocks**: T-008-05, T-008-06
- **Size**: L
- **Risk**: Medium (state management, event emission)
- **Agent Assignment**: Builder-CRDT
- **Acceptance Test**: TaskManager CRUD operations work; sync messages generated correctly

---

### T-008-05: TaskManager extensions (stale updates, unknown TaskId, malformed bytes)
- **Type**: Implementation
- **Module**: `termchat/src/tasks/manager.rs`
- **Description**:
  - In `apply_remote()`:
    - Extension 11a: Stale FieldUpdate (older timestamp) silently rejected by merge
    - Extension 11b: Unknown TaskId → create task from update (add-wins)
    - Extension 10a: Malformed bytes → log warning, return error (caller handles)
  - In `create_task()`:
    - Extension 2a: Empty title → `TaskError::TitleEmpty`
    - Extension 2b: Title > 256 chars → `TaskError::TitleTooLong`
  - Unit tests for each extension path (~8 tests)
- **From**: Extensions 2a, 2b, 10a, 11a, 11b
- **Depends On**: T-008-04
- **Blocks**: T-008-15
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Builder-CRDT
- **Acceptance Test**: All extension error paths produce correct errors or merge behavior

---

### T-008-06: TaskManager FullState sync (request, respond, merge on join)
- **Type**: Implementation
- **Module**: `termchat/src/tasks/manager.rs`
- **Description**:
  - In `apply_remote()`:
    - Handle `TaskSyncMessage::FullState`: call `merge_task_list` for full reconciliation
    - Handle `TaskSyncMessage::RequestFullState`: caller should respond with `build_full_state()`
  - Extension 30a: Any member can respond (not just admin) — `build_full_state` works for any peer
  - Unit tests: FullState merge with empty local, FullState merge with existing tasks (LWW resolves conflicts), RequestFullState returns correct data
  - ~5 tests
- **From**: MSS Part F (steps 29-33), Extension 30a
- **Depends On**: T-008-04
- **Blocks**: T-008-15
- **Size**: M
- **Risk**: Low
- **Agent Assignment**: Builder-CRDT
- **Acceptance Test**: New peer receives FullState and has complete task list; conflicts resolved by LWW

---

### T-008-07: App: PanelFocus::Tasks, DisplayTask, task state, focus cycling
- **Type**: Refactor
- **Module**: `termchat/src/app.rs`
- **Description**:
  - Add `Tasks` variant to `PanelFocus` enum
  - Add `DisplayTask` struct: `id: String`, `title: String`, `status: TaskDisplayStatus`, `assignee: Option<String>`, `number: usize`
  - Add `TaskDisplayStatus` enum: `Open`, `InProgress`, `Completed`
  - Add to `App` struct: `tasks: Vec<DisplayTask>`, `selected_task: usize`, `task_scroll: usize`
  - Update `cycle_focus_forward`: Input → Sidebar → Chat → Tasks → Input
  - Update `cycle_focus_backward`: Input → Tasks → Chat → Sidebar → Input
  - Update all `PanelFocus` match arms to include `Tasks` variant
  - Unit tests for focus cycling (~4 tests)
- **From**: MSS step 14, Postcondition 6
- **Depends On**: T-008-02
- **Blocks**: T-008-08, T-008-10, T-008-11
- **Size**: M
- **Risk**: Low (extends existing well-tested App)
- **Agent Assignment**: Builder-UI
- **Acceptance Test**: Focus cycles through all 4 panels; `Tasks` variant handled in all match arms

---

### T-008-08: App: /task commands (add, done, assign, delete, list)
- **Type**: Implementation
- **Module**: `termchat/src/app.rs`
- **Description**:
  - Add `/task` to `handle_command()` dispatch (follows `/invite-agent` pattern)
  - Implement `handle_task_command(args)` with subcommand dispatch:
    - `/task add <title>` — creates DisplayTask, pushes system message "Task created: <title>"
    - `/task done <number>` — sets status to Completed, pushes system message
    - `/task assign <number> @<name>` — sets assignee, pushes system message
    - `/task delete <number>` — removes task, pushes system message
    - `/task list` — pushes task list as system messages
    - Unknown subcommand: show usage help
  - Unit tests for each command variant (~10 tests)
- **From**: MSS Parts A, C-alt, D, E; Extensions 2a, 2b, 20a, 20b, 25a
- **Depends On**: T-008-07
- **Blocks**: T-008-09
- **Size**: M
- **Risk**: Low (follows existing command pattern)
- **Agent Assignment**: Builder-UI
- **Acceptance Test**: All /task commands produce correct system messages; invalid input shows errors

---

### T-008-09: App: command extensions (validation errors)
- **Type**: Implementation
- **Module**: `termchat/src/app.rs`
- **Description**:
  - In `handle_task_command()`:
    - Extension 2a: Empty title → "Task title cannot be empty"
    - Extension 2b: Title > 256 chars → "Task title too long (max 256 characters)"
    - Extension 20a: Task number not found → "Task #N not found"
    - Extension 20b: Assignee not a room member → "'name' is not a member of this room"
    - Extension 25a: Delete task not found → "Task #N not found"
  - Unit tests for each error path (~5 tests)
- **From**: Extensions 2a, 2b, 20a, 20b, 25a
- **Depends On**: T-008-08
- **Blocks**: T-008-15
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Builder-UI
- **Acceptance Test**: All validation errors show correct messages

---

### T-008-10: App: keyboard handling for Task Panel
- **Type**: Implementation
- **Module**: `termchat/src/app.rs`
- **Description**:
  - Implement `handle_tasks_key(key)` method:
    - `j` or `Down`: select next task (wrap around or clamp)
    - `k` or `Up`: select previous task
    - `Enter`: toggle selected task status (Open → InProgress → Completed → Open)
    - Extension 16a: If no tasks, Enter does nothing
  - Wire `handle_tasks_key` into `handle_key_event()` for `PanelFocus::Tasks`
  - Unit tests: navigation wrapping, status toggle cycle, empty panel no-op (~6 tests)
- **From**: MSS Part C (steps 14-19), Extension 16a
- **Depends On**: T-008-07
- **Blocks**: T-008-15
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Builder-UI
- **Acceptance Test**: j/k navigates, Enter toggles status, empty panel does nothing

---

### T-008-11: UI: task_panel.rs rewrite (interactive rendering from &App)
- **Type**: Implementation
- **Module**: `termchat/src/ui/task_panel.rs`
- **Description**:
  - Replace hardcoded demo data with real rendering from `&App`:
    - `pub fn render(frame: &mut Frame, area: Rect, app: &App)`
    - Iterate `app.tasks`, render each with status indicator:
      - `[ ]` for Open, `[~]` for InProgress, `[x]` for Completed
    - Show task number, title, assignee if present: `#1 [ ] Fix bug (@alice)`
    - Highlight selected task when `PanelFocus::Tasks`
    - Show focus border when task panel is focused
    - Extension 16a: Empty state placeholder "No tasks yet. Use /task add <title> to create one."
  - Import `App` type from `crate::app`
- **From**: MSS steps 7, 13, 19, 24; Postcondition 6; Extension 16a
- **Depends On**: T-008-07
- **Blocks**: T-008-12
- **Size**: M
- **Risk**: Low (replaces existing file, follows chat_panel pattern)
- **Agent Assignment**: Builder-UI
- **Acceptance Test**: Task panel renders real tasks with correct status indicators; empty state shows placeholder

---

### T-008-12: UI: status_bar help text + mod.rs pass &app
- **Type**: Implementation
- **Module**: `termchat/src/ui/status_bar.rs`, `termchat/src/ui/mod.rs`
- **Description**:
  - In `status_bar.rs`: Add `PanelFocus::Tasks` help text: "Tab: switch panel | ↑↓/jk: navigate | Enter: toggle status | Esc: quit"
  - In `mod.rs`: Update `task_panel::render()` call to pass `&app` (currently only passes `frame` and `area`)
- **From**: Postcondition 6, status bar convention
- **Depends On**: T-008-11
- **Blocks**: T-008-15
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Builder-UI
- **Acceptance Test**: Status bar shows correct help for Tasks focus; task panel renders without error

---

### T-008-13: Agent protocol: task message types
- **Type**: Implementation
- **Module**: `termchat/src/agent/protocol.rs`
- **Description**:
  - Add to `AgentMessage` enum (with `#[serde(tag = "type", rename_all = "snake_case")]`):
    - `CreateTask { title: String }`
    - `UpdateTaskStatus { task_id: String, status: String }`
    - `AssignTask { task_id: String, assignee: String }`
    - `ListTasks`
  - Add to `BridgeMessage` enum:
    - `TaskList { room_id: String, tasks: Vec<BridgeTaskInfo> }`
    - `TaskUpdate { room_id: String, task: BridgeTaskInfo }`
    - `TaskDeleted { room_id: String, task_id: String }`
  - Add `BridgeTaskInfo` struct: `task_id: String`, `title: String`, `status: String`, `assignee: Option<String>`, `created_by: String`
  - Unit tests: serialization round-trips for all new variants (~8 tests)
- **From**: MSS Part G (steps 34-38), Agent Bridge Task Protocol spec
- **Depends On**: T-008-02
- **Blocks**: T-008-14
- **Size**: M
- **Risk**: Low (extends existing tagged enum pattern)
- **Agent Assignment**: Builder-UI
- **Acceptance Test**: All new message types serialize to expected JSON; round-trip tests pass

---

### T-008-14: AgentParticipant: handle task messages, capability check
- **Type**: Implementation
- **Module**: `termchat/src/agent/participant.rs`
- **Description**:
  - In the event loop's agent message handling:
    - `CreateTask`: check `TaskManagement` capability (ext 35a), create task via outbound channel
    - `UpdateTaskStatus`: validate status string, forward to TaskManager
    - `AssignTask`: forward to TaskManager
    - `ListTasks`: build `BridgeMessage::TaskList` from TaskManager, send to agent
  - Extension 35a: If agent lacks `TaskManagement` capability, send error `{"type": "error", "code": "capability_required", ...}`
  - Forward `TaskEvent`s to agent as `BridgeMessage::TaskUpdate` / `TaskDeleted`
  - Unit tests: capability check, task creation via bridge, list tasks response (~6 tests)
- **From**: MSS Part G, Extension 35a
- **Depends On**: T-008-13, T-008-04
- **Blocks**: T-008-15
- **Size**: M
- **Risk**: Medium (cross-module integration: agent ↔ tasks)
- **Agent Assignment**: Builder-UI
- **Acceptance Test**: Agent with TaskManagement can CRUD tasks; agent without capability gets error

---

### T-008-15: Integration build gate
- **Type**: Prerequisite
- **Module**: Workspace root
- **Description**:
  - Run `cargo fmt` on all crates
  - Run `cargo build` — verify no compilation errors
  - Run `cargo test` — verify all existing tests still pass (475+)
  - Run `cargo clippy -- -D warnings` — verify no new warnings
  - Fix any cross-track compilation issues before reviewer proceeds
- **From**: Sprint 6 retrospective (Phase 2C gate)
- **Depends On**: T-008-05, T-008-06, T-008-09, T-008-10, T-008-12, T-008-14
- **Blocks**: T-008-17, T-008-18
- **Size**: S
- **Risk**: Low (gate, not implementation)
- **Agent Assignment**: Lead
- **Acceptance Test**: `cargo fmt --check && cargo clippy -- -D warnings && cargo test` all pass

---

### T-008-16: Create stub integration test file + helpers
- **Type**: Test
- **Module**: `tests/integration/task_sync.rs`
- **Description**:
  - Set up integration test file with:
    - Imports for task types, TaskManager, merge functions
    - Helper: `create_task_manager(peer_id)` → returns TaskManager with event channel
    - Helper: `create_test_task(manager, room_id, title)` → creates a task, returns it
    - Placeholder test: `#[tokio::test] async fn placeholder() {}`
  - Verify `cargo test --test task_sync` compiles and placeholder passes
- **From**: Agent Execution Notes (Test File)
- **Depends On**: T-008-01
- **Blocks**: T-008-17
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Reviewer
- **Acceptance Test**: `cargo test --test task_sync` compiles and runs

---

### T-008-17: CRDT merge + TaskManager integration tests
- **Type**: Test
- **Module**: `tests/integration/task_sync.rs`
- **Description**: Test CRDT merge correctness and TaskManager operations:
  - **LWW convergence**: Two managers create conflicting updates, merge FullState, assert convergence
  - **Add-wins**: Manager A creates task, Manager B doesn't have it, B receives FullState, B has the task
  - **Concurrent title edit**: A edits title at T1, B edits title at T2 (T2 > T1), both merge, both have B's title
  - **Independent field merge**: A edits title, B edits status (same task), both merge, both fields survive
  - **Stale update rejection**: Send older FieldUpdate to manager with newer version, verify no change
  - **Delete propagation**: A deletes task, B receives FieldUpdate with Deleted status, B's task removed from get_tasks()
  - **FullState reconciliation**: Two managers with divergent state, exchange FullState, both converge
  - **Empty room tasks**: get_tasks on room with no tasks returns empty vec
  - **Task ordering**: Tasks returned by get_tasks() sorted by created_at
  - ~12 tests
- **From**: Postconditions 4, 8; Invariants 2, 3; MSS Part H; Extensions 11a, 11b, 28a
- **Depends On**: T-008-15, T-008-16
- **Blocks**: T-008-18
- **Size**: L
- **Risk**: Medium (CRDT correctness verification)
- **Agent Assignment**: Reviewer
- **Acceptance Test**: All merge tests pass

---

### T-008-18: End-to-end task sync + agent task management tests
- **Type**: Test
- **Module**: `tests/integration/task_sync.rs`
- **Description**: Full-lifecycle integration tests:
  - **Complete lifecycle**: Create task → update status → assign → delete → verify state
  - **Two-peer sync**: Peer A creates task, syncs to Peer B via FullState, B sees task
  - **Agent task CRUD**: Agent sends create_task, update_task_status, assign_task, list_tasks via bridge protocol mock, verify correct BridgeMessage responses
  - **Agent capability gate**: Agent without TaskManagement sends create_task, verify error response
  - **Concurrent edit resolution E2E**: A and B both update same task, exchange FieldUpdates, both converge
  - **Delete + concurrent edit**: A deletes task, B updates task title, both merge, task is deleted
  - ~8 tests
- **From**: Acceptance Criteria (end-to-end); MSS Parts A-H
- **Depends On**: T-008-17
- **Blocks**: None
- **Size**: L
- **Risk**: Medium (multi-actor async scenarios)
- **Agent Assignment**: Reviewer
- **Acceptance Test**: All end-to-end tests pass; `cargo test --test task_sync` fully green

---

## Implementation Order

| Order | Task | Type | Size | Depends On | Track |
|-------|------|------|------|------------|-------|
| 1 | T-008-01 | Prerequisite | S | none | Lead |
| 2 | T-008-02 | Implementation | M | T-008-01 | Lead |
| 3a | T-008-03 | Implementation | L | T-008-02 | CRDT |
| 3b | T-008-07 | Refactor | M | T-008-02 | UI |
| 3c | T-008-13 | Implementation | M | T-008-02 | UI |
| 3d | T-008-16 | Test | S | T-008-01 | Reviewer |
| 4a | T-008-04 | Implementation | L | T-008-03 | CRDT |
| 4b | T-008-08 | Implementation | M | T-008-07 | UI |
| 4c | T-008-10 | Implementation | S | T-008-07 | UI |
| 4d | T-008-11 | Implementation | M | T-008-07 | UI |
| 4e | T-008-14 | Implementation | M | T-008-13, T-008-04 | UI (cross-track) |
| 5a | T-008-05 | Implementation | S | T-008-04 | CRDT |
| 5b | T-008-06 | Implementation | M | T-008-04 | CRDT |
| 5c | T-008-09 | Implementation | S | T-008-08 | UI |
| 5d | T-008-12 | Implementation | S | T-008-11 | UI |
| 6 | T-008-15 | Prerequisite | S | all impl | Lead |
| 7a | T-008-17 | Test | L | T-008-15, T-008-16 | Reviewer |
| 8 | T-008-18 | Test | L | T-008-17 | Reviewer |

## Notes for Agent Team

### Module Ownership (prevent merge conflicts)
- **Lead**: All `Cargo.toml` files, `*/lib.rs` module declarations, `termchat-proto/src/task.rs`, `termchat-proto/src/message.rs` (Envelope variant), `CLAUDE.md`
- **Builder-CRDT**: T-008-03, T-008-04, T-008-05, T-008-06 — `termchat/src/tasks/merge.rs`, `termchat/src/tasks/manager.rs`, `termchat/src/tasks/mod.rs`
- **Builder-UI**: T-008-07, T-008-08, T-008-09, T-008-10, T-008-11, T-008-12, T-008-13, T-008-14 — `termchat/src/app.rs`, `termchat/src/ui/task_panel.rs`, `termchat/src/ui/mod.rs`, `termchat/src/ui/status_bar.rs`, `termchat/src/agent/protocol.rs`, `termchat/src/agent/participant.rs`
- **Reviewer**: T-008-16, T-008-17, T-008-18 — `tests/integration/task_sync.rs`

### Coordination Points
1. **T-008-14 is the cross-track sync point** — Builder-UI needs TaskManager (from Builder-CRDT's T-008-04) to wire agent task operations. T-008-04 must complete before T-008-14.
2. **Builder-UI has the heavier load** (8 tasks vs 4 for Builder-CRDT) but several are small (S). Builder-CRDT has the riskier work (CRDT correctness).
3. **Builders must run `cargo fmt` and `cargo clippy -p <crate> -- -D warnings` before marking each task complete** (Sprint 6 retro action item).
4. **Include explicit "claim task #N immediately" in builder spawn prompts** (Sprint 5 retro action item).
5. **T-008-03 (merge functions) is the highest-risk task** — must be mathematically correct. Commutativity, associativity, and idempotency must be proven by tests.
6. **Task panel rewrite (T-008-11) changes function signature** — `render(frame, area)` becomes `render(frame, area, app)`. Lead must update `ui/mod.rs` call site (T-008-12) or ensure Builder-UI handles both files.
