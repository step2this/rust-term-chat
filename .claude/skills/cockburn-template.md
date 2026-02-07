---
description: Cockburn-for-Agents use case template — the canonical structure for all TermChat use cases
---

# Cockburn-for-Agents Use Case Template

When creating or reviewing a use case, follow this exact structure. Every section is required unless marked optional.

## Template

```markdown
# Use Case: UC-<NNN> <Active Verb Phrase Goal>

## Classification
- **Goal Level**: ☁️ Summary | 🌊 User Goal | 🐟 Subfunction
- **Scope**: System (black box) | Component (white box)
- **Priority**: P0 Critical | P1 High | P2 Medium | P3 Low
- **Complexity**: 🟢 Low | 🟡 Medium | 🔴 High | ⚫ Spike needed

## Actors
- **Primary Actor**: <who initiates>
- **Supporting Actors**: <systems, services, other users involved>
- **Stakeholders & Interests**:
  - <Stakeholder>: <what they care about>

## Conditions
- **Preconditions** (must be true before starting):
  1. <condition — becomes a setup assertion>
- **Success Postconditions** (true when done right):
  1. <condition — becomes a verification assertion>
- **Failure Postconditions** (true when it fails gracefully):
  1. <condition — becomes a failure-mode test>
- **Invariants** (must remain true throughout):
  1. <condition — becomes a continuous assertion>

## Main Success Scenario
1. <Actor> <does something>
2. System <responds/validates/transforms>
3. ...
n. <Success postcondition is achieved>

## Extensions (What Can Go Wrong)
- **2a. <condition at step 2>**:
  1. System <handles it>
  2. <returns to step X | use case fails>
- **3a. <condition at step 3>**:
  1. ...

## Variations
- **1a.** <Actor> may <alternative approach> → <different path>

## Agent Execution Notes
- **Verification Command**: `<shell command to verify postconditions>`
- **Test File**: `<path to test that validates this use case>`
- **Depends On**: UC-<n>, UC-<m>
- **Blocks**: UC-<x>, UC-<y>
- **Estimated Complexity**: <T-shirt size> / <token budget hint>
- **Agent Assignment**: Lead | Teammate:<role> | Subagent

## Acceptance Criteria (for grading)
- [ ] All success postconditions verified by automated test
- [ ] All extension paths have explicit handling
- [ ] No invariant violations detected
- [ ] Code passes lint + clippy (for Rust)
- [ ] Reviewer agent approves
```

## Field Reference

### Goal Level
| Level | Icon | Meaning | Maps To |
|-------|------|---------|---------|
| Summary | ☁️ | High-level business goal | Agent Team Lead task |
| User Goal | 🌊 | What a user sits down to do | Teammate task |
| Subfunction | 🐟 | A step within a larger goal | Subagent task |

### Priority
| Priority | Meaning |
|----------|---------|
| P0 Critical | Must have — system doesn't work without it |
| P1 High | Should have — core experience depends on it |
| P2 Medium | Nice to have — improves experience |
| P3 Low | Stretch goal — do if time permits |

### Complexity
| Level | Icon | Meaning | Typical Effort |
|-------|------|---------|---------------|
| Low | 🟢 | Well-understood, straightforward | S-M tasks |
| Medium | 🟡 | Some unknowns, moderate scope | M-L tasks |
| High | 🔴 | Significant unknowns or scope | L-XL tasks |
| Spike needed | ⚫ | Cannot estimate without research | Spike first, then re-estimate |

### Extension Naming Convention
- Extensions reference the MSS step they branch from: `2a`, `2b`, `5a`
- Letter suffixes for multiple extensions at the same step
- Each extension must resolve with "returns to step X" or "use case fails"

### Common TermChat Actors
- **Terminal User** (Sender, Recipient, Room Creator, Admin)
- **Transport Layer** (P2P Engine, Relay Client)
- **Crypto Layer** (Noise Protocol, Key Store)
- **Relay Server** (Store & Forward, NAT Traversal)
- **Agent Bridge** (Claude Code ↔ TermChat)
- **Task Manager** (CRDT Sync, Task Store)
- **History Store** (SQLite, Local Cache)

### Common TermChat Invariants
- Plaintext message never leaves the application boundary
- Message ordering is preserved per-conversation
- No data loss on transport failure
- UI remains responsive during network operations
- Key material is never logged or transmitted in plaintext
- Relay server never sees message plaintext
