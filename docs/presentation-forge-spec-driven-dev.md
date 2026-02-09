# The Forge: Spec-Driven Development with Multi-Agent AI Swarms

### Building a 25K-Line Rust Codebase in 3 Days with Claude Code + Cockburn Use Cases

---

## Agenda (~18 min)

1. The Problem: Why AI Agents Fail at Scale (2 min)
2. The Forge: A Cockburn-Powered Development System (3 min)
3. The Sword: What We Built (2 min)
4. Multi-Agent Parallel Swarms (4 min)
5. Metrics: Inferred from Git (4 min)
6. Live Demo (2 min)
7. Lessons Learned & Adoption Guide (1 min)

---

## 1. The Problem: Why AI Agents Fail at Scale

### The default AI-assisted workflow

```
User: "Build me a chat app with E2E encryption"
Agent: *writes 500 lines* *gets confused* *rewrites 300* *breaks something*
```

**What goes wrong:**

- **Ambiguity kills agents.** "Build a chat app" has infinite interpretations
- **No verification contract.** How does the agent know it's done?
- **Context window death.** Big tasks exceed the window; agent loses track
- **Parallel work = merge hell.** Two agents touching the same file = disaster
- **No process memory.** Lessons from Sprint 1 are forgotten by Sprint 5

### The community convergence: PRDs + TaskMaster

Most teams are settling on: write a PRD, auto-decompose into tasks, run agents.

**We can do better.** Alistair Cockburn's use case format solves the exact problems that plague AI agents:

| Cockburn Concept | AI Agent Benefit |
|-----------------|-----------------|
| **Extension conditions** | Catches the 80% of work that isn't the happy path |
| **Pre/postconditions** | Become setup assertions and verification gates |
| **Goal-level hierarchy** | Maps to lead/teammate/subagent delegation |
| **Invariants** | Continuous assertions the agent must never violate |
| **Iterative fill** | Matches AI's iterative refinement capability |

> *"The little niggling things that consume most development time are captured by systematically asking: what could go wrong at each step?"* -- Cockburn's key insight, and why extensions produce ~40% of implementation tasks.

---

## 2. The Forge: A Cockburn-Powered Development System

### What is The Forge?

A system of **11 slash commands**, **3 reusable skills**, **2 agent team configs**, and **automated quality gates** -- all built as Claude Code customizations. It enforces a spec-driven loop:

```
/uc-create  -->  /uc-review  -->  /task-decompose  -->  implement  -->  /verify-uc  -->  /grade-work  -->  /retrospective
    |               |                  |                    |               |               |                |
 Cockburn      Devil's           Task graph           Code on          Run post-       Score against      Feed back
 template      Advocate           + deps             feature           condition        acceptance         into
 wizard        review                                branch            tests            criteria           CLAUDE.md
```

### The Slash Commands

| Command | What It Does | Time Investment |
|---------|-------------|-----------------|
| `/uc-create` | Interactive Cockburn template wizard (7 steps, scores completeness) | ~5 min |
| `/uc-review` | Devil's Advocate: finds missing extensions, untestable postconditions | ~2 min |
| `/task-decompose` | MSS steps -> tasks, extensions -> additional tasks, dependency graph | ~3 min |
| `/agent-team-plan` | Designs multi-agent team with file ownership matrix | ~2 min |
| `/verify-uc` | Runs postcondition checks after implementation | ~1 min |
| `/grade-work` | Scores against acceptance criteria (weighted rubric) | ~2 min |
| `/retrospective` | Captures what worked/failed, updates CLAUDE.md | ~3 min |
| `/session-handoff` | Writes structured state for cross-session continuity | ~1 min |
| `/parallel-sprint` | Sets up 3 worktrees + branches for parallel agents | ~1 min |
| `/code-quality` | Full quality gate: fmt + clippy + test + cargo-deny | ~30 sec |

### The Cockburn Template (Adapted for Agents)

```markdown
# Use Case: UC-006 Create Room

## Classification
- Goal Level: Sea-level (User Goal)
- Complexity: High
- Priority: P2

## Conditions
- Preconditions: (become setup assertions in tests)
  1. Creator has valid identity keypair
  2. At least one transport available
- Success Postconditions: (become verification assertions)
  1. Room exists in RoomRegistry
  2. Creator is sole member with admin role
  3. Room is discoverable via relay
- Invariants: (become continuous assertions)
  1. Room name uniqueness enforced
  2. Membership list consistency

## Main Success Scenario (9 steps)
## Extensions (14 "what could go wrong" paths)
## Agent Execution Notes (verification command, test file, deps)
## Acceptance Criteria (8 checkboxes)
```

**UC-006 produced 271 lines of specification.** That's not waste -- it's _the implementation plan_.

