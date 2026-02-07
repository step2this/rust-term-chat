# Agent Team Plan: UC-001 Send Direct Message

Generated on 2026-02-07.

## Design Rationale

UC-001 has 18 tasks with a clear funnel shape: one prerequisite, a wide parallel band (Group A: 4 tasks), another parallel band (Group B: 5 tasks), then a sequential pipeline (send → ack → history → extensions → integration test). This shape favors **2 Builders working in parallel** during Groups A/B, converging to a single Builder for the sequential pipeline, with 1 Reviewer providing quality gates.

A 3rd Builder was considered but rejected: the sequential tail (tasks 08-13) is the majority of the work and cannot be parallelized. A 3rd agent would sit idle for most of the session.

**Model selection**: Use **Sonnet** for all teammates. The tasks are well-specified with clear acceptance criteria — Sonnet handles this well and saves significant tokens vs Opus. Use Opus only if a task fails review twice (escalation path).

## Team Composition

| Role | Agent Name | Model | Responsibilities |
|------|-----------|-------|-----------------|
| Lead | `lead` | Sonnet | Manage task list, route work, resolve merge conflicts, run review gates |
| Builder 1 | `builder-proto` | Sonnet | Proto crate: wire types, validation, serialization, tests. Then: send pipeline + sequential chain |
| Builder 2 | `builder-infra` | Sonnet | Transport trait, loopback transport, crypto stub, crypto tests. Then: extensions (fallback, retry, history failure) |
| Reviewer | `reviewer` | Sonnet | Run quality gates, write integration test (T-001-17), write property test (T-001-18), final verification |

## Task Assignment

| Task | Owner | Phase | Parallel Group | Review Gate |
|------|-------|-------|----------------|-------------|
| T-001-01: Init Workspace | `lead` | 1 | — | — |
| T-001-02: Wire Format Types | `builder-proto` | 2 | — | Gate 1 |
| T-001-03: Message Validation | `builder-proto` | 3 | A | — |
| T-001-04: Message Serialization | `builder-proto` | 3 | A | — |
| T-001-14: Test — Validation | `builder-proto` | 3 | A (with 03) | — |
| T-001-15: Test — Serialization | `builder-proto` | 3 | A (with 04) | — |
| T-001-05: Crypto Stub | `builder-infra` | 3 | A | — |
| T-001-06: Transport Trait | `builder-infra` | 3 | A | — |
| T-001-07: Loopback Transport | `builder-infra` | 4 | B | — |
| T-001-16: Test — No Noise Session | `builder-infra` | 4 | B | — |
| T-001-18: Property Test | `reviewer` | 4 | B | — |
| — | — | — | — | **Gate 2: Foundation Check** |
| T-001-08: Send Pipeline | `builder-proto` | 5 | — | Gate 3 |
| T-001-09: Delivery Ack + Status | `builder-proto` | 6 | — | — |
| T-001-10: Local History Storage | `builder-proto` | 7 | — | — |
| T-001-11: Transport Fallback | `builder-infra` | 8 | C | — |
| T-001-13: History Write Failure | `builder-infra` | 8 | C | — |
| T-001-12: Retry + Timeout | `builder-proto` | 9 | — | — |
| T-001-17: Integration Test | `reviewer` | 10 | — | **Gate 4: Final** |

## Execution Phases

### Phase 1: Workspace Bootstrap (Lead only)
- **Tasks**: T-001-01
- **Who**: Lead creates the Cargo workspace directly (small, fast task — no need to delegate)
- **Gate**: `cargo build` succeeds across all 3 crates
- **Output**: Workspace compiles with stub files

### Phase 2: Foundation Types (Builder-Proto, sequential)
- **Tasks**: T-001-02
- **Who**: `builder-proto` defines all wire format types
- **Gate 1**: Reviewer checks type design against UC-001 MSS steps and future UC needs
- **Commands**: `cargo build -p termchat-proto`
- **Why sequential**: All other tasks depend on these types. Getting them right avoids cascading rework.

