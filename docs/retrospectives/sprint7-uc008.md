# Retrospective: Sprint 7 — UC-008 Share Task List

Date: 2026-02-07
Scope: Sprint 7 (UC-008 Share Task List), Phase 7: Task Coordination
Commit: `bd7ab88`

## Summary

Implemented the shared task list subsystem with LWW CRDT-based conflict resolution, room-scoped task management, interactive TUI task panel, slash commands, and agent bridge integration for task CRUD. This was the first sprint to introduce CRDT concepts (Last-Write-Wins registers with per-field merge). The proven Forge workflow executed cleanly for the 4th consecutive sprint. 18 tasks across 3 parallel tracks (CRDT, UI, Agent), 146 new tests (475 → 621), 21 files changed (4,205 lines added). Zero agent kills, zero merge conflicts. Three cross-track compilation issues (borrow checker, duplicate constant race, clippy) were caught and fixed by Lead at the integration gate.

## Metrics

| Metric | Value |
|--------|-------|
| Use cases completed | 1 (UC-008) |
| Tasks decomposed | 18 |
| Tasks completed | 18/18 |
| Tests added | 146 (475 → 621 total) |
| New files | 7 (tasks/{mod,merge,manager}.rs, proto/task.rs, task_sync integration test, UC doc, task doc, team plan) |
| Modified files | 14 |
| Lines added | ~4,205 |
| Lines deleted | ~40 |
| Agent team size | 4 (Lead + Builder-CRDT + Builder-UI + Reviewer) |
| Agent kills | 0 |
| Quality gate failures | 1 (3 issues at Phase 2C — borrow checker, duplicate constant, clippy) |
| Merge conflicts | 0 |
| Manual interventions | 3 (borrow checker fix, duplicate constant removal, clippy fixes) |
| Builder-CRDT tasks | 4 (T-008-03 through T-008-06), ~60 unit tests |
| Builder-UI tasks | 8 (T-008-07 through T-008-14), ~70 unit tests |
| Reviewer tasks | 3 (T-008-16 through T-008-18), 25 integration tests + property tests |

## What Worked

1. **Forge workflow: 4th consecutive clean execution** — `/uc-create` → `/uc-review` → fix → `/task-decompose` → `/agent-team-plan` → execute → verify → commit → `/retrospective`. The use case document (293 lines, 42 MSS steps, 16 extensions, 4 variations) was comprehensive enough that builders needed zero clarification. Task decomposition into 18 tasks across 3 tracks was accurate — no tasks needed splitting or merging.

2. **Proto-types-first pattern: 5th sprint validation** — Lead completed `TaskId`, `LwwRegister<T>`, `Task`, `TaskStatus`, `TaskFieldUpdate`, `TaskSyncMessage`, and `Envelope::TaskSync` in Phase 1 before spawning builders. Both builders coded against the shared contract. Builder-CRDT implemented merge logic against `LwwRegister<T>`, Builder-UI mapped proto types to `DisplayTask` and `BridgeTaskInfo`. No contract mismatches.

3. **CRDT correctness validated by tests** — The LWW merge logic was the highest-risk component (marked Medium risk in task decomposition). Builder-CRDT wrote ~20 merge tests covering commutativity, associativity, idempotency, equal-timestamp tie-breaking, and independent field merge. The reviewer then wrote 25 integration tests that verified convergence across two independent TaskManagers. Zero CRDT bugs found — the implementation was correct on first pass.

4. **3-track parallelism worked** — Track A (CRDT: merge.rs, manager.rs) and Track B (UI: app.rs, task_panel.rs, protocol.rs, participant.rs) ran fully in parallel. Track C (Agent Bridge) had a cross-track dependency on Track A (T-008-14 needed TaskManager from T-008-04) but Builder-UI had 6 independent tasks to work on while waiting. No idle time, no blocking.

5. **Builder-UI handled 8 tasks cleanly** — The UI track was the heaviest load (8 tasks vs 4 for CRDT), but task sizes were well-calibrated (4S + 3M + 1M-cross-track). Builder-UI completed all 8 tasks including the cross-track T-008-14 (agent capability checking), agent protocol extensions, keyboard handling, and the full task_panel.rs rewrite.

6. **Sprint 6 action items applied** — "Builders must run `cargo clippy -p <crate> -- -D warnings` before marking each task complete" was in CLAUDE.md and the team plan. While builders still didn't catch all cross-track clippy issues (see "What Didn't Work"), per-crate clippy reduced the Phase 2C gate catch from 7 issues (Sprint 6) to 3 issues.

7. **Task panel UX followed established patterns** — The task_panel.rs rewrite closely followed the chat_panel.rs rendering pattern (block with border, list items, focus-based highlighting). Focus cycling extended cleanly from 3 panels (Input/Sidebar/Chat) to 4 (Input/Sidebar/Chat/Tasks). Status bar help text for Tasks focus was trivial to add.