Extensions alone (name validation edge cases, relay unavailable, duplicate names, full rooms, concurrent joins) produced 6 additional tasks that would have been discovered as bugs otherwise.

### The Quality Gate

Enforced automatically via Claude Code hooks (`PreToolUse` on `git commit`):

```
cargo fmt --check        # formatting
cargo clippy -- -D warnings   # linting (unwrap_used = "deny")
cargo test               # all tests
cargo deny check         # license + dependency audit
```

Also enforced in CI (GitHub Actions) on every push/PR.

---

## 3. The Sword: What We Built

### TermChat: A terminal-native encrypted messenger

```
+------------------+------------------+------------------+
|    Sidebar       |   Chat Panel     |   Task Panel     |
|  - Rooms         |  - Messages      |  - Shared Tasks  |
|  - DMs           |  - Input Box     |  - CRDT Sync     |
|  - Agents        |  - Status        |  - Assignments   |
+------------------+------------------+------------------+
         |                  |                  |
    Application Layer (ChatManager, TaskManager, AgentBridge)
         |                  |                  |
    Transport Layer (QUIC P2P preferred, WebSocket relay fallback)
         |                  |                  |
    Crypto Layer (Noise XX handshake, x25519, ChaCha20-Poly1305)
```

### Three-Crate Workspace