### Phase 3: Parallel Build — Group A (Both Builders)
- **Tasks**: T-001-03, T-001-04, T-001-14, T-001-15 (`builder-proto`) | T-001-05, T-001-06 (`builder-infra`)
- **Parallelism**: Builders work on completely separate crates/modules — no file conflicts
  - `builder-proto`: validation + serialization + their tests (all in `termchat-proto/`)
  - `builder-infra`: crypto stub + transport trait (in `termchat/src/crypto/` and `termchat/src/transport/`)
- **Commands**: `cargo build`, `cargo test -p termchat-proto`

### Phase 4: Parallel Build — Group B (Both Builders + Reviewer)
- **Tasks**: T-001-07, T-001-16 (`builder-infra`) | T-001-18 (`reviewer`)
- **Parallelism**: Loopback transport, crypto tests, and property tests are independent
- **Gate 2 (Foundation Check)**: After Group B, Reviewer runs full build + test suite
- **Commands**: `cargo build && cargo test --lib && cargo clippy -- -D warnings`

### Phase 5: Send Pipeline (Builder-Proto, sequential — critical path)
- **Tasks**: T-001-08
- **Who**: `builder-proto` — this is the integration point for all foundation work
- **Gate 3 (Pipeline Check)**: Reviewer verifies:
  - Pipeline chains validate → serialize → encrypt → transmit correctly
  - Error types aggregate properly
  - Unit test with loopback passes
- **Commands**: `cargo test -p termchat --lib`

### Phase 6-7: Ack + History (Builder-Proto, sequential)
- **Tasks**: T-001-09, T-001-10
- **Who**: `builder-proto` continues the sequential chain
- **No gate between these**: They're tightly coupled, reviewed together

### Phase 8: Extensions — Group C (Both Builders, parallel)
- **Tasks**: T-001-11, T-001-13 (`builder-infra`) | T-001-12 follows after T-001-11
- **Parallelism**: Fallback/queuing and history write failure are in different modules
- `builder-infra` handles T-001-11 (transport fallback) and T-001-13 (history failure)
- `builder-proto` handles T-001-12 (retry/timeout) after T-001-11 is complete

### Phase 9: Retry Logic (Builder-Proto)
- **Tasks**: T-001-12
- **Depends on**: T-001-11 being complete

### Phase 10: Integration Test + Final Gate (Reviewer)
- **Tasks**: T-001-17
- **Who**: `reviewer` writes the integration test — fresh eyes, tests against postconditions not implementation
- **Gate 4 (Final Verification)**:
  - `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
  - `cargo test --test send_receive` (the UC-001 verification command)
  - Check all 4 success postconditions
  - Check both invariants
  - Check all extension paths have handling

## Review Gates

### Gate 1: Type Design Review
- **After**: T-001-02 (Wire Format Types)
- **Reviewer checks**:
  - Types cover all MSS steps (message, metadata, ack, envelope)
  - Serde derives are correct
  - Types are extensible for future use cases (UC-002 receive, UC-005 handshake)
  - Doc comments on all public items
- **Commands**: `cargo build -p termchat-proto && cargo doc -p termchat-proto --no-deps`
- **Pass criteria**: Builds, docs generate, types map to all MSS data flows
- **On failure**: `builder-proto` revises types based on reviewer feedback

### Gate 2: Foundation Check
- **After**: All of Phase 3 + Phase 4 (Groups A and B complete)
- **Reviewer checks**:
  - All unit tests pass
  - Crypto stub encrypts/decrypts round-trip
  - Loopback transport sends/receives bytes
  - Validation catches empty + oversized messages
  - Serialization round-trips all message types
  - No clippy warnings
- **Commands**: `cargo build && cargo test && cargo clippy -- -D warnings`
- **Pass criteria**: Full green build, all foundations solid
- **On failure**: Assign fix tasks to the responsible builder

### Gate 3: Pipeline Integration
- **After**: T-001-08 (Send Pipeline)
- **Reviewer checks**:
  - `send_message()` chains all steps correctly
  - `SendError` properly wraps all component errors
  - A manual test: construct ChatManager with loopback + stub crypto, send a message, verify encrypted bytes appear on the other end
- **Commands**: `cargo test -p termchat --lib`
- **Pass criteria**: Pipeline unit test passes, error handling is complete
- **On failure**: `builder-proto` fixes pipeline, re-review

### Gate 4: Final UC-001 Verification
- **After**: T-001-17 (Integration Test) + all tasks complete
- **Reviewer checks**:
  - All success postconditions: (1) local history, (2) delivery, (3) ack status, (4) encryption
  - All failure postconditions: error message, pending save, retry behavior
  - Both invariants: no plaintext on wire, message ordering
  - Extension coverage: all 8 extension paths exercised
  - Code quality: `cargo fmt --check && cargo clippy -- -D warnings`
- **Commands**: `cargo fmt --check && cargo clippy -- -D warnings && cargo test && cargo test --test send_receive`
- **Pass criteria**: All commands exit 0, all postconditions verified
- **On failure**: Specific rework tasks created and assigned

## Parallelization Opportunities

```
Timeline (phases →)

