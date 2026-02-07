---
description: Generate a PRD by synthesizing multiple use cases into a coherent product requirements document
allowed-tools: Read, Glob, Grep, Write
---

# Generate PRD from Use Cases: $ARGUMENTS

You are a **Product Architect** synthesizing Cockburn use cases into a coherent Product Requirements Document (PRD) for the TermChat project.

## Input

`$ARGUMENTS` is optional and can be:
- A list of UC numbers (e.g., "001 002 005") — include only these
- A sprint name (e.g., "Sprint 1") — include use cases tagged for that sprint
- Empty — include ALL use cases in `docs/use-cases/`

## Step 1: Load All Relevant Use Cases

Use Glob to find all `docs/use-cases/uc-*.md` files. Read each one and extract:
- UC number and title
- Goal level (☁️/🌊/🐟)
- Priority (P0-P3)
- Complexity
- Dependencies (Depends On / Blocks)
- Success postconditions
- Actors

If `$ARGUMENTS` specifies a subset, filter to only those use cases.

## Step 2: Build the Dependency Graph

From the Depends On / Blocks fields, construct a dependency graph:
- Identify root use cases (no dependencies)
- Identify leaf use cases (nothing depends on them)
- Identify critical path (longest chain of dependencies)
- Flag any circular dependencies as errors

Render the graph as a text diagram showing the ordering.

## Step 3: Group by Goal Level

Organize use cases into a hierarchy:
- **☁️ Summary Goals** — high-level capabilities (these become PRD "Epics")
- **🌊 User Goals** — what users sit down to do (these become PRD "Features")
- **🐟 Subfunctions** — supporting steps (these become PRD "Requirements" under their parent feature)

If a 🐟 Subfunction doesn't have a parent 🌊 User Goal, flag it.

## Step 4: Extract Cross-Cutting Concerns

Scan all use cases for recurring themes:
- **Invariants** that appear in multiple use cases → become "System-Wide Requirements"
- **Common extensions** (e.g., "network unavailable") → become "Error Handling Requirements"
- **Shared actors** → become "User Roles" or "System Components"
- **Common preconditions** → become "System Prerequisites"

## Step 5: Generate the PRD

Write the PRD to `docs/prd.md` (or the path specified in `$ARGUMENTS`) with this structure:

```markdown
# TermChat Product Requirements Document

Generated from <N> use cases on <date>.

## 1. Overview

### 1.1 Product Vision
<Synthesized from blueprint and use case goals>

### 1.2 Scope
<What's included and excluded based on the use cases analyzed>

### 1.3 User Roles
<Extracted from Primary Actors across all use cases>

## 2. Feature Map

### 2.1 Dependency Graph
<Text diagram of use case dependencies>

### 2.2 Critical Path
<The longest dependency chain — this determines minimum timeline>

## 3. Epics and Features

### Epic: <Summary Goal Title>

#### Feature: <User Goal Title> (UC-NNN)
- **Priority**: <P0-P3>
- **Complexity**: <emoji>
- **Depends On**: <list>
- **Key Postconditions**:
  - <extracted from use case>
- **Key Extensions**:
  - <most impactful error paths>

<Repeat for each use case>

## 4. System-Wide Requirements

### 4.1 Security Invariants
<From crypto/security invariants>

### 4.2 Performance Requirements
<From timing-related postconditions and extensions>

### 4.3 Error Handling
<Common extension patterns>

### 4.4 Prerequisites
<Common preconditions>

## 5. Implementation Order

<Topologically sorted list based on dependency graph>

| Phase | Use Cases | Key Deliverables |
|-------|-----------|-----------------|
| 1     | UC-NNN, ... | <what's built> |
| 2     | UC-NNN, ... | <what's built> |
| ...   | ...       | ...             |

## 6. Risks and Open Questions

<Flagged items: circular deps, untestable postconditions, missing extensions, orphan subfunctions>

## 7. Acceptance Criteria

<Aggregated from all use case acceptance criteria>
```

## Step 6: Report Summary

After writing the PRD, give the user a summary:
- Total use cases analyzed
- Dependency graph overview (roots, leaves, critical path length)
- Coverage gaps (use cases missing extensions, untestable postconditions)
- Suggested next actions (missing use cases that would fill gaps)
