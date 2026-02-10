# Retrospective: Sprint 11 — UC-017 Connect TUI to Live Backend State

Date: 2026-02-09
Scope: UC-017 (connect TUI to live backend state — per-conversation isolation, dynamic status bar, room commands, presence/typing wiring, delivery status, unread counts)

## Summary

UC-017 was the largest single UC to date (XL complexity, 12 tasks, 4-agent team). It successfully wired the TUI to live backend state: per-conversation message isolation, dynamic connection status, presence/typing indicators from network events, room commands (/create-room, /list-rooms, /join-room, /approve, /deny), message delivery tracking, and unread counts. The implementation scored B (83%) at grading. The primary process failure was a **single 1729-line commit** — the team plan explicitly called for commits after each phase gate, but the Lead waited until Gate 4. A secondary gap was the room protocol being stubbed at the transport level (`send_room_message()` is a no-op), meaning room message delivery doesn't actually work end-to-end yet.

## Metrics

| Metric | Value |
|--------|-------|
| Use cases completed | 1 (UC-017) |
| Tasks decomposed | 12 (in `docs/tasks/uc-017-tasks.md`) |
| Forge steps completed | 6/6 (uc-create, uc-review, task-decompose, team-plan, execute, verify) |
| Tests at sprint start | 699 |
| Tests at sprint end | 715 (+16 from tui_live_backend.rs) |
| Lines added/changed | +1725 / -274 (net: +1451 lines) |
| Files modified | 13 (production: 9, tests: 4) |
| New files created | 1 (tests/integration/tui_live_backend.rs) |
| Agent team size | 1 lead + 2 builders + 1 reviewer |
| Context kills | 0 |
| Quality gate failures | 0 (fmt, clippy, test, deny all clean) |
| Commits | 1 implementation + 1 doc merge = **2 total** (should have been 4-5) |
| Grade | B (83%) |
| Code quality grade | B (82%) |
| Complexity hotspots | App MI=-25.3, command_handler cognitive=60, drain_net_events cyclomatic=26 |

## Sprint 10 Action Items Follow-Through

| # | Sprint 10 Action Item | Status | Notes |
|---|---------------------|--------|-------|
| 1 | Use worktrees for all feature branches | **Applied** | `/home/ubuntu/rust-term-chat-uc-017` worktree created |
| 2 | Always run /uc-review before /task-decompose | **Applied** | UC-017 reviewed before task decomposition |
| 3 | Commit after every phase gate | **VIOLATED** | Single 1729-line commit. Same failure as warned against |
| 4 | Use background subagents for read-only analysis | **Applied** | Review and decompose ran as background agents |
| 5 | Update docs as part of sprint commit | **Applied** | UC registry, sprint doc, backlog updated in merge commit |

## What Worked

### 1. Forge Workflow (Full Pipeline)
The complete pipeline was followed: `/uc-create` with comprehensive Cockburn template (9 postconditions, 17 extensions, 4 variations) → `/uc-review` catching issues early → `/task-decompose` producing 12 well-scoped tasks → `/agent-team-plan` with 4-agent team configuration → worktree-based development → execution → grading. The UC document quality was the highest yet — 23 MSS steps, extensive extension coverage, clear out-of-scope boundaries.

