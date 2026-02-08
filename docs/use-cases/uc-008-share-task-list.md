# Use Case: UC-008 Share Task List

## Classification
- **Goal Level**: 🌊 User Goal
- **Scope**: System (black box)
- **Priority**: P1 High
- **Complexity**: 🔴 High

## Actors
- **Primary Actor**: Room Member (terminal user who creates, edits, or views tasks in a room)
- **Supporting Actors**: Other Room Members (peers who receive task sync updates), Agent Participant (agent connected via UC-007 bridge that can manage tasks), Transport Layer, Crypto Layer
- **Stakeholders & Interests**:
  - Room Member: wants a shared, always-up-to-date task list embedded in the chat UI for team coordination — no context-switching to external tools
  - Other Room Members: want to see task changes in real-time, trust that concurrent edits resolve deterministically
  - Agent Participant: needs programmatic task CRUD to coordinate work, report progress, and track assignments
  - System: task sync must use the same E2E encryption as chat messages — relay never sees task content

## Conditions
- **Preconditions** (must be true before starting):
  1. Room Member has a valid identity keypair (from UC-005)
  2. Room Member is a member of at least one room (from UC-006)
  3. At least one transport (P2P or relay) is available (from UC-003/UC-004)
  4. TUI is running and responsive (from Phase 1)
- **Success Postconditions** (true when done right):
  1. A task list exists for the room, visible in the Task Panel
  2. Room Member can create, edit status, assign, and delete tasks via slash commands or keyboard shortcuts
  3. Task changes are broadcast to all room members as encrypted `TaskSync` messages via the existing transport pipeline
  4. Remote room members receive task updates and their local task lists converge to the same state (LWW CRDT convergence)
  5. When a new member joins a room, they receive the full task list via `FullState` sync
  6. The Task Panel is interactive: focus cycling includes it, j/k navigates, Enter toggles status
  7. Connected agents with `TaskManagement` capability can create, update, and query tasks via the bridge protocol
  8. Task state survives concurrent edits from multiple peers — LWW (Last-Write-Wins) per field ensures deterministic resolution
- **Failure Postconditions** (true when it fails gracefully):
  1. If task sync message fails to send to a remote peer, the message is queued for retry (same as chat message offline queue)
  2. If a task command has invalid arguments, the user sees a clear error message in the chat panel
  3. If the bridge protocol receives an invalid task operation, the agent receives an error response
- **Invariants** (must remain true throughout):
  1. Task sync messages are encrypted per-peer (via Noise sessions) before transmission — relay never sees task content
  2. LWW merge is commutative, associative, and idempotent — peers converge regardless of message ordering or duplication
  3. Each task field (title, status, assignee) has an independent LWW register — concurrent edits to different fields of the same task both survive
  4. TaskId uses UUID v7 for time-ordering and global uniqueness

## Main Success Scenario

### Part A: Create a Task (Primary Actor: Room Member)

1. Room Member types `/task add Fix the login bug` in the input box and presses Enter
2. System validates the title (non-empty, within 256 character limit)
3. System creates a new `Task` with UUID v7 TaskId, the title in an LWW register (timestamp = now, author = local peer_id), status = Open, assignee = None, created_by = local peer_id
4. System adds the task to the local room task list (in `TaskManager`)
5. System builds a `TaskSyncMessage::FullState` containing just the new task (receivers merge via add-wins: unknown TaskId → add task)
6. System serializes the sync message with postcard, wraps in `Envelope::TaskSync`, encrypts per-peer, and sends to all room members via the existing transport
7. System renders the new task in the Task Panel with `[ ] Fix the login bug`
8. System shows a system message in chat: "Task created: Fix the login bug"

### Part B: Remote Peer Receives Task Update

9. Remote peer receives encrypted envelope, decrypts, decodes `Envelope::TaskSync`
10. Remote peer deserializes `TaskSyncMessage::FieldUpdate` with postcard
11. Remote peer's `TaskManager` applies the update using LWW merge logic
12. Since the task is new (TaskId not in local list), the task is added (add-wins semantics)
13. Remote peer's Task Panel re-renders showing the new task

### Part C: Update Task Status (Keyboard Interaction)

14. Room Member presses Tab to cycle focus to the Task Panel
15. Room Member uses j/k (or Up/Down) to select a task
16. Room Member presses Enter to toggle the task's status (Open → InProgress → Completed → Open)
17. System updates the local task's status LWW register (timestamp = now, author = local peer_id)
18. System broadcasts `TaskSyncMessage::FieldUpdate` with the new status to all room members
19. System re-renders the task with updated status indicator: `[ ]` → `[~]` → `[x]`

### Part C-alt: Update Task Status (Command)

Room Member may alternatively type `/task done <task-number>` to mark a task as Completed, following the same broadcast flow as steps 17-19.

### Part D: Assign a Task

20. Room Member types `/task assign <task-number> @alice` in the input box
21. System validates the task number exists and the assignee is a room member
22. System updates the local task's assignee LWW register
23. System broadcasts the field update to all room members
24. System re-renders the task showing the assignee: `[ ] Fix the login bug (@alice)`