Phase:    1     2       3           4         5      6-7     8        9     10
         ┌───┐ ┌─────┐ ┌─────────┐ ┌───────┐ ┌────┐ ┌─────┐ ┌──────┐ ┌───┐ ┌─────┐
lead:    │ 01│ │coord│ │ coord   │ │Gate 2 │ │Gate│ │coord│ │coord │ │   │ │     │
         └───┘ └─────┘ └─────────┘ └───────┘ │ 3  │ └─────┘ └──────┘ │   │ │     │
                ┌─────┐ ┌─────────┐           └────┘ ┌─────┐          │   │ │     │
b-proto:        │ 02  │ │03,04,14,│                  │ 08  │→│09│→│10│→│12│ │     │
                └─────┘ │  15     │                  └─────┘          └───┘ │     │
                        └─────────┘ ┌───────┐                ┌──────┐       │     │
b-infra:                │05,06    │ │07, 16 │                │11, 13│       │     │
                        └─────────┘ └───────┘                └──────┘       │     │
                                    ┌───────┐                               ┌─────┐
reviewer:                           │ 18    │                               │ 17  │
                                    └───────┘                               └─────┘
```

### Conflict-Free Zones
- **Phase 3**: `builder-proto` works only in `termchat-proto/`. `builder-infra` works only in `termchat/src/crypto/` and `termchat/src/transport/`. Zero file overlap.
- **Phase 8**: `builder-infra` works in `transport/hybrid.rs` and `chat/history.rs`. `builder-proto` idle until T-001-11 complete.

### Potential Conflicts
- **`termchat/src/chat/mod.rs`**: Both builders may need to modify this file in phases 5-9. Mitigation: `builder-proto` owns `chat/mod.rs`; `builder-infra` creates separate files (`transport/hybrid.rs`, adds to `chat/history.rs`) and `builder-proto` integrates.
- **`Cargo.toml` dependencies**: Adding new deps (e.g., `proptest`). Mitigation: Lead manages workspace `Cargo.toml` updates.

## Risk Mitigation

| Risk | Task(s) | Mitigation |
|------|---------|------------|
| Transport trait too narrow for UC-003/004 | T-001-06 | Reviewer checks trait against P2P (quinn) and WebSocket (tungstenite) API patterns at Gate 2 |
| Crypto stub interface doesn't fit real Noise | T-001-05 | Review `noise-protocol` crate API before finalizing trait. Mark as `// TODO: UC-005` |
| Async trait ergonomics (edition 2024) | T-001-06 | Try native async traits first; fall back to `async-trait` crate if unstable |
| Send pipeline integration failures | T-001-08 | Gate 3 catches this early; all components tested individually first |
| Bincode version/config mismatch | T-001-04 | Pin bincode version in `Cargo.toml`; use explicit config, not defaults |

