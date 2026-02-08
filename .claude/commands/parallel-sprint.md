---
description: Set up 3 parallel Claude sessions on worktrees for a sprint
allowed-tools: Read, Glob, Grep, Bash, Write, Edit
---

# Parallel Sprint Setup: $ARGUMENTS

You are a **Sprint Coordinator** setting up parallel Claude Code sessions for the TermChat project. Each session works on an isolated git worktree with its own feature branch, preventing merge conflicts.

## Input

`$ARGUMENTS` is either:
- A sprint name (e.g., "sprint-9")
- A list of UC numbers (e.g., "013 014 015")
- A single UC with work packages (e.g., "013" — will be split into parallel tracks)
- Empty — analyze pending work and suggest a split

## Step 1: Analyze Available Work

Read the current sprint doc (`docs/sprints/current.md`) and backlog (`docs/backlog.md`) to understand what needs to be done.

If UC numbers were provided, read each use case doc from `docs/use-cases/`.
If task decomposition files exist in `docs/tasks/`, read those too.

Identify parallelizable work — tasks that don't share files or have dependencies between them.

## Step 2: Design the 3-Track Split

Divide work into 3 tracks that can run concurrently. Follow these rules:

### File Ownership Rules (from CLAUDE.md)
- **Track 1 (Lead)**: Root `Cargo.toml`, `*/Cargo.toml`, `CLAUDE.md`, `.github/`, `docs/`
- **Track 2 (Builder-A)**: Assigned production source files (no overlap with Track 3)
- **Track 3 (Builder-B)**: Assigned production source files (no overlap with Track 2)

### Dependency Rules
- Shared workspace dependencies MUST be added to root `Cargo.toml` by Track 1 FIRST
- `lib.rs` module declarations MUST be added by Track 1 FIRST
- Track 2 and Track 3 should rebase on main after Track 1's prep commit
- No two tracks may edit the same file

### Merge Order
Tracks merge in dependency order:
1. Track 1 merges first (shared deps, Cargo.toml, lib.rs, docs)
2. Track 2 and Track 3 merge in any order (no overlap)
3. Final integration verification on main after all merges

## Step 3: Create Worktrees and Branches

For each track, create a git worktree:

```bash
# From the main repo root
git worktree add ../rust-term-chat-track2 -b feature/<track2-branch-name>
git worktree add ../rust-term-chat-track3 -b feature/<track3-branch-name>
```

Track 1 (Lead) works on main or its own feature branch in the primary worktree.

## Step 4: Write the Coordination File

Create `docs/sessions.md` with the session plan:

```markdown
# Active Parallel Sessions

Sprint: <sprint name>
Created: <date>

## Track Assignment

| Track | Worktree | Branch | Work Items | Files Owned | Status |
|-------|----------|--------|------------|-------------|--------|
| Track 1 (Lead) | `/home/ubuntu/rust-term-chat` | `main` | <items> | <files> | Not started |
| Track 2 | `/home/ubuntu/rust-term-chat-track2` | `feature/<name>` | <items> | <files> | Not started |
| Track 3 | `/home/ubuntu/rust-term-chat-track3` | `feature/<name>` | <items> | <files> | Not started |

## Merge Queue

1. Track 1 prep commit (shared deps) → main
2. Track 2 rebase on main → implement → merge to main
3. Track 3 rebase on main → implement → merge to main
4. Final integration gate on main

## File Ownership Matrix

| File/Module | Track 1 | Track 2 | Track 3 |
|-------------|---------|---------|---------|
| Cargo.toml (root) | ✅ OWNER | ❌ | ❌ |
| ... | ... | ... | ... |

## Communication Protocol

- Each track reads `docs/sessions.md` at start
- Each track updates its status before starting and after completing
- To request a change in another track's files, update the "Requests" section below
- The Lead (Track 1) resolves all requests and merge conflicts

## Requests

| From | To | Request | Status |
|------|-----|---------|--------|
```

## Step 5: Generate Session Launch Instructions

Output clear, copy-pastable instructions for the user to spawn each session:

```markdown
## How to Launch

### Terminal 1 (Track 1 — Lead)
Already running (this session). Working on: <items>

### Terminal 2 (Track 2)
```bash
cd /home/ubuntu/rust-term-chat-track2
claude
```
Then tell Claude:
> Read `docs/sessions.md` and `docs/use-cases/uc-<NNN>.md`. You are Track 2.
> Your work items: <list>
> Your file ownership: <list>
> First: `git rebase main` to pick up shared deps.
> Then implement your tasks. Run `cargo fmt && cargo clippy -- -D warnings && cargo test` before committing.
> When done, update your status in `docs/sessions.md` and commit.

### Terminal 3 (Track 3)
```bash
cd /home/ubuntu/rust-term-chat-track3
claude
```
Then tell Claude:
> Read `docs/sessions.md` and `docs/use-cases/uc-<NNN>.md`. You are Track 3.
> <same format as Track 2>
```

## Step 6: Prep Commit (Track 1)

If any shared dependencies or module declarations need to be added before parallel work begins:

1. Add workspace dependencies to root `Cargo.toml`
2. Add `pub mod` declarations to `lib.rs` if new modules are needed
3. Create any new empty files that Track 2/3 will populate
4. Commit to main: `git commit -m "Prep: add shared deps for <sprint>"`
5. The user will tell Track 2/3 sessions to rebase

## Step 7: Merge Protocol

After all tracks complete, provide merge commands:

```bash
# From main repo
cd /home/ubuntu/rust-term-chat

# Merge Track 2
git merge feature/<track2-branch>

# Verify
cargo fmt --check && cargo clippy -- -D warnings && cargo test

# Merge Track 3
git merge feature/<track3-branch>

# Final verification
cargo fmt --check && cargo clippy -- -D warnings && cargo test

# Cleanup worktrees
git worktree remove ../rust-term-chat-track2
git worktree remove ../rust-term-chat-track3
```

## Rules for Track Agents

When spawned as a Track agent, follow these rules:
1. **Read `docs/sessions.md` first** — understand your role and file ownership
2. **Never edit files outside your ownership** — if you need a change, add a Request in `docs/sessions.md`
3. **Run quality gate before committing** — `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
4. **Commit per-task** — not one monolithic commit
5. **Update your status** in `docs/sessions.md` when starting and finishing each work item
6. **Rebase on main** if the Lead has pushed shared dependency changes
