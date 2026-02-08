# Retrospective: Sprint 8 — UC-010 Live Relay Messaging, UC-011 Config System, UC-012 Polish

Date: 2026-02-08
Scope: Sprint 8 (Polish & Ship): UC-010 (Connect to Relay and Exchange Live Messages), UC-011 (Configure Client and Relay via Config File and CLI), UC-012 (Polish: theme, CI, logging, CLI)
Commits: `1d8567f`, `60f0fca`, `f98b027`

## Summary

Sprint 8 was the largest single-commit sprint in the project: 2,453 lines added across 25 files in one monolithic commit (`1d8567f`), followed by a config-wiring polish pass (`60f0fca`, 107 lines across 6 files). Three use cases were implemented — UC-010 (relay messaging), UC-011 (config system), UC-012 (polish) — without formal task decomposition, without a worktree, and with code partially pre-built from a prior session. The result: all 675 tests pass, quality gates green, but the process was the messiest of any sprint. Multiple Claude sessions touched the same files on main, creating confusion about what was implemented vs. what still needed wiring. A half-started reconnect feature (UC-011-auto-reconnect) was left orphaned in a worktree. The Forge workflow was fully bypassed.

## Metrics

| Metric | Value |
|--------|-------|
| Use cases completed | 3 (UC-010, UC-011, UC-012) |
| Tasks formally decomposed | 0 (plan existed but no `/task-decompose` file) |
| Tests at sprint end | 675 |
| New integration test files | 1 (`tui_net_wiring.rs`, 314 lines) |
| New production files | 4 (`config/mod.rs`, `config.rs`, `net.rs`, `.github/workflows/ci.yml`) |
| Modified production files | 14 |
| Lines added (commit 1) | 2,453 |
| Lines added (commit 2) | 107 |
| Agent team size | 1 (single agent, no team) |
| Agent context kills/restarts | 1 (session continued from prior context that hit limit) |
| Quality gate failures | 1 (linter reverted `chat/mod.rs` edit due to partial application) |
| Merge conflicts | 0 (but only because all work was on main — no branches to merge) |
| Forge steps completed | 1/6 (UC doc written, but post-implementation; no task-decompose, no team-plan, no verify-uc, no grade-work, no reviewer) |
| Worktree used | No (all work directly on main) |
| Orphaned work items | 2 (reconnect worktree with WIP, stale stash from UC-002 era) |

## What Worked

1. **Config system design was solid** — Layered resolution (CLI > env > config file > defaults) via clap `env` attribute + `#[serde(default)]` TOML structs is elegant and zero-boilerplate. `ClientConfig::default()` preserves exact prior hardcoded values, so all 675 existing tests pass without modification. The pattern is reusable for any future config needs.

2. **Builder pattern for `App` configuration** — `App::new().with_typing_timeout(secs).with_max_task_title_len(len)` keeps the constructor clean while allowing config injection. This replaced hardcoded constants without breaking any call sites.

3. **Parameterized constructors preserved backward compatibility** — `RelayTransport::connect()` (convenience) delegates to `connect_with_timeouts()` (full). `ChatManager::new()` delegates to `new_with_config()`. `MessageStore::new()` delegates to `with_max_queue_size()`. Every existing call site continues to work unchanged.

4. **Quality gate caught a real issue** — The linter correctly reverted a partial edit to `chat/mod.rs` where a `use` import was added before the struct field it referenced. This forced atomic application of all related changes. Without the linter, the code would have been in a broken intermediate state.

5. **Integration test for UC-010 is comprehensive** — `tui_net_wiring.rs` (314 lines) tests: connection, bidirectional messaging, delivery acks, unreachable relay fallback, and clean shutdown. All tests use an in-process relay server for reliability.

## What Didn't Work

1. **Working directly on main caused confusion across sessions** — A prior Claude session had already created `config/mod.rs` and `config.rs` with full implementations, added dependencies, and modified `lib.rs` — all on main. The current session's plan called for "reset main, create worktree, implement from scratch" but instead discovered the half-built state and decided to continue on main. This meant the agent had to reverse-engineer what was already done vs. what still needed wiring, wasting significant context budget on detective work instead of implementation.

2. **No task decomposition file** — A plan existed (in `.claude/plans/`) but no formal `docs/tasks/uc-011-tasks.md` was created. The plan listed 12 tasks but they were never tracked. When the session hit context limits and was continued, there was no external record of progress. The agent had to re-discover the state from git diff output.

3. **Monolithic commit hides structure** — `1d8567f` contains 2,453 lines across 25 files spanning three separate use cases (UC-010, UC-011, UC-012). This makes it impossible to bisect, revert a single UC, or understand the commit history. Previous sprints committed per-UC.

4. **Orphaned reconnect work** — A worktree at `/home/ubuntu/rust-term-chat-reconnect` was created for UC-011-auto-reconnect, with a complete Cockburn UC doc and a `ReconnectConfig` struct, but was abandoned when the session pivoted to the config system. This work was nearly lost — it wasn't committed until the cleanup phase.

5. **UC docs written post-implementation or mid-implementation** — UC-011's doc was written as part of the plan but never validated against the actual implementation. UC-010's doc was bundled into the monolithic commit without evidence it drove the implementation. The Cockburn-first philosophy was violated.

6. **No reviewer agent** — Every previous multi-UC sprint had a reviewer writing blind tests against postconditions. Sprint 8 had no independent verification. The integration tests were author-written and test what was built, not what should have been built.

7. **Linter as gatekeeper caused thrashing** — When editing `chat/mod.rs`, the linter reverted the first edit because the code was in a non-compiling intermediate state (import added but struct field not yet added). The agent had to re-read the file and apply all changes atomically. This happened again with `main.rs`. A plan-then-apply-atomically approach would have avoided the thrashing.

