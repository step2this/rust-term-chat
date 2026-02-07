---
description: Break a use case into implementable tasks with dependencies and test requirements
allowed-tools: Read, Glob, Grep, Write
---

# Decompose Use Case into Tasks: $ARGUMENTS

You are an **Implementation Coordinator** breaking a Cockburn use case into concrete, implementable tasks for the TermChat project.

## Input

`$ARGUMENTS` is either:
- A UC number (e.g., "001" or "UC-001")
- A file path to a use case document
- A use case name (e.g., "Send Direct Message")

## Step 1: Load the Use Case

Use Glob to find matching files in `docs/use-cases/uc-*.md` and Read to load the content.

If the use case cannot be found, list available use cases and ask the user which one to decompose.

## Step 2: Extract Implementable Units

Map the use case structure to tasks:

### From Main Success Scenario (MSS)
Each MSS step (or logical group of 2-3 tightly coupled steps) becomes a task:
- **What to build**: The system behavior described in the step
- **Input/Output**: What data flows in and out
- **Where it lives**: Which module/file in the codebase (refer to blueprint §2.3 module breakdown)

### From Extensions
Each extension becomes either:
- An **additional task** (if it requires new code paths, e.g., "fallback to relay")
- A **subtask** of the MSS step it extends (if it's error handling within existing code)

### From Preconditions
Each precondition that requires setup becomes:
- A **prerequisite task** (if the setup code doesn't exist yet)
- A **test setup task** (creating test fixtures that establish preconditions)

### From Postconditions
Each postcondition becomes:
- A **test task** (write the test that verifies the postcondition)

## Step 3: Determine Task Dependencies

Build a dependency graph for the tasks:
- MSS tasks are generally sequential (task N depends on task N-1)
- Extension tasks depend on the MSS task they extend
- Test tasks depend on the implementation task they verify
- Prerequisite tasks must come before everything else

## Step 4: Assign to Modules

Map each task to the TermChat module structure:

| Module | Path | Typical Tasks |
|--------|------|---------------|
| Proto | `termchat-proto/src/` | Wire format types, serialization |
| Crypto | `src/crypto/` | Noise handshake, key management |
| Transport | `src/transport/` | P2P, relay, hybrid selection |
| Chat | `src/chat/` | Message types, room management, history |
| UI | `src/ui/` | Panel rendering, input handling |
| App | `src/app.rs` | State machine, event routing |
| Agent | `src/agent/` | Agent bridge, commands |
| Tasks | `src/tasks/` | Task sync, display |
| Relay | `termchat-relay/src/` | Relay server logic |

## Step 5: Estimate Complexity

For each task, estimate:
- **Size**: S (< 50 lines), M (50-200 lines), L (200-500 lines), XL (500+ lines)
- **Risk**: Low (well-understood), Medium (some unknowns), High (spike may be needed)
- **Agent suitability**: Can this be delegated to a teammate agent, or does it need human judgment?

## Step 6: Generate Task List

Write the task list to `docs/tasks/uc-<NNN>-tasks.md` with this structure:

```markdown
# Tasks for UC-<NNN>: <Title>

Generated from use case on <date>.

## Summary
- **Total tasks**: <N>
- **Implementation tasks**: <N>
- **Test tasks**: <N>
- **Critical path**: <task IDs in order>
- **Estimated total size**: <S/M/L/XL>

## Dependency Graph

<Text diagram showing task ordering>

## Tasks

### T-<NNN>-01: <Task Title>
- **Type**: Implementation | Test | Prerequisite | Refactor
- **Module**: <module path>
- **Description**: <what to do>
- **From**: MSS Step <N> | Extension <Na> | Precondition <N> | Postcondition <N>
- **Depends On**: T-<NNN>-<NN>, ...
- **Blocks**: T-<NNN>-<NN>, ...
- **Size**: S | M | L | XL
- **Risk**: Low | Medium | High
- **Agent Assignment**: Teammate:Builder | Teammate:Reviewer | Lead
- **Acceptance Test**: <how to verify this task is done>

<Repeat for each task>

## Implementation Order

<Topologically sorted list>

| Order | Task | Type | Size | Depends On |
|-------|------|------|------|------------|
| 1     | T-<NNN>-01 | ... | ... | none |
| 2     | T-<NNN>-02 | ... | ... | T-<NNN>-01 |
| ...   | ... | ... | ... | ... |

## Notes for Agent Team
- <Any coordination notes: shared state, potential conflicts, review gates>
```

Also create the `docs/tasks/` directory if it doesn't exist.

## Step 7: Report to User

Summarize the decomposition:
- Total tasks and breakdown by type
- Critical path and estimated effort
- Highest-risk tasks that may need spikes
- Suggested implementation order
- Ask if the user wants to adjust anything before proceeding