8. **Reviewer wrote 25 blind integration tests + property tests** — Tests covered: LWW convergence (2 managers, conflicting updates), add-wins semantics, concurrent field edits, stale update rejection, delete propagation, FullState reconciliation, empty room handling, task ordering, complete lifecycle, two-peer sync, concurrent edit resolution, and delete+concurrent edit. All passed on first run against the builders' code.

## What Didn't Work

1. **Borrow checker issue in manager.rs required Lead intervention** — Builder-CRDT's `update_status` and `update_assignee` methods had a classic Rust borrow issue: `self.get_task_mut()` borrows `self` mutably, then `self.local_peer_id.clone()` tries to borrow `self` immutably while the `&mut Task` is still alive. This compiled with some Rust editions/versions but failed at workspace level. Lead fixed by cloning `local_peer_id` before the mutable borrow: `let peer_id = self.local_peer_id.clone(); let task = self.get_task_mut(...)?;`. This is a known Rust pattern that should be documented.

2. **Duplicate constant race condition** — Both Lead and Builder-UI edited `participant.rs`. Lead added stub match arms for new `AgentMessage` variants, which included a `CAPABILITY_TASK_MANAGEMENT` constant. Builder-UI independently added the same constant with full capability-checking logic. The result was a duplicate constant compilation error. Fixed with `sed` because the Edit tool couldn't handle the concurrent modification. This was a file ownership violation — Lead should not have edited Builder-UI's file.

3. **Clippy cross-track issues still accumulated to Phase 2C** — Despite the Sprint 6 action item for per-task clippy, 3 clippy issues were caught at the integration gate: `if_same_then_else` in merge.rs (CRDT track), `implicit_hasher` in merge.rs (CRDT track), and `cast_possible_truncation` in manager.rs (CRDT track). All three were from Builder-CRDT's files. Builder-CRDT may have run `cargo clippy -p termchat` but these were workspace-level pedantic lints that might not trigger in isolation.

4. **TaskManager doesn't wire into ChatManager's encrypt→transport pipeline** — Similar to UC-007's limitation, task sync messages generate `TaskSyncMessage` structs but there's no actual wiring to encrypt them through a `CryptoSession` and send via `Transport`. The `apply_remote()` is tested but there's no real network path exercised. This is architecturally expected (application layer doesn't know about transport details) but means true end-to-end task sync across real peers isn't tested yet.

5. **`TaskEvent` enum defined but not wired** — The task decomposition included `TaskEvent` variants (`TaskCreated`, `TaskUpdated`, `TaskDeleted`, `FullSyncCompleted`) but these were never implemented. The `TaskManager` doesn't have an `event_tx: mpsc::Sender<TaskEvent>` channel as originally planned. The agent participant uses direct method calls instead. This is a gap for real-time UI updates from network-received task changes.

## Patterns Observed

1. **Fewer tasks, same output quality** — 18 tasks (vs 20 for Sprint 6) produced comparable output: 4,205 lines (vs 5,581), 146 tests (vs 135). Task granularity was slightly better — no filler tasks. The 2 "prerequisite" tasks (T-008-01, T-008-15) were efficient scaffolding/gate tasks.

2. **CRDT complexity is manageable with good types** — `LwwRegister<T>` as a generic struct with `(value, timestamp, author)` made the merge logic straightforward. Per-field independent merge meant concurrent edits to different fields always both survive. The feared "CRDT is hard" complexity didn't materialize because the LWW approach is the simplest possible CRDT.

3. **Cross-track sync points decreased** — Sprint 6 had one explicit cross-track dependency (T-007-07↔T-007-09). Sprint 7 had one (T-008-14 needing T-008-04). But Sprint 7's sync was naturally resolved by task ordering — Builder-CRDT finished Track A before Builder-UI reached T-008-14. No coordination needed.

4. **Lead intervention shifted from clippy to Rust semantics** — Sprint 5: Lead nudged builder (process issue). Sprint 6: Lead fixed 7 clippy warnings (lint issue). Sprint 7: Lead fixed borrow checker + duplicate constant (language semantics + coordination issue). The interventions are getting more technically nuanced as the codebase grows.

5. **Builder agent specialization pays off** — Builder-CRDT (4 tasks, pure logic, no UI) and Builder-UI (8 tasks, UI + protocol, no CRDT) had completely disjoint concerns. Neither needed to understand the other's domain. This confirms the two-builder pattern works best when tracks are conceptually independent, not just file-independent.

## Comparison with Previous Sprints