### 2. File Ownership = Zero Merge Conflicts
The strict file ownership model (Builder-TUI: app.rs + ui/*, Builder-Infra: net.rs, Lead: main.rs, Reviewer: tests/) again produced zero merge conflicts. The "Lead owns main.rs" pattern where Builder-Infra delivers NetEvent/NetCommand contracts and Lead integrates them into `drain_net_events()` is now a proven pattern across 4 sprints.

### 3. Return-Value Pattern for Commands
`handle_command()` returning `Option<NetCommand>` instead of storing a channel reference in App was a clean design decision. It keeps App testable (no channel dependency), keeps command dispatch centralized in main.rs, and prevents tight coupling between App and the networking layer.

### 4. Per-Conversation Isolation Architecture
`HashMap<String, Vec<DisplayMessage>>` with `push_message(conversation, msg)` and `current_messages()` is a clean, testable design. Auto-creation of conversation entries for unknown peers (extension 10a) was well-implemented.

### 5. Test Coverage (15 focused tests)
The 15 integration tests in `tui_live_backend.rs` cover: empty app (no demo data), per-conversation isolation, connection status wiring, presence wiring, typing wiring, unread counts, auto-conversation creation, message previews, deduplication, and conversation name tracking. Each test targets a specific postcondition.

## What Didn't Work

### 1. Single Monolithic Commit (1729 lines)
**This is the third time this has been flagged.** The team plan explicitly stated "Commit strategy: One commit after Gate 4 passes" — but this contradicts the CLAUDE.md rule of "commit after every phase gate." The team plan was wrong, and the Lead followed the plan instead of the project standard. Result: a 1729-line commit that is not bisectable and hard to review. Should have been: commit after Phase 1 gate (~200 lines), commit after Phase 2C (~400 lines), commit after Phase 3C (~300 lines), final commit after Gate 4 (~800 lines).

**Root cause**: The team plan document said "One commit after Gate 4" — this overrode the CLAUDE.md standard. The team plan template needs to enforce the commit-per-gate rule.

### 2. Room Protocol Stub (send_room_message is no-op)
`send_room_message()` in net.rs logs a warning and returns Ok — room message delivery doesn't actually work. The UC doc specifies "Both users can now exchange messages in the room conversation" (MSS step 23) but this isn't implemented. The commands for creating/joining rooms work, but sending a message in a room conversation silently drops it. This gap should have been caught by the grading rubric (it was — scored C on room-related criteria).

### 3. StatusChanged Uses Sender Search Instead of Message ID
T-017-07 specified "Update StatusChanged handler to find by ID instead of 'You' sender search" but the implementation still searches for the last message with sender "You" to update delivery status. This is fragile — if you send two messages quickly, only the most recent gets its status updated.

### 4. Complexity Accumulation in App Module
App.rs is now 976 lines with MI (Maintainability Index) of -25.3 (critical). `handle_command()` has cognitive complexity of 60. `drain_net_events()` in main.rs has cyclomatic complexity of 26 (13 match arms). These weren't introduced by UC-017 alone, but UC-017 added significant new responsibilities to an already-overloaded module without decomposition.

### 5. 4 Orphaned Worktrees
Worktrees for UC-014, UC-015, UC-016, and UC-017 still exist despite all being merged:
- `/home/ubuntu/rust-term-chat-uc-017` (feature/uc-017-connect-tui-live-backend)
- `/home/ubuntu/rust-term-chat-uc014` (feature/uc-014-chatmanager-refactor)
- `/home/ubuntu/rust-term-chat-uc015` (feature/uc-015-agent-crypto-fanout)
- `/home/ubuntu/rust-term-chat-uc016` (feature/uc-016-join-relay-routing)

These waste disk space and create confusion about which directory to work in.

## Patterns Observed

### 1. "Commit at Gate 4" Keeps Recurring
Despite being explicitly added to CLAUDE.md after Sprint 10, the commit-per-gate rule was violated again. The team plan template is the culprit — it specifies commit strategy and sometimes contradicts CLAUDE.md. **The template must enforce the standard, not override it.**

### 2. Complexity Accumulates in App Without Decomposition
App.rs started at ~400 lines (Sprint 1) and is now 976 lines. Each UC adds new fields, methods, and command handlers. Without a deliberate decomposition UC (like UC-014 did for ChatManager), App will become unmaintainable. A similar pattern is forming in `main.rs::drain_net_events()`.

### 3. Stubs Propagate Across UC Boundaries
`send_room_message()` was stubbed in UC-017 because the relay-side room message routing wasn't in scope. But the UC postconditions implied it should work. When stubs cross UC boundaries, they should be explicitly tracked as tech debt with a follow-up UC.

### 4. Team Plans That Override Project Standards
The team plan for UC-017 said "One commit after Gate 4" while CLAUDE.md says "commit after every phase gate." When there's a conflict, CLAUDE.md should always win. Team plans should reference the standard, not redefine it.

## Action Items

### Immediate (apply now)

| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | Clean up 4 orphaned worktrees (uc014, uc015, uc016, uc017) | git worktree | Lead |
| 2 | Add to CLAUDE.md: "Team plan commit strategy MUST match CLAUDE.md — never override with 'one commit at Gate 4'" | CLAUDE.md | Lead |
| 3 | Add to CLAUDE.md: "Clean up worktrees after merging feature branches — `git worktree remove`" | CLAUDE.md | Lead |

### Next Sprint (apply before starting)

| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | Create UC for App decomposition (extract command_handler, extract drain_net_events, extract conversation management) | docs/use-cases/ | Lead |
| 2 | Fix room message delivery stub (`send_room_message()` no-op) | net.rs | Builder-Infra |
| 3 | Fix StatusChanged handler to use message_id instead of sender search | app.rs | Builder-TUI |
| 4 | Update `/agent-team-plan` template to include "Commit strategy: Per CLAUDE.md — commit after each phase gate" with no override option | .claude/commands/agent-team-plan.md | Lead |
| 5 | Add `pub(crate)` audit — 97 `pub` items, 0 `pub(crate)` | workspace | Builder |

### Backlog (nice to have)

| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | Add `just grade` recipe that runs /grade-work equivalent checks automatically | justfile | Lead |
| 2 | Add worktree cleanup to post-merge hook or `/retrospective` checklist | hooks or commands | Lead |
| 3 | Track stubs/no-ops in a dedicated `docs/tech-debt.md` file | docs/ | Lead |
| 4 | Consider extracting App state into sub-structs (ConnectionState, ConversationState, UIState) | app.rs | Builder-TUI |

## Key Learnings

1. **Team plan commit strategies must defer to CLAUDE.md, never override.** The team plan said "one commit at Gate 4" — this produced a 1729-line commit that violates the project standard. The fix is systemic: update the team plan template to reference the standard.

2. **Return-value pattern (`handle_command() -> Option<NetCommand>`) is the right approach for TUI→network dispatch.** It keeps App free of channel dependencies, enables unit testing without async runtime, and centralizes dispatch in main.rs.

3. **File ownership with Lead-owned integration points (main.rs) produces zero merge conflicts.** This has now been proven across 4 sprints (UC-006 through UC-017). The pattern: Builder-A owns module A, Builder-B owns module B, Lead owns the glue code that connects them.

4. **Stubs at transport boundaries should be tracked explicitly.** `send_room_message()` being a no-op means room messaging doesn't work despite the UC implying it should. Each stub should generate a tracked follow-up task.

5. **App.rs decomposition is overdue at 976 lines.** Following the ChatManager pattern (UC-014 split it into mod.rs + history.rs + room.rs), App needs a similar treatment: command handling, conversation management, and UI state should be separate submodules.

## Process Rating

| Category | Rating | Notes |
|----------|--------|-------|
| Use Case Quality | 4/5 | Comprehensive Cockburn template, excellent extension coverage. Room protocol gap not caught. |
| Task Decomposition | 4/5 | 12 well-scoped tasks with clear dependencies, parallel tracks. Task sizes were accurate. |
| Agent Coordination | 4/5 | Zero merge conflicts, clean parallel execution. main.rs integration points worked well. |
| Quality Gates | 3/5 | All automated gates passed. But commit-per-gate was violated — the process enforcement failed. |
| Overall | 4/5 | Strong execution, good test coverage. Commit discipline and complexity accumulation are the gaps. |
