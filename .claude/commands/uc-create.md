---
description: Interactively create a Cockburn-style use case document for the TermChat project
allowed-tools: Read, Glob, Write, AskUserQuestion
---

# Create a Use Case: $ARGUMENTS

You are a **Requirements Architect** creating a fully-dressed Cockburn use case for the TermChat project. The use case goal is: **$ARGUMENTS**

## Step 1: Determine the Next UC Number

Scan the `docs/use-cases/` directory for existing use case files matching the pattern `uc-*.md`. Extract the highest UC number and increment by 1. If no files exist, start at UC-001.

Use the Glob tool to find files matching `docs/use-cases/uc-*.md`.

## Step 2: Validate the Goal

The use case title MUST be an **Active Verb Phrase Goal** (e.g., "Send Direct Message", "Establish P2P Connection", "Create Chat Room"). If `$ARGUMENTS` is not an active verb phrase, rephrase it into one and confirm with the user.

## Step 3: Walk Through Each Section Interactively

Do NOT dump the entire template at once. Instead, work through the sections in conversational groups, asking clarifying questions at each stage. Use AskUserQuestion when you need choices or confirmation.

### Group 1: Classification

Ask the user to classify the use case. Present these options:

- **Goal Level**: ☁️ Summary (cloud — high-level business goal) | 🌊 User Goal (sea — what a user sits down to do) | 🐟 Subfunction (fish — a step within a larger goal)
- **Scope**: System (black box — external behavior) | Component (white box — internal design)
- **Priority**: P0 Critical | P1 High | P2 Medium | P3 Low
- **Complexity**: 🟢 Low | 🟡 Medium | 🔴 High | ⚫ Spike needed

Suggest reasonable defaults based on the goal description and let the user confirm or override.

### Group 2: Actors

Ask the user:
- Who is the **Primary Actor** (who initiates this use case)?
- What **Supporting Actors** are involved (systems, services, other users)?
- What **Stakeholders** care about this, and what are their interests?

For TermChat, common actors include: Terminal User (Sender/Recipient), Transport Layer, Crypto Layer, Relay Server, Agent Bridge, Task Manager.

### Group 3: Conditions

Walk through each condition type, explaining what it maps to in practice:

- **Preconditions** (must be true before starting — these become setup assertions in tests): What must already be in place?
- **Success Postconditions** (true when done right — these become verification assertions): What is true when this succeeds?
- **Failure Postconditions** (true when it fails gracefully — these become failure-mode tests): What should be true even when things go wrong?
- **Invariants** (must remain true throughout — these become continuous assertions): What must NEVER be violated during execution?

IMPORTANT: Every postcondition must be **testable and automatable**. If a postcondition cannot be verified by a command or test, flag it and ask the user to rephrase.

### Group 4: Main Success Scenario (MSS)

Build the happy path step by step:
1. Start with the Primary Actor's triggering action
2. Alternate between Actor actions and System responses
3. End with the success postcondition being achieved
4. Number every step — these numbers are referenced by Extensions

Ask the user to walk you through what happens when everything goes right.

### Group 5: Extensions (What Can Go Wrong)

This is the MOST IMPORTANT section. For each step in the MSS, systematically ask: **"What could go wrong at step N?"**

Rules for extensions:
- Each extension MUST reference a specific MSS step number (e.g., "2a", "5b")
- Use letter suffixes for multiple extensions at the same step (2a, 2b, 2c)
- Each extension must specify either "returns to step X" or "use case fails"
- Cover at minimum: validation failures, network errors, timeout conditions, authentication/authorization failures, resource exhaustion

Do NOT skip this section. Cockburn says extensions capture 80% of the real development work.

### Group 6: Variations

Ask about alternative paths that aren't errors but different ways to achieve the same step. For example: different input methods, alternative UX flows, or optional features.

### Group 7: Agent Execution Notes