## Spawn Commands

```
# 1. Lead creates the team
TeamCreate: team_name="uc-001-impl", description="UC-001 Send Direct Message implementation"

# 2. Lead creates all 18 tasks via TaskCreate (with dependencies set via TaskUpdate)

# 3. Spawn teammates (all use Sonnet model for cost efficiency)
Task tool: name="builder-proto", team_name="uc-001-impl", subagent_type="general-purpose", model="sonnet"
  Prompt: "You are Builder-Proto for UC-001 implementation. You own the termchat-proto crate
  and the chat module. Read docs/tasks/uc-001-tasks.md and docs/use-cases/uc-001-send-direct-message.md.
  Check TaskList for your assigned tasks. Follow TDD: write tests alongside implementation.
  Use thiserror for all error types. No unwrap() in production code. Doc comments on all public items.
  After completing each task, mark it done via TaskUpdate and check TaskList for next work."

Task tool: name="builder-infra", team_name="uc-001-impl", subagent_type="general-purpose", model="sonnet"
  Prompt: "You are Builder-Infra for UC-001 implementation. You own the crypto and transport
  modules in the termchat crate. Read docs/tasks/uc-001-tasks.md and docs/use-cases/uc-001-send-direct-message.md.
  Check TaskList for your assigned tasks. The crypto implementation is a STUB — design the trait
  interface carefully for UC-005 compatibility. Transport trait must be async and generic enough
  for future P2P (quinn) and WebSocket (tungstenite) implementations.
  After completing each task, mark it done via TaskUpdate and check TaskList for next work."

Task tool: name="reviewer", team_name="uc-001-impl", subagent_type="general-purpose", model="sonnet"
  Prompt: "You are the Reviewer for UC-001 implementation. You run quality gates, write the
  integration test (T-001-17) and property test (T-001-18). Read docs/tasks/uc-001-tasks.md
  and docs/use-cases/uc-001-send-direct-message.md. At each review gate, run the specified
  commands and verify against UC-001 postconditions. You write T-001-17 LAST as a blind test
  against the postconditions. Report gate pass/fail to the lead.
  After completing each task, mark it done via TaskUpdate and check TaskList for next work."

# 4. Lead assigns initial tasks and kicks off Phase 1
```

## Estimated Timeline

| Phase | Tasks | Parallelism | Est. Agent Turns |
|-------|-------|-------------|-----------------|
| 1: Bootstrap | 1 | None (lead) | 2-3 |
| 2: Wire Types | 1 | None | 3-4 |
| 3: Group A | 6 | 2 builders parallel | 4-6 (each) |
| 4: Group B + Gate 2 | 3 + gate | 3-way parallel | 3-4 (each) |
| 5: Pipeline + Gate 3 | 1 + gate | None | 4-5 |
| 6-7: Ack + History | 2 | None | 3-4 (each) |
| 8: Extensions | 3 | 2 builders parallel | 3-4 (each) |
| 9: Retry | 1 | None | 2-3 |
| 10: Integration + Gate 4 | 1 + gate | None | 4-5 |
| **Total** | **18** | | **~35-45 turns total** |

## Coordination Notes

- **Shared file protocol**: Only one builder modifies a given file at a time. Lead resolves conflicts if they arise.
- **Module boundaries**: `builder-proto` owns `termchat-proto/` and `termchat/src/chat/`. `builder-infra` owns `termchat/src/crypto/` and `termchat/src/transport/`. Lead owns root `Cargo.toml` files.
- **Communication**: Builders message the lead when tasks complete. Reviewer messages the lead with gate results. Lead messages builders with gate feedback.
- **Escalation**: If a task fails review twice, lead escalates by re-running with Opus model or taking over directly.
- **Commit strategy**: Each completed phase gets a commit. Lead manages commits after gate approval.