## Patterns Observed

1. **Cross-session state is the biggest coordination failure mode** — When one Claude session partially implements a feature and another session picks it up, the second session spends 20-30% of its context budget just understanding what the first session did. Task files and worktrees are the solution: task files record progress externally; worktrees isolate changes to a branch that can be inspected cleanly.

2. **"Continue on main" is always a mistake for multi-session work** — The temptation to skip worktree creation when code already exists on main leads to exactly the confusion observed here. Even if it means cherry-picking or recreating work, a clean feature branch in a worktree is always safer.

3. **Monolithic commits correlate with process shortcuts** — When the Forge workflow is followed (UC doc -> tasks -> implement per task -> verify), natural commit points emerge at each task boundary. When the workflow is skipped, everything accumulates into a single "implement everything" commit.

4. **The Forge workflow's value is highest during multi-session work** — For single-session, single-UC implementations (like UC-009), skipping the Forge is a reasonable tradeoff. But the moment work spans sessions or agents, the external state (UC docs, task files, worktree branches) becomes critical for continuity.

5. **Config-as-code is a cross-cutting concern that touches everything** — UC-011 modified 14 production files across all three crates. Cross-cutting changes like this are the hardest to coordinate across agents and the most important to plan carefully.

## Comparison with Previous Sprints

| Dimension | Sprint 6 (UC-007) | Sprint 7 (UC-008+009) | Sprint 8 (UC-010+011+012) |
|-----------|-------------------|----------------------|---------------------------|
| Forge workflow | Full (6/6) | Partial (2/6 for UC-009) | Minimal (1/6) |
| Task files | Yes | Yes (UC-008), No (UC-009) | No |
| Worktree used | Yes | Yes (UC-009) | No |
| Agent team | 4 agents | 1 agent | 1 agent |
| Reviewer agent | Yes | No | No |
| Commits per UC | 1 | 1-2 | 1 monolithic (3 UCs) |
| Context kills | 0 | 0 | 1 |
| Orphaned work | 0 | 0 | 2 (worktree + stash) |
| Lines added | ~5,581 | ~2,800 | ~2,560 |
| Tests at end | 405 | 499 | 675 |
| Quality | 4.5/5 | 3.8/5 | 3.0/5 |

## Action Items

### Immediate (apply now)

| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | Add "Cross-session work MUST use task files for progress tracking" to Process Learnings | `CLAUDE.md` | Lead |
| 2 | Add "One commit per UC, never bundle multiple UCs into one commit" to Coding Standards | `CLAUDE.md` | Lead |
| 3 | Add "When continuing from a prior session, read task files FIRST before examining code" to Process Learnings | `CLAUDE.md` | Lead |
| 4 | Add "Apply all related edits to a file atomically to avoid linter revert thrashing" to Process Learnings | `CLAUDE.md` | Lead |

### Next Sprint (apply before starting)

| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | Always create `docs/tasks/uc-NNN-tasks.md` even for single-agent work | Process | Lead |
| 2 | Pick up orphaned UC-011-auto-reconnect from `feature/uc-011-reconnect` worktree | Feature work | Lead |
| 3 | Add a pre-implementation checklist to `/uc-create`: worktree created? task file created? | `.claude/commands/uc-create.md` | Lead |
| 4 | Consider adding a `/session-handoff` command that writes session state to a file for the next session to read | `.claude/commands/` | Lead |

### Backlog

| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | Split `1d8567f` retroactively into per-UC commits via interactive rebase | Git history | Lead |
| 2 | Add UC-010 and UC-011 task decomposition files retroactively for documentation | `docs/tasks/` | Lead |
| 3 | Run `/grade-work` on UC-010 and UC-011 to establish quality baselines | Quality | Lead |
| 4 | Investigate `/session-handoff` command design: what state needs persisting? | Tooling | Lead |

## Key Learnings

1. **The Forge workflow is not optional for multi-session work.** Task files (`docs/tasks/uc-NNN-tasks.md`) are the external memory that survives context kills and session boundaries. Without them, the next session wastes 20-30% of its budget on archaeology.

2. **Worktrees are not just for parallel agents — they're for sequential sessions too.** A feature branch in a worktree has a clean diff, a clear scope, and can be inspected by a fresh session without confusion. Working directly on main with uncommitted changes from a prior session is a recipe for confusion.

3. **Monolithic commits are a smell that the process was skipped.** When the Forge is followed, natural commit boundaries emerge at task completion. A single commit spanning 25 files and 3 UCs means no task boundaries were honored.

4. **Linters are a safety net, not an annoyance.** The linter correctly prevented a non-compiling intermediate state. The fix is to plan all related edits before starting, then apply them atomically — not to fight the linter.

5. **Cross-cutting infrastructure changes (config, logging, CI) are the hardest to coordinate** and benefit most from formal task decomposition. They touch every module and every crate, making them high-risk for conflicts and confusion.

## Process Rating

| Dimension | Rating | Notes |
|-----------|--------|-------|
| Use Case Quality | 2/5 | UC docs written post-implementation or bundled into monolithic commit |
| Task Decomposition | 1/5 | Plan existed but no task file was created; no tracking across sessions |
| Agent Coordination | 2/5 | Cross-session handoff failed; orphaned work; no reviewer |
| Quality Gates | 4/5 | All gates green; linter caught real issue; 675 tests pass |
| Documentation | 3/5 | UC docs exist but retrospective was not written until prompted |
| **Overall** | **2.4/5** | **Functional outcome (code works, tests pass) but worst process adherence of any sprint. The Forge was built for exactly this scenario and was not used.** |
