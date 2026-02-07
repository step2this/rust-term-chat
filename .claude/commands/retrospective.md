---
description: Capture lessons learned after completing a milestone and feed improvements back into tooling
allowed-tools: Read, Glob, Grep, Write
---

# Retrospective: $ARGUMENTS

You are a **Process Improvement Agent** conducting a retrospective after completing a milestone in the TermChat project. The goal is to capture what worked, what didn't, and feed concrete improvements back into CLAUDE.md, commands, and project tooling.

## Input

`$ARGUMENTS` is either:
- A milestone/sprint name (e.g., "Sprint 1", "Phase 0")
- A UC number or range (e.g., "001-005") — retro on those use cases
- "forge" — retro on the meta-tooling itself
- Empty — retro on the most recent body of work

## Step 1: Gather Evidence

Collect data from multiple sources:

### Use Case Documents
Use Glob to scan `docs/use-cases/uc-*.md` for all use cases completed in this milestone.

### Task Files
Use Glob to scan `docs/tasks/uc-*-tasks.md` for task completion data.

### Grade Reports
Use Glob to check for any grade reports in `docs/` that indicate quality outcomes.

### Git History
If available, note the scope of changes (number of files, lines changed) to gauge effort.

### CLAUDE.md
Read the current `CLAUDE.md` to understand existing project standards.

## Step 2: Analyze What Worked

For each category, identify successes:

### Process
- Did the use case → task → implement → verify pipeline work smoothly?
- Were the Cockburn templates useful? Did they catch issues early?
- Did agent teams coordinate effectively?

### Technical
- Which Rust patterns or crates worked well?
- Were there architectural decisions that paid off?
- Did the test strategy catch real bugs?

### Tooling
- Which slash commands were most useful?
- Did the quality gates (fmt, clippy, test) catch issues?
- Were the review processes valuable?

## Step 3: Analyze What Didn't Work

For each category, identify pain points:

### Process
- Were there use cases that were underspecified? Which sections were most often incomplete?
- Did task decomposition match actual implementation? Were tasks too large or too small?
- Were there coordination bottlenecks?

### Technical
- Which Rust concepts caused the most friction?
- Were there crate choices that didn't pan out?
- Were there architectural decisions that need revisiting?

### Tooling
- Which slash commands were unused or unhelpful?
- Were quality gates too strict or too lenient?
- Did the review process slow things down without adding value?

## Step 4: Identify Patterns

Look for recurring themes:

- **Repeated mistakes**: Same type of error appearing across multiple use cases
- **Consistent gaps**: Sections of the template that are always weak
- **Efficiency wins**: Shortcuts or patterns that consistently saved time
- **Bottlenecks**: Steps that consistently slowed progress

## Step 5: Generate Concrete Improvements

For each pain point, propose a specific, actionable improvement:

| Pain Point | Improvement | Applies To |
|-----------|-------------|------------|
| Extensions always incomplete | Add "common extensions checklist" to `/uc-create` | `.claude/commands/uc-create.md` |
| Tests written after code | Enforce TDD in task decomposition | `.claude/commands/task-decompose.md` |
| Clippy warnings at end | Add clippy to pre-commit hook | `.claude/hooks/` |
| <pattern> | <improvement> | <where to apply> |

Categorize improvements as:
- **CLAUDE.md update**: New coding standard, pattern, or instruction
- **Command update**: Improvement to an existing slash command
- **New command**: A slash command that would have helped
- **Hook**: An automated check that should run
- **Template update**: Change to the Cockburn template itself
- **Process change**: Non-tooling workflow improvement

## Step 6: Write the Retrospective Document

Write to `docs/retrospectives/<milestone-slug>.md`:

```markdown
# Retrospective: <Milestone Name>

Date: <date>
Scope: <what was covered>

## Summary
<2-3 sentence overview of the milestone>

## Metrics
- Use cases completed: <N>
- Tasks completed: <N>
- Average grade: <letter>
- Tests written: <N>
- Defects found by review: <N>
- Defects found in production: <N>

## What Worked
1. <success with evidence>
2. <success with evidence>
3. ...

## What Didn't Work
1. <pain point with evidence>
2. <pain point with evidence>
3. ...

## Patterns Observed
1. <recurring theme>
2. <recurring theme>

## Action Items

### Immediate (apply now)
| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | <action> | <file/tool> | <who> |

### Next Sprint (apply before starting)
| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | <action> | <file/tool> | <who> |

### Backlog (nice to have)
| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | <action> | <file/tool> | <who> |

## Key Learnings
1. <insight that should be remembered>
2. <insight that should be remembered>

## Process Rating
- Use Case Quality: <1-5 stars>
- Task Decomposition: <1-5 stars>
- Agent Coordination: <1-5 stars>
- Quality Gates: <1-5 stars>
- Overall: <1-5 stars>
```

Create the `docs/retrospectives/` directory if it doesn't exist.

## Step 7: Apply Immediate Improvements

For any action items marked "Immediate", offer to apply them now:

1. If updating CLAUDE.md — show the proposed change and ask for confirmation
2. If updating a slash command — show the diff and ask for confirmation
3. If adding a hook — write the hook file and explain what it does

Ask the user which improvements to apply immediately.

## Step 8: Report Summary

Give the user a concise summary:
- Top 3 things that worked
- Top 3 things to improve
- Number of action items by priority
- Which improvements are ready to apply now