| Crate | Lines | Purpose |
|-------|-------|---------|
| **termchat** | 14,396 | TUI client (ratatui + crossterm), chat/task/agent managers |
| **termchat-proto** | 1,916 | Shared wire protocol (the "contract" crate) |
| **termchat-relay** | 1,974 | Lightweight axum WebSocket relay server |
| **tests/** | 6,884 | Integration + property tests |
| **Total** | **25,170** | |

### Key Design Patterns (Emerged from Spec-Driven Process)

**Hybrid Transport with offline queue:**
```rust
// Try preferred (QUIC P2P), fall back (WebSocket relay), or queue
async fn send(&self, peer: &PeerId, payload: &[u8]) -> Result<(), TransportError> {
    match self.try_send(peer, payload).await {
        Ok(()) => Ok(()),
        Err(err) => {
            self.pending.enqueue(peer.clone(), payload.to_vec()).await;
            Err(err)  // Signal failure even though queued (crucial for UX)
        }
    }
}
```

**CRDT merge (87 lines, 22 tests, zero external deps):**
```rust
// Evaluated `crdts` crate (32 transitive deps). Decision: keep hand-rolled.
// 87 lines of focused LWW logic vs 32 new dependencies.
pub fn merge_lww<T: Clone>(local: &LwwRegister<T>, remote: &LwwRegister<T>)
    -> LwwRegister<T>
{
    if remote.timestamp > local.timestamp
        || (remote.timestamp == local.timestamp && remote.author > local.author)
    { remote.clone() } else { local.clone() }
}
```

**Stub-then-real pattern:** UC-001 (Send Message) shipped with `StubNoiseSession` (XOR cipher). Real Noise XX came 3 sprints later. Zero code changes needed -- trait abstraction made it a drop-in replacement.

---

## 4. Multi-Agent Parallel Swarms

### The Setup

**Requirements:** Claude Opus 4.6 + experimental agent teams feature

```bash
# Enable in ~/.claude/settings.json
{ "env": { "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" } }
```

### Team Structure

```
Lead (Implementation Coordinator)
  |-- Builder-Proto    owns: termchat-proto/, termchat/src/chat/
  |-- Builder-Agent    owns: termchat/src/agent/
  |-- Builder-Infra    owns: termchat/src/crypto/, termchat/src/transport/
  |-- Builder-TUI      owns: termchat/src/ui/, termchat/src/app.rs
  |-- Reviewer         owns: tests/integration/, tests/property/
```

**Key constraint: File ownership is exclusive.** No two builders touch the same file. Lead never edits builder-owned files (uses `SendMessage` instead).

### The Worktree Pattern (Conflict-Free Parallel Development)

```bash
# Each UC gets its own worktree + feature branch
git worktree add ../uc-014-work -b feature/uc-014-chatmanager-refactor
git worktree add ../uc-015-work -b feature/uc-015-agent-crypto-fanout
git worktree add ../uc-016-work -b feature/uc-016-join-relay-routing
```

Three agents work in parallel on isolated worktrees. Each has a full copy of the repo but can only modify their assigned files. Merges happen through the lead.

**Result from our repo: 3 parallel feature branches merged, only 1 conflict** (in UC-016, documented and resolved cleanly).

### Agent Reliability Rules (Learned the Hard Way)

| Rule | Why |
|------|-----|
| **Max ~20 tool calls per task** | Agents lose coherence beyond this; decompose further |
| **`plan_mode_required: true` for teammates** | Forces agents to present plan before implementing |
| **Builders run clippy per-task, not just at gate** | Lint warnings accumulate across parallel tracks |
| **Define proto types BEFORE spawning builders** | Everyone codes against the same contract |
| **Lead uses subagents for all implementation** | Keeps lead's context window clean for coordination |
| **Reviewer role is non-negotiable** | Blind review against postconditions catches real bugs |
| **Gate merges on reviewer approval** | Never merge a feature branch without review |

### Cross-Session Continuity

AI agents have a fundamental limitation: context windows are finite and sessions end.

**Solution: `/session-handoff`** writes structured state to `docs/handoff.md`:
- Git state (branch, dirty files, recent commits)
- In-progress tasks from `docs/tasks/uc-NNN-tasks.md`
- Test/clippy status
- Blockers and decisions made
- Concrete "next session first priority"

**Without this, the next session wastes 20-30% of its context rediscovering state.**

---

## 5. Metrics: What Git Tells Us

### Development Velocity

| Metric | Value |
|--------|-------|
| **Total development time** | 3 calendar days (Feb 7-9, 2026) |
| **Total commits** | 46 |
| **Use cases completed** | 16 (UC-001 through UC-016) |
| **Sprints completed** | 9 |
| **Lines of Rust** | 25,170 |
| **Tests** | 685 passing |
| **AI co-authored commits** | 89% (41 of 46) |

### Quality Signals

| Metric | Value | Interpretation |
|--------|-------|---------------|
| **Rework commits** (fix/revert/broken) | 3 of 41 non-merge (7.3%) | And none were UC logic bugs |
| **Reverts** | 0 | Nothing was rolled back |
| **Merge conflicts** | 1 of 5 merges | File ownership matrix works |
| **Files deleted** | 0 of 58 created | No speculative over-creation |
| **WIP commits** | 1 | Clean commit discipline |

### The One-Shot Success Story

Of 13 "Implement UC-NNN" commits, **zero required follow-up bug-fix commits**.

The 3 "fix" commits addressed:
1. Clippy pedantic warnings across crates (lint config, not logic)
2. Dependency migration (bincode -> postcard, planned)
3. Documentation gaps found by retroactive `/uc-review`

**Inferred first-attempt success rate for UC implementations: 100%**

### Test Growth (Cumulative)

```
Sprint 1 (UC-001, UC-002):  121 tests  ██░░░░░░░░░░░░░░░░░░
Sprint 2 (UC-005):          149 tests  ██░░░░░░░░░░░░░░░░░░
Sprint 3 (UC-003):          190 tests  ███░░░░░░░░░░░░░░░░░
Sprint 4 (UC-004):          247 tests  ████░░░░░░░░░░░░░░░░
Sprint 5 (UC-006):          318 tests  █████░░░░░░░░░░░░░░░
Sprint 6 (UC-007):          405 tests  ██████░░░░░░░░░░░░░░
Sprint 7 (UC-008, UC-009):  499 tests  ████████░░░░░░░░░░░░
Sprint 8 (UC-010-012):      675 tests  ██████████░░░░░░░░░░
Sprint 9 (UC-013):          685 tests  ██████████░░░░░░░░░░
```

Tests grew **5.7x** over the project lifetime. Every UC added tests proportionally -- they were never bolted on after the fact.

### Sprint Velocity (Accelerating)

```
Sprint 1-4:  1 UC/sprint     (learning patterns)
Sprint 5-6:  1 UC/sprint     (growing complexity)
Sprint 7:    2 UCs/sprint    (patterns established)
Sprint 8:    3 UCs/sprint    (parallel execution)
Sprint 9+:   4 UCs/sprint    (parallel worktrees)
```

Velocity increased as the Forge workflow matured and process learnings accumulated in CLAUDE.md.

### File Churn (Low = Good)

Most-changed files are exactly what you'd expect:

| Changes | File | Why |
|---------|------|-----|
| 19 | `CLAUDE.md` | Updated every sprint retrospective |
| 12 | `termchat/Cargo.toml` | New deps per UC |
| 10 | `docs/sprints/current.md` | Sprint tracking |
| 8 | `termchat/src/chat/mod.rs` | Core business logic |
| 7 | `termchat-relay/src/relay.rs` | Relay server features |

Source files average 3-5 changes. No churn, no rewrite cycles.

### The A/B Comparison (from Sprint 9 Retrospective)

| Dimension | Sprint 8 (no Forge, infra scope) | Sprint 9 (full Forge, infra scope) |
|-----------|--------------------------------|-----------------------------------|
| **Quality score** | 2.4 / 5 | 4.6 / 5 |
| **Rework** | Yes | Zero |
| **Orphaned work** | Yes | None |
| **Review defects caught** | N/A | 7 (2 critical, 5 warnings) |
| **Context kills** | Multiple | 0 |

Same scope. Same model. Same developer. The only variable was whether the Forge workflow was followed.

---

## 6. Live Demo

### Demo Script (~2 minutes)

**A. Show the Forge workflow (30 sec)**

```bash
# Show the slash commands
ls .claude/commands/

# Show a use case doc
cat docs/use-cases/uc-006-create-room.md | head -60

# Show the task decomposition
cat docs/tasks/uc-001-tasks.md | head -40
```

**B. Show the codebase (30 sec)**

```bash
# Three-crate workspace
ls -la termchat/src/ termchat-proto/src/ termchat-relay/src/

# Run the quality gate
cargo fmt --check && cargo clippy -- -D warnings

# Run tests (show the count)
cargo test 2>&1 | tail -20
```

**C. Launch the TUI (30 sec)**

```bash
# Start the relay server in background
cargo run --bin termchat-relay &

# Launch the TUI client (offline demo mode)
cargo run
```

Walk through: sidebar, chat panel, task panel, keyboard navigation, theme.

**D. Show parallel branch evidence (30 sec)**

```bash
# Feature branches from parallel agents
git branch -a

# The merge history
git log --oneline --graph | head -15

# Co-authored commits
git log --oneline | head -10
```

---

## 7. Lessons Learned & Adoption Guide

### Top 5 Lessons

1. **Review-before-implement is the single highest-ROI step.** `/uc-review` caught 7 issues in 2 minutes during Sprint 9. Each would have cost 5-10 min if found during implementation. Total: ~5 min invested, ~60 min saved.

2. **Extensions produce ~40% of implementation tasks.** Cockburn's "what could go wrong at each step?" systematically surfaces edge cases that would otherwise become bugs. Never skip extensions.

3. **File ownership eliminates merge conflicts.** Assign exclusive file ownership before spawning parallel agents. In 5 merges across 3 parallel branches, we had exactly 1 conflict.

4. **Process learnings compound.** CLAUDE.md grew from 20 lines to 300+ lines of accumulated wisdom. Each retrospective improved the next sprint. By Sprint 8, velocity had tripled.

5. **External memory is non-negotiable.** Task files (`docs/tasks/`), handoff docs, and retrospectives survive context kills. Without them, 20-30% of each session is wasted on archaeology.

### Adoption Checklist for Your Team

```
[ ] Copy .claude/commands/ and .claude/skills/ to your project
[ ] Write your first use case with /uc-create (~5 min)
[ ] Run /uc-review before any implementation (~2 min)
[ ] Use /task-decompose to get a dependency graph (~3 min)
[ ] Set up CLAUDE.md with coding standards and module ownership
[ ] Add quality gate hooks (fmt + clippy + test)
[ ] After each milestone, run /retrospective and update CLAUDE.md
[ ] For parallel work: git worktree + file ownership matrix
```

### The Key Insight

> **Spec-driven development with AI agents requires not just better specs, but better processes.** Review gates, task decomposition, team coordination, and external memory are what scale AI-assisted development beyond single-agent, single-session work.

The Forge demonstrates that the gap between "AI writes some code" and "AI reliably builds production systems" is bridged by **process discipline**, not model capability.

---

## Appendix: Repository Structure

```
rust-term-chat/
  CLAUDE.md                    # 300+ lines of project state + process learnings
  deny.toml                    # Dependency audit policy
  .claude/
    commands/                  # 11 slash commands (the Forge)
    skills/                    # 3 reusable skills (template, rubric, checklist)
    agents/                    # 2 team configs (requirements, implementation)
    settings.json              # Quality gate hooks
  .github/workflows/ci.yml    # Automated CI (fmt + clippy + test + deny)
  docs/
    termchat-blueprint.md      # Vision doc + roadmap
    use-cases/                 # 16 Cockburn use case documents
    tasks/                     # Task decomposition files per UC
    sprints/current.md         # Sprint tracking
    retrospectives/            # 9 retrospective documents
    handoff.md                 # Cross-session state
  termchat/src/                # 14,396 LOC - TUI client
  termchat-proto/src/          # 1,916 LOC - wire protocol
  termchat-relay/src/          # 1,974 LOC - relay server
  tests/                       # 6,884 LOC - integration + property tests
```

### By the Numbers

| | |
|---|---|
| Lines of Rust | 25,170 |
| Tests passing | 685 |
| Use cases completed | 16 |
| Sprints | 9 |
| Calendar days | 3 |
| Merge conflicts | 1 |
| Reverts | 0 |
| UC logic bugs in production | 0 |
| Retrospectives written | 9 |
| Process learnings captured | 35+ |
| AI co-authorship rate | 89% |

---

*Built with Claude Opus 4.6 + Claude Code + The Forge*
*February 2026*