Fill in the operational details for AI agent consumption:
- **Verification Command**: The shell command to verify postconditions (usually `cargo test --test <name>`)
- **Test File**: Path to the integration test (usually `tests/integration/<slug>.rs`)
- **Depends On**: Which other use cases must be completed first? (e.g., UC-005 for E2E Handshake)
- **Blocks**: Which use cases cannot start until this one is done?
- **Estimated Complexity**: T-shirt size (S/M/L/XL) and token budget hint
- **Agent Assignment**: Lead | Teammate:Builder | Teammate:Reviewer | Subagent

### Group 8: Acceptance Criteria

Generate acceptance criteria based on the postconditions and extensions. Always include these standard criteria:
- [ ] All success postconditions verified by automated test
- [ ] All extension paths have explicit handling
- [ ] No invariant violations detected
- [ ] Code passes lint + clippy (`cargo clippy -- -D warnings`)
- [ ] Reviewer agent approves

Add use-case-specific criteria based on the postconditions defined in Group 3.

## Step 4: Score Completeness

After drafting the full use case, evaluate it against this checklist and report a score:

| Criterion | Weight | Check |
|-----------|--------|-------|
| Title is active verb phrase | 5% | Is it? |
| All classification fields filled | 5% | Any missing? |
| Primary actor identified | 5% | Clear who initiates? |
| At least 2 stakeholder interests | 5% | Enough perspectives? |
| At least 2 preconditions | 10% | Setup is clear? |
| At least 2 success postconditions | 10% | Verifiable outcomes? |
| At least 1 failure postcondition | 5% | Graceful failure defined? |
| At least 1 invariant | 5% | Safety constraints exist? |
| MSS has 5+ steps | 10% | Enough detail? |
| Extensions cover 50%+ of MSS steps | 15% | Error paths explored? |
| All postconditions are testable | 10% | Can be automated? |
| Verification command specified | 5% | Agent can verify? |
| Dependencies listed | 5% | Ordering is clear? |
| Acceptance criteria present | 5% | Grading is possible? |

Report the total score as a percentage. Flag any items scoring 0 and suggest how to fill the gaps. A score below 70% means the use case needs more work before implementation.

## Step 5: Write the Use Case File

Generate the kebab-case slug from the title (e.g., "Send Direct Message" → "send-direct-message").

Write the final document to: `docs/use-cases/uc-<NNN>-<slug>.md`

where `<NNN>` is the zero-padded three-digit number (e.g., 001, 002, 012).

Use this exact template structure:

```markdown
# Use Case: UC-<NNN> <Active Verb Phrase Goal>

## Classification
- **Goal Level**: <selected>
- **Scope**: <selected>
- **Priority**: <selected>
- **Complexity**: <selected>

## Actors
- **Primary Actor**: <identified>
- **Supporting Actors**: <identified>
- **Stakeholders & Interests**:
  - <Stakeholder>: <interest>

## Conditions
- **Preconditions** (must be true before starting):
  1. <condition>
- **Success Postconditions** (true when done right):
  1. <condition>
- **Failure Postconditions** (true when it fails gracefully):
  1. <condition>
- **Invariants** (must remain true throughout):
  1. <condition>

## Main Success Scenario
1. <step>
2. <step>
...

## Extensions (What Can Go Wrong)
- **Na. <condition at step N>**:
  1. <handling>
  2. <resolution>

## Variations
- **Na.** <variation description>

## Agent Execution Notes
- **Verification Command**: `<command>`
- **Test File**: `<path>`
- **Depends On**: <dependencies>
- **Blocks**: <dependents>
- **Estimated Complexity**: <size>
- **Agent Assignment**: <assignment>

## Acceptance Criteria (for grading)
- [ ] All success postconditions verified by automated test
- [ ] All extension paths have explicit handling
- [ ] No invariant violations detected
- [ ] Code passes lint + clippy (for Rust)
- [ ] Reviewer agent approves
- [ ] <use-case-specific criteria>
```

After writing, confirm the file path and completeness score to the user.

## Reference: Quality Bar

See the example UC-001 (Send Direct Message) in `docs/termchat-blueprint.md` section 2.5 for the quality bar. Your output should match that level of detail and specificity.
