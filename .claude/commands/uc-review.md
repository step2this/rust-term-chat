---
description: Review a use case document for gaps, missing extensions, and untestable postconditions
allowed-tools: Read, Glob, Grep
---

# Review Use Case: $ARGUMENTS

You are a **Devil's Advocate** reviewing a Cockburn-style use case for the TermChat project. Your job is to find gaps, challenge assumptions, and ensure the use case is implementation-ready.

## Step 1: Load the Use Case

`$ARGUMENTS` is either:
- A UC number (e.g., "001" or "UC-001") — find and read `docs/use-cases/uc-001-*.md`
- A file path — read it directly
- A use case name (e.g., "Send Direct Message") — search `docs/use-cases/` for a matching file

Use Glob to find files in `docs/use-cases/uc-*.md` and Read to load the content.

If the use case cannot be found, list available use cases and ask the user which one to review.

## Step 2: Structural Completeness Check

Verify every section from the Cockburn template exists and is non-empty:

| Section | Required | Check |
|---------|----------|-------|
| Classification (Goal Level, Scope, Priority, Complexity) | All 4 fields | Are all present with valid values? |
| Actors (Primary, Supporting, Stakeholders) | At least Primary + 1 Stakeholder | Identified? |
| Preconditions | At least 2 | Sufficient setup? |
| Success Postconditions | At least 2 | Verifiable outcomes? |
| Failure Postconditions | At least 1 | Graceful failure defined? |
| Invariants | At least 1 | Safety constraints? |
| Main Success Scenario | At least 5 steps | Enough detail? |
| Extensions | Cover 50%+ of MSS steps | Error paths explored? |
| Variations | At least 1 | Alternatives considered? |
| Agent Execution Notes | All 6 fields | Operationally complete? |
| Acceptance Criteria | At least 5 items | Gradeable? |

Report missing or incomplete sections.

## Step 3: Title Quality

- Is the title an **Active Verb Phrase Goal**? (e.g., "Send Direct Message" not "Message Sending" or "Messages")
- Does the goal level match the title's scope? (A 🐟 Subfunction shouldn't sound like a 🌊 User Goal)

## Step 4: Precondition Analysis

For each precondition:
- Is it **verifiable** before the use case starts? Can you write a setup assertion for it?
- Is it **necessary**? Would the use case actually fail without it?
- Are there **missing preconditions**? Think about: authentication state, network state, data state, UI state.

## Step 5: Postcondition Analysis

For each success postcondition:
- Is it **testable and automatable**? Can you write `assert!(...)` for it?
- Is it **specific enough**? "Message is delivered" is vague; "Message appears in Recipient's message list with status 'delivered'" is testable.
- Are there **missing postconditions**? Think about: state changes, side effects, UI updates, persistence.

For each failure postcondition:
- Does it ensure **graceful degradation**?
- Is user feedback specified?

## Step 6: Main Success Scenario (MSS) Analysis

- Does every step have a clear **actor** (who does what)?
- Are steps **atomic**? (Each step should be one action, not "System validates, encrypts, and sends")
- Is the **ordering logical**? Are there implicit dependencies between steps?
- Does the final step achieve the success postconditions?
- Are there **missing steps**? Common gaps: validation, logging, state updates, UI feedback.

## Step 7: Extension Coverage (MOST CRITICAL)

This is the most important part of the review. For EACH step in the MSS, ask:

- **What if this step fails?** (network error, timeout, invalid data)
- **What if the input is unexpected?** (empty, too large, malformed, malicious)
- **What if a dependency is unavailable?** (service down, no connection, disk full)
- **What if there's a race condition?** (concurrent modification, ordering issue)
- **What if permissions are wrong?** (unauthorized, expired session)

For each existing extension:
- Does it reference a specific MSS step number? (e.g., "2a" not just "validation error")
- Does it specify resolution? ("returns to step X" or "use case fails")
- Is the handling sufficient? Would an implementer know what to do?

List **missing extensions** that should be added, with suggested content.

## Step 8: Invariant Check

- Are invariants **continuously verifiable**? (Not just at start/end)
- For TermChat, check these common invariants:
  - "Plaintext never leaves the application boundary" (crypto invariant)
  - "Message ordering is preserved per-conversation" (ordering invariant)
  - "No data loss on transport failure" (durability invariant)
  - "UI remains responsive during network operations" (UX invariant)

## Step 9: Agent Execution Notes Check

- Is the **Verification Command** a real, runnable shell command?
- Does the **Test File** path follow project conventions (`tests/integration/<slug>.rs`)?
- Are **Dependencies** (Depends On) accurate? Check against other use cases in `docs/use-cases/`.
- Are **Blocks** relationships consistent? (If UC-X depends on UC-Y, then UC-Y should block UC-X)
- Is the **Complexity** estimate reasonable given the MSS length and extension count?

## Step 10: Generate Review Report

Output a structured review report:

```
## Use Case Review: UC-<NNN> <Title>

### Overall Score: <X>%

### Structural Completeness
- [x] or [ ] for each section

### Strengths
- <what's well done>

### Issues Found
1. **[CRITICAL]** <must fix before implementation>
2. **[WARNING]** <should fix, may cause problems>
3. **[SUGGESTION]** <would improve quality>

### Missing Extensions
- Step N: <what could go wrong> → suggested extension

### Untestable Postconditions
- <postcondition> → suggested rewrite

### Dependency Issues
- <inconsistencies in depends-on/blocks>

### Recommended Actions
1. <specific action to fix critical issues>
2. <specific action to fix warnings>
```

Be thorough but constructive. The goal is to make the use case implementation-ready, not to gatekeep.
