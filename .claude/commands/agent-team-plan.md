---
description: Design an agent team configuration for implementing a set of tasks
allowed-tools: Read, Glob, Grep, Write
---

# Plan Agent Team: $ARGUMENTS

You are a **Team Architect** designing an agent team configuration to implement tasks for the TermChat project.

## Input

`$ARGUMENTS` is either:
- A UC number (e.g., "001") — load tasks from `docs/tasks/uc-001-tasks.md`
- A task file path — load directly
- "all" — plan a team for all pending tasks across all use cases
- A sprint name — plan a team for tasks in that sprint

## Step 1: Load Tasks

Use Glob and Read to load the relevant task file(s) from `docs/tasks/`.

If no task files exist, suggest running `/task-decompose` first.

## Step 2: Analyze Task Graph

From the loaded tasks, determine:
- **Parallelizable groups**: Tasks with no dependencies between them that can run concurrently
- **Sequential chains**: Tasks that must happen in order
- **Review gates**: Points where work should be checked before continuing
- **Risk hotspots**: High-risk tasks that need extra attention

## Step 3: Design Team Roles

Based on the task analysis, assign roles from the blueprint's Implementation Team pattern (§1.4):

### Standard Roles

| Role | Responsibility | When to Include |
|------|---------------|-----------------|
| **Lead (Implementation Coordinator)** | Manages task list, routes work, monitors conflicts | Always |
| **Builder** | Writes Rust code, follows TDD | When there are implementation tasks |
| **Reviewer** | Reviews code against postconditions, runs checks | When there are 3+ implementation tasks |
| **Documentation** | Updates docs, CLAUDE.md, use case registry | When there are doc-affecting changes |

For larger task sets, consider multiple Builders working in parallel on independent task groups.

### Role Assignment Rules
- Each task gets exactly one **owner** (primary agent)
- High-risk tasks should have the Reviewer assigned as a **gate** (must approve before dependent tasks start)
- Test tasks can be assigned to the Builder (TDD: write test first) or a dedicated Test Writer
- Documentation tasks are batched and assigned after implementation tasks complete

## Step 4: Define Review Gates

Insert review checkpoints in the task flow:

1. **After prerequisite tasks**: Verify foundations before building on them
2. **After each MSS task group**: Verify the happy path works incrementally
3. **After extension tasks**: Verify error handling is correct
4. **After all implementation**: Full integration check
5. **Before marking UC complete**: Run verification command from use case

Each gate specifies:
- What the Reviewer checks
- What commands to run (`cargo test`, `cargo clippy`, specific test files)
- Pass/fail criteria
- What happens on failure (rework task assigned back to Builder)

## Step 5: Generate Team Configuration

Write the team plan to `docs/teams/uc-<NNN>-team.md` (create `docs/teams/` if needed):

```markdown
# Agent Team Plan: UC-<NNN> <Title>

Generated on <date>.

## Team Composition

| Role | Agent | Responsibilities |
|------|-------|-----------------|
| Lead | Implementation Coordinator | Manage task list, route work, resolve conflicts |
| Builder | Builder-1 | Write implementation code (TDD) |
| Reviewer | Reviewer-1 | Review code, run quality checks |
| Docs | Documentation | Update docs and registry |

## Task Assignment

| Task | Owner | Reviewer Gate | Priority |
|------|-------|--------------|----------|
| T-<NNN>-01 | Builder-1 | — | 1 |
| T-<NNN>-02 | Builder-1 | Gate 1 | 2 |
| ... | ... | ... | ... |

## Execution Phases

### Phase 1: Prerequisites
- Tasks: <list>
- Gate: Reviewer verifies foundations
- Commands: `cargo build`, `cargo test --lib`

### Phase 2: Happy Path
- Tasks: <list>
- Gate: Reviewer verifies MSS implementation
- Commands: `cargo test --test <integration_test>`

### Phase 3: Error Handling
- Tasks: <list>
- Gate: Reviewer verifies extensions
- Commands: `cargo test`, `cargo clippy -- -D warnings`

### Phase 4: Polish & Verify
- Tasks: <list>
- Gate: Full verification
- Commands: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`

## Review Gates

### Gate 1: <Name>
- **After**: <task list>
- **Reviewer checks**: <what to verify>
- **Commands**: `<commands>`
- **Pass criteria**: <criteria>
- **On failure**: <rework plan>

## Parallelization Opportunities

<Which tasks can run concurrently, and what to watch out for (merge conflicts, shared state)>

## Risk Mitigation

| Risk | Task(s) | Mitigation |
|------|---------|------------|
| <risk> | T-<NNN>-XX | <strategy> |

## Coordination Notes
- <Shared files that multiple agents may touch>
- <Module boundaries to respect>
- <Communication protocol between agents>
```

## Step 6: Generate Claude Code Team Spawn Commands

Provide the actual commands to set up the team:

```
# Create the team
Use TeamCreate with team_name: "uc-<NNN>-impl"

# Create tasks in the shared task list
Use TaskCreate for each task

# Spawn teammates
Use Task tool with team_name and appropriate subagent_type for each role

# The Lead then assigns tasks and monitors progress
```

## Step 7: Report to User

Summarize:
- Team size and roles
- Number of phases and review gates
- Parallelization opportunities
- Estimated coordination overhead
- Ask if the user wants to adjust team composition or task assignments before spawning