| Dimension | Sprint 4 (UC-004) | Sprint 5 (UC-006) | Sprint 6 (UC-007) | Sprint 7 (UC-008) | Trend |
|-----------|-------------------|-------------------|-------------------|-------------------|-------|
| Process steps followed | 6/6 | 6/6 | 6/6 | 6/6 | Stable (4 sprints) |
| Tasks decomposed | 16 | 18 | 20 | 18 | Stable (~18 avg) |
| Tests added | 57 | 71 | 135 | 146 | +8% (test density up) |
| Agent kills | 0 | 0 | 0 | 0 | Stable (4 sprints) |
| Merge conflicts | 0 | 0 | 0 | 0 | Stable (4 sprints) |
| Quality gate failures | 0 | 1 (fmt) | 1 (clippy) | 1 (borrow+clippy) | Stable (minor, caught) |
| Manual interventions | 0 | 1 (nudge) | 2 (sync+clippy) | 3 (borrow+dup+clippy) | +1 (complexity cost) |
| Lines added | ~4,022 | ~2,651 | ~5,581 | ~4,205 | Variable |
| Phase 2C gate issues | N/A | 0 | 7 | 3 | Improving (-57%) |
| UC complexity | Medium | Medium | High | Medium-High | Stable |
| Sprint N-1 retro actions applied | N/A | N/A | 3/3 | 1/3 partially | Regression |

## Action Items

### Immediate (apply now)

| # | Action | Target | Status |
|---|--------|--------|--------|
| 1 | Add "clone `self` fields before `get_*_mut()` calls to avoid borrow checker issues" to CLAUDE.md process learnings | `CLAUDE.md` | Pending |
| 2 | Add "Lead must NOT edit builder-owned files — use SendMessage to request changes" to CLAUDE.md file ownership rules | `CLAUDE.md` | Pending |
| 3 | Add "when builders work on the same crate, run `cargo clippy` at workspace level, not per-crate" to CLAUDE.md | `CLAUDE.md` | Pending |

### Next Sprint (apply before starting)

| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | Wire TaskManager to ChatManager for real network task sync (encrypt→transport pipeline) | UC-008 enhancement or Sprint 8 | Lead |
| 2 | Implement TaskEvent channel for real-time UI updates from remote task changes | Task subsystem enhancement | Lead |
| 3 | Add centralized config system for runtime settings (heartbeat, task limits) — carried from Sprint 6 | Sprint 8 | Lead |

### Backlog

| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | Task persistence (SQLite) for restart resilience | Future UC | Lead |
| 2 | Task subtasks/dependencies | Future UC | Lead |
| 3 | Task comments/discussion | Future UC | Lead |
| 4 | Task due dates and priorities | Future UC | Lead |
| 5 | Fix JoinApproved/JoinDenied relay routing — carried from Sprint 5 | Backlog | Lead |
| 6 | Exercise full crypto/transport path for agent message fan-out — carried from Sprint 6 | Backlog | Lead |

## Key Learnings

1. **Borrow checker patterns should be documented as team knowledge** — The `clone-before-get_mut` pattern is a classic Rust footgun. Adding it to CLAUDE.md process learnings will prevent future builders from hitting the same issue. The pattern: always clone `self` fields you'll need before taking a `&mut self` borrow via a method call.

2. **File ownership violations cause race conditions** — Lead's edit of `participant.rs` (Builder-UI's file) created a duplicate constant. The Edit tool couldn't resolve it because the file was being concurrently modified. Strict file ownership is a hard rule, not a guideline. When Lead needs changes in a builder's file, use `SendMessage` to request the builder make the change.

3. **LWW CRDTs are accessible for first-time implementation** — Despite being Sprint 7's marquee feature ("CRDT basics" in the blueprint's learning path), the LWW approach required no external crate and was implemented correctly on first pass. The key insight: per-field independent registers with `(value, timestamp, author)` tuples keep merge logic trivially simple.

4. **Per-crate clippy doesn't catch all workspace-level pedantic lints** — Builder-CRDT's clippy runs on `termchat` didn't surface `implicit_hasher` (a pedantic lint) because it may require workspace-wide analysis. The fix: run `cargo clippy -- -D warnings` at workspace root, not `cargo clippy -p <crate>`.

5. **Manual intervention count is a leading indicator of codebase complexity** — Sprint 4: 0, Sprint 5: 1, Sprint 6: 2, Sprint 7: 3. Each sprint adds more cross-cutting concerns. This is manageable but suggests that Sprint 8 (polish/ship) should prioritize reducing tech debt before adding more features.

## Process Rating

| Dimension | Rating | Notes |
|-----------|--------|-------|
| Use Case Quality | 5/5 | 293 lines, 42 MSS steps, 16 extensions, comprehensive CRDT sync specification |
| Task Decomposition | 5/5 | 18 tasks, accurate sizes, 3 parallel tracks, correct dependency graph |
| Agent Coordination | 4/5 | Zero kills, zero conflicts, but 1 file ownership violation (Lead edited builder's file) |
| Quality Gates | 4/5 | Phase 2C caught 3 issues (down from 7), but per-task clippy only partially applied |
| Documentation | 5/5 | CLAUDE.md updated, team plan detailed, task docs comprehensive, UC doc published |
| **Overall** | **4.6/5** | **Solid sprint — CRDT implementation clean, but interventions trending up. Must enforce file ownership strictly.** |
