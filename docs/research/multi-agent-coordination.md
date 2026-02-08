# Multi-Agent Coordination Research

Date: 2026-02-08
Context: Sprint 8 post-mortem — process adherence scored 2.4/5, worst of any sprint.

## Root Causes Identified

1. **No session handoff mechanism** — 20-30% of context budget wasted on archaeology when continuing work across sessions
2. **No task files for progress tracking** — agents lost track of what was done vs. remaining
3. **Worked on main instead of worktree** — violated own process rules
4. **Monolithic commit bundling 3 UCs** — violated one-commit-per-UC standard

## Community Patterns Researched

### 1. Session Handoff Files (ADOPTED)

**Pattern**: Write structured state to a markdown file before ending a session. Next session reads it first.

**Why it works**: Claude Code sessions are ephemeral — context dies when the session ends. A handoff file acts as external memory that survives context boundaries.

**Implementation**: `/session-handoff` command writes `docs/handoff.md` with: accomplishments, current state (branch, dirty files, worktrees, test status), in-progress tasks, blockers, decisions made, next session instructions.

**Source**: Multiple Claude Code power users report this pattern. The key insight is that the file should be version-controlled (visible in `git status`) and co-located with other docs.

### 2. Pre-Implementation Checklists (ADOPTED)

**Pattern**: A reusable checklist that gates implementation start. Prevents the recurring pattern of skipping worktree creation, task files, and review.

**Why it works**: Sprint 8 failures were all "skipped a step" failures. A checklist at the start of implementation catches these before they compound.

**Implementation**: `.claude/skills/pre-implementation-checklist.md` referenced by `/uc-create`, `/task-decompose`, `/parallel-sprint`, and agent configs.

### 3. Delegate Mode for Team Leads (ADOPTED)

**Pattern**: Use `Shift+Tab` to switch to delegate mode when running as team lead. Allows spawning agents without manual approval for each tool call.

**Why it works**: Team leads spend significant time approving routine operations. Delegate mode trusts the lead to manage its sub-agents.

**Implementation**: Document in CLAUDE.md coding standards.

### 4. Plan Approval Gates (ADOPTED)

**Pattern**: Set `plan_mode_required: true` for teammate agents. They must present a plan before implementing, and the lead approves or rejects.

**Why it works**: Prevents teammates from going off-track on expensive implementation work. Catches misunderstandings early.

**Implementation**: Document in CLAUDE.md coding standards and agent team configs.

### 5. Clash — Merge Conflict Detection (DEFERRED)

**Pattern**: Tool that detects when multiple agents are editing overlapping files and alerts before conflicts occur.

**Why it matters**: Merge conflicts between parallel agents are expensive to resolve.

**Why deferred**: File ownership rules already prevent conflicts in our workflow. Only relevant if we move away from strict ownership.

**Status**: Monitor. Revisit if merge conflicts become a recurring issue despite ownership rules.

### 6. TaskCompleted Hooks (DEFERRED)

**Pattern**: Automated hook that fires when an agent marks a task complete. Runs verification commands automatically.

**Why it matters**: Would automate Gate 3 (UC Verification) of the quality pipeline.

**Why deferred**: Requires Agent Teams infrastructure (event-based hooks). Manual `/verify-uc` is adequate for current project size.

**Status**: Implement when Agent Teams event hooks are available.

### 7. Continuous Claude Ledger (DEFERRED)

**Pattern**: A running log file that captures all significant decisions, state changes, and artifacts across sessions. More detailed than handoff, less structured than task files.

**Why it matters**: Full audit trail for complex multi-sprint features.

**Why deferred**: Heavy infrastructure — requires hooks on every significant action. Overkill for current project size. The combination of handoff files + task files + retrospectives provides sufficient coverage.

**Status**: Revisit for Phase 5+ when features span multiple sprints.

### 8. GitButler Session Isolation (DEFERRED)

**Pattern**: Use GitButler's virtual branches instead of git worktrees for parallel agent isolation.

**Why it matters**: GitButler provides a richer model for managing parallel work than raw worktrees.

**Why deferred**: Worktrees are well-understood, work with standard git, and are already documented in our process. GitButler adds a dependency and learning curve.

**Status**: Monitor GitButler development. Consider if worktree management becomes painful.

## Patterns We Already Do Well

- **File ownership matrix** — prevents merge conflicts without tooling
- **One commit per UC** — clean git history (when we follow it)
- **Quality gates** — fmt + clippy + test catches most issues
- **Task decomposition** — `/task-decompose` produces well-scoped tasks
- **Cockburn extensions** — catch ~40% of implementation work

## Key Insight

The Sprint 8 failures were not tooling gaps — they were **process discipline** failures. The tools exist (worktrees, task files, one-commit-per-UC) but weren't used. The highest-ROI fix is making the tools more visible and harder to skip:

1. `/session-handoff` makes cross-session state explicit (can't forget what you don't track)
2. Pre-implementation checklist gates implementation start (can't skip what's required)
3. Updated commands reference the checklist (can't miss what's in your face)

## Action Items

| Priority | Item | Status |
|----------|------|--------|
| P0 | Create `/session-handoff` command | Implementing |
| P0 | Create pre-implementation checklist skill | Implementing |
| P1 | Update CLAUDE.md with delegate mode + plan gates | Implementing |
| P1 | Update existing commands to reference checklist | Implementing |
| P1 | Add `cargo deny check` to commit hook | Implementing |
| P2 | Update agent team configs | Implementing |
| P3 | Monitor Clash for merge conflict detection | Deferred |
| P3 | TaskCompleted hook automation | Deferred |
| P3 | Continuous Claude ledger | Deferred |
| P3 | GitButler session isolation | Deferred |