### Part E: Delete a Task

25. Room Member types `/task delete <task-number>` in the input box
26. System removes the task from the local task list
27. System broadcasts a `TaskSyncMessage::FieldUpdate` with a deletion marker (status = Deleted)
28. Remote peers remove the task from their local lists upon receiving the update

### Part F: New Peer Joins and Receives Full State

29. A new peer joins the room (via UC-006 join flow)
30. The new peer sends `TaskSyncMessage::RequestFullState` to the room admin
31. Room admin responds with `TaskSyncMessage::FullState` containing all tasks for the room
32. New peer merges the full state into their (empty) local task list
33. Task Panel renders the complete task list

### Part G: Agent Manages Tasks via Bridge

34. Connected agent (from UC-007) sends `{"type": "create_task", "title": "Review PR #42"}` via bridge
35. System validates the agent has `TaskManagement` capability
36. System creates the task (same as MSS steps 2-8, with agent's peer_id as author)
37. System sends `{"type": "task_update", ...}` to the agent confirming the creation
38. Agent can also send `update_task_status`, `assign_task`, and `list_tasks` messages

### Part H: Concurrent Edit Resolution

39. Two room members simultaneously edit the same task's title
40. Each peer's local `TaskManager` applies their own edit immediately (optimistic)
41. When each peer receives the other's `FieldUpdate`, LWW merge resolves: the edit with the higher timestamp wins; ties broken by peer_id lexicographic comparison
42. Both peers converge to the same title value

## Extensions (What Can Go Wrong)

- **2a. Task title is empty**:
  1. System shows error: "Task title cannot be empty"
  2. Returns to step 1
- **2b. Task title exceeds 256 characters**:
  1. System shows error: "Task title too long (max 256 characters)"
  2. Returns to step 1
- **6a. Transport fails for one or more remote members**:
  1. System queues the sync message for retry (reuses existing offline queue from UC-001)
  2. Continues — local state is authoritative; remote peers catch up later
- **6b. No Noise session with a remote member**:
  1. System skips that member with a debug log
  2. Peer will catch up via FullState sync when session is established
- **10a. Incoming TaskSync bytes are malformed (postcard decode failure)**:
  1. System logs a warning: "Failed to decode TaskSync message from <peer_id>"
  2. Message is dropped, no state change
  3. Continues normal operation
- **11a. Incoming FieldUpdate has older timestamp than local version (stale update)**:
  1. `TaskManager` LWW merge rejects the stale update (local wins)
  2. No state change, no event emitted
  3. This is the normal CRDT convergence path — not an error
- **11b. Incoming FieldUpdate references unknown TaskId**:
  1. System creates the task from the update (add-wins semantics)
  2. Continues to step 13
- **16a. Task Panel is empty (no tasks in room)**:
  1. Task Panel renders: "No tasks yet. Use /task add <title> to create one."
  2. Enter key does nothing
- **20a. Task number doesn't exist**:
  1. System shows error: "Task #N not found"
  2. Returns to step 20
- **20b. Assignee is not a room member**:
  1. System shows error: "'name' is not a member of this room"
  2. Returns to step 20
- **25a. Task number doesn't exist**:
  1. System shows error: "Task #N not found"
  2. Returns to step 25
- **28a. Delete conflicts with concurrent edit**:
  1. For MVP: delete wins locally. If a remote peer edited the task before seeing the delete, their edit becomes a no-op when the delete arrives. The task may briefly reappear then disappear on the remote peer.
  2. Acceptable for MVP — documented limitation
- **30a. Room admin is offline**:
  1. New peer broadcasts `RequestFullState` to all room members
  2. Any member can respond with their local FullState
  3. LWW guarantees convergence regardless of which member responds
- **35a. Agent does not have TaskManagement capability**:
  1. System sends error to agent: `{"type": "error", "code": "capability_required", "message": "TaskManagement capability required"}`
  2. Agent can retry after re-connecting with the capability
- **39a. Three or more peers edit the same field simultaneously**:
  1. LWW resolution is the same — highest timestamp wins, peer_id breaks ties
  2. All peers converge to the same value regardless of the number of concurrent writers

## Variations
- **1a.** Room Member may create a task from the Task Panel directly (future: press `a` to add, type title, press Enter)
- **16a.** Status cycle may be customizable (future: skip InProgress, go directly Open → Completed)
- **24a.** Assignee display may use display name instead of peer_id if available
- **33a.** Full state sync may be triggered periodically (every 5 minutes) as reconciliation, not just on join

## Out of Scope

The following are NOT part of this use case and will be addressed in future work:
- **Task persistence (SQLite)**: Tasks are in-memory only for this sprint. Persistence is a Phase 8 enhancement.
- **Subtasks and dependencies**: Flat task list only. No parent-child or blocking relationships.
- **Task comments/descriptions**: Title only for MVP. Rich content is a future enhancement.
- **Due dates and priorities**: Not included. Keep the data model minimal.
- **Task filtering/search**: All tasks shown in the panel. No filtering UI yet.
- **Undo/redo**: LWW is one-way. No undo support.
- **Tombstone-based delete with CRDT**: Using soft-delete (status = Deleted) for MVP. True CRDT delete with tombstones and garbage collection is deferred.

## Agent Execution Notes
- **Verification Command**: `cargo test --test task_sync`
- **Test File**: `tests/integration/task_sync.rs`
- **Depends On**: UC-001 (Send — transport pipeline), UC-002 (Receive — receive pipeline), UC-005 (E2E Handshake — encryption), UC-006 (Rooms — room-scoped tasks), UC-007 (Agent Join — agent task management)
- **Blocks**: None (final feature use case before Polish phase)
- **Estimated Complexity**: L / ~3000 tokens per agent turn
- **Agent Assignment**: Teammate:Builder (2 builders — CRDT logic + UI/app integration)

## Implementation Notes

### LWW Register (Core CRDT Primitive)

```rust
/// A Last-Write-Wins register. The merge rule:
/// 1. Higher timestamp wins
/// 2. Equal timestamps: higher peer_id (lexicographic) wins
/// This guarantees: commutativity, associativity, idempotency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LwwRegister<T> {
    pub value: T,
    pub timestamp: u64,  // milliseconds since epoch
    pub author: String,  // peer_id of the writer
}
```

### Task Model

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub room_id: String,
    pub title: LwwRegister<String>,
    pub status: LwwRegister<TaskStatus>,
    pub assignee: LwwRegister<Option<String>>,
    pub created_at: u64,
    pub created_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus { Open, InProgress, Completed, Deleted }
```

### Sync Protocol

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskSyncMessage {
    FieldUpdate { task_id: TaskId, room_id: String, field: TaskFieldUpdate },
    FullState { room_id: String, tasks: Vec<Task> },
    RequestFullState { room_id: String },
}
```

### Wire Integration

Add `Envelope::TaskSync(Vec<u8>)` variant to `termchat-proto/src/message.rs`. The `Vec<u8>` contains postcard-encoded `TaskSyncMessage` bytes, keeping the Envelope codec decoupled from task types.

### New Module: `termchat/src/tasks/`

| File | Purpose |
|------|---------|
| `termchat/src/tasks/mod.rs` | Module root, TaskError, TaskEvent, re-exports |
| `termchat/src/tasks/merge.rs` | Pure CRDT merge functions: `merge_lww()`, `merge_task()`, `merge_task_list()`, `apply_field_update()` |
| `termchat/src/tasks/manager.rs` | TaskManager: create/update/delete/apply_remote, sync message generation, room-scoped task storage |

### Modified Files

| File | Change |
|------|--------|
| `termchat/src/lib.rs` | Add `pub mod tasks;` |
| `termchat/src/app.rs` | Add `PanelFocus::Tasks`, `DisplayTask`, task state, focus cycling, `/task` commands, keyboard handling |
| `termchat/src/ui/task_panel.rs` | Rewrite: interactive panel from `&App` state instead of hardcoded demo |
| `termchat/src/ui/mod.rs` | Pass `&app` to `task_panel::render()` |
| `termchat/src/ui/status_bar.rs` | Add Tasks focus help text |
| `termchat/src/agent/protocol.rs` | Add task message types to AgentMessage and BridgeMessage |
| `termchat/src/agent/participant.rs` | Handle task message variants in event loop |
| `termchat-proto/src/lib.rs` | Add `pub mod task;` |
| `termchat-proto/src/message.rs` | Add `Envelope::TaskSync(Vec<u8>)` |

### Agent Bridge Task Protocol

Agent → System:
```json
{"type": "create_task", "title": "Review PR #42"}
{"type": "update_task_status", "task_id": "abc-123", "status": "completed"}
{"type": "assign_task", "task_id": "abc-123", "assignee": "alice"}
{"type": "list_tasks"}
```

System → Agent:
```json
{"type": "task_list", "room_id": "uuid", "tasks": [{"task_id": "abc-123", "title": "Review PR", "status": "open", "assignee": null, "created_by": "bob"}]}
{"type": "task_update", "room_id": "uuid", "task": {"task_id": "abc-123", "title": "Review PR", "status": "completed", "assignee": "alice", "created_by": "bob"}}
{"type": "task_deleted", "room_id": "uuid", "task_id": "abc-123"}
```

## Acceptance Criteria (for grading)
- [ ] All success postconditions verified by automated test
- [ ] All extension paths have explicit handling
- [ ] No invariant violations detected
- [ ] Code passes lint + clippy (`cargo clippy -- -D warnings`)
- [ ] Reviewer agent approves
- [ ] Task CRUD works via `/task add`, `/task done`, `/task assign`, `/task delete` commands
- [ ] Task Panel is interactive with focus cycling, navigation, and status toggle
- [ ] LWW merge resolves concurrent edits deterministically (unit tests prove convergence)
- [ ] Task sync messages are encrypted and delivered to all room members
- [ ] New peer joining a room receives full task state via FullState sync
- [ ] Connected agents with TaskManagement capability can manage tasks via bridge
- [ ] Empty task panel shows helpful placeholder text
- [ ] Task status indicators render correctly: `[ ]` Open, `[~]` InProgress, `[x]` Completed
