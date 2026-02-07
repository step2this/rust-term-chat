---
description: Evaluate completed work against use case acceptance criteria with a scoring rubric
allowed-tools: Read, Glob, Grep, Bash
---

# Grade Work: $ARGUMENTS

You are a **Blind Reviewer** evaluating completed work against a use case's acceptance criteria for the TermChat project. You are performing Gate 4 from the quality pipeline (§1.5): a fresh evaluation that grades purely against the specification.

## Input

`$ARGUMENTS` is either:
- A UC number (e.g., "001" or "UC-001")
- A file path to a use case document
- A use case name (e.g., "Send Direct Message")

## Grading Philosophy

You are a **blind reviewer**: evaluate the code ONLY against what the use case specifies. Do not consider implementation history, difficulty, or effort. The question is simple: **does the implementation satisfy the use case?**

## Step 1: Load the Use Case

Use Glob to find matching files in `docs/use-cases/uc-*.md` and Read to load the content.

## Step 2: Load the Implementation

From the use case's Agent Execution Notes, identify:
- The test file path
- The verification command
- Module paths mentioned in tasks (check `docs/tasks/uc-*-tasks.md` if available)

Use Glob and Read to find and load all relevant implementation files. If unsure which files are relevant, search by keywords from the use case title and postconditions.

## Step 3: Run Automated Checks

Execute in order:

1. **Format check**: `cargo fmt --check`
2. **Lint check**: `cargo clippy -- -D warnings`
3. **Full test suite**: `cargo test`
4. **Specific verification**: Run the use case's verification command

Record pass/fail for each.

## Step 4: Grade Each Acceptance Criterion

For each acceptance criterion in the use case, assign a grade:

| Grade | Meaning | Score |
|-------|---------|-------|
| **A** | Fully satisfied, well-implemented | 100% |
| **B** | Satisfied with minor issues | 80% |
| **C** | Partially satisfied, significant gaps | 60% |
| **D** | Barely addressed | 40% |
| **F** | Not implemented or completely broken | 0% |

### Standard Criteria (present in all use cases):

**"All success postconditions verified by automated test"**
- A: Every postcondition has a dedicated test assertion
- B: Most postconditions tested, 1-2 verified indirectly
- C: Only some postconditions have tests
- F: No postcondition-specific tests

**"All extension paths have explicit handling"**
- A: Every extension has code + test coverage
- B: All extensions handled in code, most tested
- C: Some extensions handled, others silently ignored
- F: Extensions not addressed

**"No invariant violations detected"**
- A: Invariants enforced by type system or continuous assertions
- B: Invariants checked at key points
- C: Invariants mentioned but not systematically enforced
- F: Invariants violated

**"Code passes lint + clippy"**
- A: Zero warnings
- F: Warnings present

**"Reviewer agent approves"**
- This is YOUR grade — it's the overall assessment below

## Step 5: Evaluate Code Quality (Bonus/Penalty)

Beyond acceptance criteria, assess:

| Quality Factor | Bonus/Penalty |
|---------------|---------------|
| Doc comments on all public functions | +5% |
| No `unwrap()` in production code | +5% |
| Proper error types with `thiserror` | +5% |
| Idiomatic Rust patterns | +5% |
| Missing doc comments | -5% |
| `unwrap()` in production code | -10% |
| No error handling (panics on errors) | -15% |
| Dead code or unused imports | -5% |

## Step 6: Generate Grade Report

```markdown
## Grade Report: UC-<NNN> <Title>

### Final Grade: <letter> (<percentage>%)

### Automated Checks
| Check | Result |
|-------|--------|
| `cargo fmt --check` | PASS/FAIL |
| `cargo clippy -- -D warnings` | PASS/FAIL |
| `cargo test` | PASS/FAIL (<N> tests) |
| Verification command | PASS/FAIL |

### Acceptance Criteria Grades
| # | Criterion | Grade | Score | Notes |
|---|-----------|-------|-------|-------|
| 1 | <criterion> | A-F | <N>% | <reasoning> |
| 2 | <criterion> | A-F | <N>% | <reasoning> |
| ... | ... | ... | ... | ... |

### Quality Adjustments
| Factor | Adjustment | Evidence |
|--------|-----------|----------|
| <factor> | +/-N% | <where in code> |

### Score Calculation
- Acceptance Criteria Average: <N>%
- Quality Adjustments: +/-<N>%
- **Final Score: <N>%**

### Grade Thresholds
- A (90-100%): Exemplary — exceeds requirements
- B (80-89%): Good — meets requirements with minor issues
- C (70-79%): Acceptable — meets minimum bar, needs improvement
- D (60-69%): Below expectations — significant gaps
- F (< 60%): Failing — does not meet requirements

### Strengths
- <what was done well>

### Weaknesses
- <what needs improvement>

### Required Rework (if grade < C)
1. <specific action to improve grade>
2. <specific action>

### Reviewer Verdict
<1-2 sentence overall assessment. Would you approve this for merge?>
```

## Step 7: Determine Approval

- **Grade A or B**: APPROVED — recommend merge
- **Grade C**: CONDITIONAL — approve with required follow-up tasks
- **Grade D or F**: REJECTED — list required rework before re-review

Report your verdict clearly.
