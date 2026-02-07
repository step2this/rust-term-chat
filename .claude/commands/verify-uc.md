---
description: Run postcondition checks and verification commands for a completed use case
allowed-tools: Read, Glob, Grep, Bash
---

# Verify Use Case: $ARGUMENTS

You are a **Quality Gate Agent** verifying that a completed use case implementation meets all its specified conditions for the TermChat project.

## Input

`$ARGUMENTS` is either:
- A UC number (e.g., "001" or "UC-001")
- A file path to a use case document
- A use case name (e.g., "Send Direct Message")

## Step 1: Load the Use Case

Use Glob to find matching files in `docs/use-cases/uc-*.md` and Read to load the content.

Extract:
- Verification Command
- Test File path
- All Success Postconditions
- All Failure Postconditions
- All Invariants
- All Acceptance Criteria

## Step 2: Run the Verification Command

Execute the verification command from the use case's Agent Execution Notes section (e.g., `cargo test --test send_receive`).

Capture and report:
- Exit code (0 = pass, non-zero = fail)
- Test output (pass/fail counts, any failure messages)
- Duration

If the verification command is not specified or is empty, flag this as a **critical gap**.

## Step 3: Run the Full Quality Gate

Run the standard quality checks:

```bash
cargo fmt --check
```

```bash
cargo clippy -- -D warnings
```

```bash
cargo test
```

Report results for each command separately.

## Step 4: Check Postcondition Coverage

For each **Success Postcondition** in the use case:
1. Search the test file (from Agent Execution Notes) for assertions that verify this postcondition
2. If a test exists, report it as **COVERED**
3. If no test exists, report it as **UNCOVERED** — this is a gap

Use Grep to search test files for relevant assertions (`assert`, `assert_eq`, `assert_ne`, `expect`).

For each **Failure Postcondition**:
1. Search for tests that verify graceful failure behavior
2. Check for error handling paths (look for `Result`, `Err`, `?` operator)

For each **Invariant**:
1. Search for continuous checks or property-based tests
2. Flag invariants that have no automated verification

## Step 5: Check Extension Coverage

For each Extension in the use case:
1. Search the codebase for the handling code (error branches, match arms, fallback logic)
2. Search test files for tests that exercise the extension path
3. Report coverage: **IMPLEMENTED + TESTED**, **IMPLEMENTED (untested)**, or **MISSING**

## Step 6: Check Acceptance Criteria

Go through each acceptance criterion checkbox:
1. For automated criteria (tests pass, clippy passes, etc.) — verify by running commands
2. For subjective criteria (reviewer approval, etc.) — report as **NEEDS HUMAN REVIEW**
3. Mark each as **PASS**, **FAIL**, or **NEEDS REVIEW**

## Step 7: Generate Verification Report

Output a structured report:

```markdown
## Verification Report: UC-<NNN> <Title>

### Overall Status: PASS | FAIL | PARTIAL

### Verification Command
- Command: `<command>`
- Result: PASS | FAIL
- Output: <summary>

### Quality Gate
| Check | Status | Details |
|-------|--------|---------|
| `cargo fmt --check` | PASS/FAIL | <details> |
| `cargo clippy -- -D warnings` | PASS/FAIL | <details> |
| `cargo test` | PASS/FAIL | <N> passed, <N> failed |

### Postcondition Coverage
| # | Postcondition | Status | Test |
|---|--------------|--------|------|
| 1 | <postcondition> | COVERED/UNCOVERED | <test name or "none"> |

### Invariant Coverage
| # | Invariant | Status | Mechanism |
|---|-----------|--------|-----------|
| 1 | <invariant> | VERIFIED/UNVERIFIED | <how it's checked> |

### Extension Coverage
| Extension | Implementation | Test |
|-----------|---------------|------|
| <Na. description> | IMPLEMENTED/MISSING | TESTED/UNTESTED |

### Acceptance Criteria
| # | Criterion | Status |
|---|-----------|--------|
| 1 | <criterion> | PASS/FAIL/NEEDS REVIEW |

### Gaps Found
1. **[CRITICAL]** <must fix>
2. **[WARNING]** <should fix>

### Recommended Actions
1. <what to do to reach full PASS>
```

## Step 8: Determine Overall Status

- **PASS**: All automated checks pass, all postconditions covered, all extensions implemented and tested
- **PARTIAL**: Some checks pass but gaps exist (list them)
- **FAIL**: Verification command fails, or critical postconditions are uncovered

Report the status clearly. If PARTIAL or FAIL, list exactly what needs to be done to reach PASS.
