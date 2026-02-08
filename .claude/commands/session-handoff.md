---
description: Write structured session state to docs/handoff.md for cross-session continuity
allowed-tools: Read, Glob, Grep, Bash, Write, AskUserQuestion
---

# Session Handoff: $ARGUMENTS

You are writing a **session handoff document** to preserve context for the next Claude Code session. This prevents the 20-30% context waste on archaeology that occurs when continuing work without explicit state transfer.

## Step 1: Auto-Gather Current State

Collect the following information automatically (do not ask the user for these):

### Git State
Run these commands:
- `git status` — current branch, dirty files, staged changes
- `git log --oneline -5` — recent commits
- `git worktree list` — active worktrees
- `git branch -a` — all branches (to identify feature branches in progress)

### Task State
- Use Glob to scan `docs/tasks/uc-*-tasks.md` for task files
- Read any found task files to identify incomplete tasks (tasks not marked with `[x]` or status != "completed")
- Scan `docs/sprints/current.md` if it exists for sprint context

### Test State
- Run `cargo test 2>&1 | tail -5` to capture current test status (pass/fail count)
- Run `cargo clippy -- -D warnings 2>&1 | tail -3` to capture lint status

### Active Context
- Read `docs/handoff.md` if it exists (to understand what the *previous* session handed off)
- Read `CLAUDE.md` Project State section for current milestone context

## Step 2: Ask the User for Session-Specific Context

If `$ARGUMENTS` is provided, use it as the "what was accomplished" input. Otherwise, ask the user:

Use AskUserQuestion to gather:

1. **What was accomplished this session?** (free text — what did you build, fix, or decide?)
2. **Are there any blockers?** (things that prevented progress or need resolution)
3. **What should the next session do first?** (the single most important thing to start with)

## Step 3: Write docs/handoff.md

Write the handoff document with this structure:

```markdown
# Session Handoff

Written: <current date and time>
Previous session: <date from previous handoff if it existed, otherwise "N/A">

## What Was Accomplished

<User's description of what was done this session>

## Current State

### Branch & Working Tree
- **Branch**: <current branch>
- **Clean/Dirty**: <clean or list dirty files>
- **Active worktrees**: <list or "none">

### Recent Commits
<last 5 commits from git log>

### Test Status
- **Tests**: <pass count> passing, <fail count> failing
- **Clippy**: <clean or number of warnings>

## In-Progress Tasks

<List any tasks from docs/tasks/ that are not complete>
<If no task files exist, note "No task files found">

## Blockers

<User's blockers, or "None" if none reported>

## Decisions Made This Session

<Any architectural or process decisions that affect future work>
<If the user didn't mention any, note "None recorded — consider documenting decisions explicitly in future sessions">

## Next Session Instructions

**Start by reading this file**: `docs/handoff.md`

**First priority**: <user's stated first priority for next session>

**Then**:
1. Check `docs/tasks/` for any incomplete task files
2. Check `docs/sprints/current.md` for sprint context
3. Run `cargo test` to verify current state
4. Continue from where this session left off

## Important File Paths

<List any files that were actively being worked on, based on git status and user input>
```

## Step 4: Validate the Handoff

After writing, verify:
1. The file exists at `docs/handoff.md`
2. All auto-gathered sections have real data (not placeholders)
3. The "Next Session Instructions" section has a concrete first priority

## Step 5: Remind the User

Output this message:

> Handoff written to `docs/handoff.md`.
>
> **Next session**: Start by telling Claude to "Read `docs/handoff.md` and continue from where the last session left off."
>
> Consider committing the handoff file so it's tracked in version control:
> ```
> git add docs/handoff.md && git commit -m "Session handoff: <brief description>"
> ```
