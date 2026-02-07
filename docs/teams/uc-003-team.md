# Agent Team Plan: UC-003 Establish P2P Connection

Generated on 2026-02-07.

## Design Rationale

UC-003 has 14 tasks concentrated in a single new file (`transport/quic.rs`). The task graph is mostly sequential: TLS config → listener/connect → send/recv → is_connected → tests → error mapping → integration. Unlike UC-001 which had work across 4 modules enabling parallel builders, UC-003's single-file focus means **one builder is optimal** — two builders on `quic.rs` would cause merge conflicts.

The builder handles all implementation tasks (T-003-02 through T-003-09) sequentially. The reviewer handles the comprehensive unit tests (T-003-10), error mapping tests (T-003-13), and the integration test (T-003-14) — blind testing against postconditions.

**Team size: 3** (Lead + 1 Builder + 1 Reviewer). Smaller than UC-001's 4-agent team, matching the scope.

**Model selection**: Sonnet for all teammates. Tasks are well-specified with clear acceptance criteria. T-003-02 (TLS config) is highest risk — if it fails, Lead can escalate to Opus.

**Max turns**: 20 per agent per assignment. Per retrospective, this prevents context kills.

## Team Composition

| Role | Agent Name | Model | Responsibilities |
|------|-----------|-------|-----------------|
| Lead | `lead` | (current session) | Prerequisite tasks (T-003-01, T-003-11), task routing, review gates, commit |
| Builder | `builder-quic` | Sonnet | All implementation in `transport/quic.rs` (T-003-02 through T-003-09) |
| Reviewer | `reviewer` | Sonnet | Comprehensive tests (T-003-10), error tests (T-003-13), integration test (T-003-14) |

## Task Assignment

| Task | Owner | Phase | Review Gate | Est. Turns |
|------|-------|-------|-------------|------------|
| T-003-01: Add quinn + rustls deps | `lead` | 1 | — | 3-4 |
| T-003-11: Wire up quic module | `lead` | 1 | — | 2-3 |
| T-003-02: TLS config | `builder-quic` | 2 | Gate 1 | 10-15 |
| T-003-03: QuicListener | `builder-quic` | 3 | — | 8-12 |
| T-003-04: QuicTransport connect | `builder-quic` | 3 | — | 8-12 |
| T-003-05: send with framing | `builder-quic` | 3 | — | 5-8 |
| T-003-06: recv with framing | `builder-quic` | 3 | — | 5-8 |
| T-003-07: is_connected + drop | `builder-quic` | 3 | — | 3-5 |
| T-003-08: Test — listener errors | `builder-quic` | 3 | — | 5-8 |
| T-003-09: Test — connect errors | `builder-quic` | 3 | — | 5-8 |
| — | — | — | **Gate 2: Transport Contract** | — |
| T-003-10: Unit tests (round-trip) | `reviewer` | 4 | — | 10-15 |
| T-003-12: Error mapping review | `builder-quic` | 5 | — | 5-8 |
| T-003-13: Test — PeerId + errors | `reviewer` | 5 | — | 5-8 |
| T-003-14: Integration test | `reviewer` | 6 | **Gate 3: Final** | 10-15 |

## Execution Phases

### Phase 1: Prerequisites (Lead only)
- **Tasks**: T-003-01, T-003-11
- **Who**: Lead handles directly (Cargo.toml + mod.rs are lead-owned files)
- **Actions**:
  1. Add `quinn`, `rustls`, `rcgen` to `termchat/Cargo.toml`
  2. Create empty `termchat/src/transport/quic.rs` with module doc comment
  3. Add `pub mod quic;` to `transport/mod.rs`
  4. Add `[[test]]` section for `p2p_connection` integration test
  5. Verify `cargo build` succeeds
- **Gate**: `cargo build` passes with new deps
- **Output**: Builder is unblocked to start T-003-02

### Phase 2: TLS Foundation (Builder-Quic)
- **Tasks**: T-003-02
- **Who**: `builder-quic` — this is the highest-risk task
- **Actions**:
  1. Research quinn 0.11 + rustls 0.23 API (Endpoint, ServerConfig, ClientConfig)
  2. Implement `generate_self_signed_cert()` using `rcgen`
  3. Implement `make_server_config()`
  4. Implement `make_client_config()` with custom `ServerCertVerifier` (skip verify)
  5. Write unit tests for all three functions
- **Gate 1 (TLS Config Check)**: Lead verifies:
  - `cargo build` passes
  - Unit tests for cert generation + config creation pass
  - API usage looks correct (not misusing rustls/quinn)
- **Commands**: `cargo test -p termchat -- quic`
- **Pass criteria**: All TLS helper functions work, unit tests green
- **On failure**: Lead reviews quinn API docs and provides guidance; if stuck, escalate to Opus

### Phase 3: Core Transport Implementation (Builder-Quic, sequential)
- **Tasks**: T-003-03, T-003-04, T-003-05, T-003-06, T-003-07, T-003-08, T-003-09
- **Who**: `builder-quic` works through these sequentially
- **Ordering**:
  1. T-003-03 (QuicListener) and T-003-04 (QuicTransport connect) — these can be built in either order, but since they're in the same file by one builder, sequential is fine. Listener first makes testing connect easier.
  2. T-003-05 (send) → T-003-06 (recv) → T-003-07 (is_connected) — must be sequential
  3. T-003-08 (listener error tests) and T-003-09 (connect error tests) — written alongside implementation
- **Note**: Builder should write tests (T-003-08, T-003-09) immediately after each component, not deferred. TDD style.
- **Gate 2 (Transport Contract Check)**: After all Phase 3 tasks, Lead verifies:
  - `cargo build` passes
  - All inline unit tests pass
  - Builder's error-path tests (T-003-08, T-003-09) pass
  - The `Transport` trait is fully implemented (all 4 methods)
- **Commands**: `cargo test -p termchat -- quic && cargo clippy -- -D warnings`
- **Pass criteria**: Full green, no clippy warnings, builder's own tests pass
- **On failure**: Lead identifies failing tests and assigns fix back to builder

### Phase 4: Reviewer Unit Tests (Reviewer)
- **Tasks**: T-003-10
- **Who**: `reviewer` — writes comprehensive tests as blind validation against postconditions
- **Approach**: Reviewer reads only the UC-003 use case (postconditions, invariants) and the `Transport` trait definition. Tests verify the contract, not the implementation.
- **Tests cover**: round-trip, bidirectional, FIFO ordering, large/empty payloads, transport_type, is_connected, connection drop, PeerId mismatch
- **Commands**: `cargo test -p termchat -- quic::tests`
- **Pass criteria**: All postcondition-derived tests pass
- **On failure**: Lead determines if it's a test issue or implementation bug; routes to appropriate agent

### Phase 5: Error Mapping Polish (Builder-Quic + Reviewer, parallel)
- **Tasks**: T-003-12 (`builder-quic`), T-003-13 (`reviewer`)
- **Parallelism**: Builder reviews error mapping in `quic.rs`; Reviewer writes targeted error tests. They work on the same file but in different sections (implementation vs `#[cfg(test)]`). Lead coordinates to avoid conflicts — builder finishes T-003-12 first, then reviewer writes T-003-13.
- **Actually**: Run sequentially to avoid file conflicts. Builder does T-003-12, then Reviewer does T-003-13.
- **Commands**: `cargo test -p termchat -- quic && cargo clippy -- -D warnings`

### Phase 6: Integration Test + Final Gate (Reviewer)
- **Tasks**: T-003-14
- **Who**: `reviewer` — writes the end-to-end integration test
- **Gate 3 (Final UC-003 Verification)**:
  - `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
  - `cargo test --test p2p_connection` (the UC-003 verification command)
  - Check all 7 success postconditions
  - Check all 4 invariants
  - Check all 11 extension paths have handling
- **Commands**: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
- **Pass criteria**: All commands exit 0, all postconditions verified
- **On failure**: Specific rework tasks assigned to builder

## Review Gates

### Gate 1: TLS Config Check
- **After**: T-003-02
- **Reviewer checks**:
  - Self-signed cert generation works
  - Server and client configs create successfully
  - Client config skips certificate verification (custom verifier)
  - No misuse of rustls/quinn APIs (check against crate docs)
- **Commands**: `cargo test -p termchat -- quic`
- **Pass criteria**: Unit tests green, code review passes
- **On failure**: Builder fixes based on feedback; if quinn API confusion, Lead researches and provides guidance

### Gate 2: Transport Contract
- **After**: T-003-03 through T-003-09
- **Reviewer checks**:
  - `QuicTransport` implements all 4 `Transport` trait methods
  - `QuicListener` binds, accepts, and produces `QuicTransport`
  - Builder's unit tests cover happy path + error paths
  - Length-prefix framing matches existing codec conventions (4-byte LE prefix)
  - Error mapping covers all quinn error variants mentioned in extensions
  - `is_connected()` detects connection drops
- **Commands**: `cargo test -p termchat -- quic && cargo clippy -- -D warnings`
- **Pass criteria**: All builder tests pass, clippy clean
- **On failure**: Specific fix tasks assigned to builder

### Gate 3: Final UC-003 Verification
- **After**: T-003-14 (integration test) + all tasks complete
- **Reviewer checks**:
  - All 7 success postconditions verified by test
  - All 4 failure postconditions have code paths
  - All 4 invariants maintained
  - All 11 extension error paths handled
  - Code quality: fmt, clippy, full test suite
- **Commands**: `cargo fmt --check && cargo clippy -- -D warnings && cargo test && cargo test --test p2p_connection`
- **Pass criteria**: All commands exit 0
- **On failure**: Specific rework tasks created and assigned

## Parallelization Opportunities

```
Timeline (phases →)

Phase:    1          2            3                    4          5         6
         ┌────────┐ ┌──────────┐ ┌──────────────────┐ ┌────────┐ ┌──────┐ ┌──────────┐
lead:    │01 + 11 │ │Gate 1    │ │  coordinate      │ │Gate 2  │ │coord │ │Gate 3    │
         └────────┘ └──────────┘ └──────────────────┘ └────────┘ └──────┘ └──────────┘
                    ┌──────────┐ ┌──────────────────┐            ┌──────┐
b-quic:             │ 02 (TLS) │ │03→04→05→06→07    │            │ 12   │
                    └──────────┘ │   + 08, 09       │            └──────┘
                                 └──────────────────┘  ┌────────┐          ┌──────────┐
reviewer:                                              │ 10     │ → │ 13 │ → │ 14     │
                                                       └────────┘          └──────────┘
```

**Minimal parallelism** — UC-003 is inherently sequential because it's one file. The main parallelism is between Phase 4 (reviewer tests) starting while builder is idle (waiting for review), and the builder can start T-003-12 after Gate 2 while reviewer works on T-003-10.

## Risk Mitigation

| Risk | Task(s) | Mitigation |
|------|---------|------------|
| quinn 0.11 + rustls 0.23 API incompatibility | T-003-01, T-003-02 | Lead verifies versions compile in T-003-01 before builder starts. Check quinn's Cargo.toml for its rustls version. |
| `ServerCertVerifier` implementation incorrect | T-003-02 | Research quinn examples and tests for skip-verify patterns. The `rustls::client::danger` module API changes between versions — verify against 0.23 docs. |
| quinn API confusion (Endpoint::server vs Endpoint::new) | T-003-02, T-003-03 | Builder should read quinn 0.11 examples before writing code. Key patterns: `Endpoint::server()` for listener, `Endpoint::client()` for initiator. |
| Timeout tests flaky in CI | T-003-09 | Use `192.0.2.1:1` (TEST-NET-1, RFC 5737) for unreachable address with 1-2s timeout. Never use `localhost` for timeout tests. |
| Connection drop detection unreliable | T-003-07 | Enable QUIC keep-alive (15s). In tests, use `connection.close()` explicitly rather than relying on drop detection timing. |
| Builder agent killed mid-work (>20 turns) | All builder tasks | Tasks T-003-03 through T-003-09 should be given as one batch to the builder. If the builder's context fills, the lead can split remaining work into a second builder invocation. |

## Spawn Commands

```
# 1. Lead completes Phase 1 directly (T-003-01 + T-003-11)
# No team needed for this — lead edits Cargo.toml and mod.rs directly

# 2. Create the team
TeamCreate: team_name="uc-003-impl", description="UC-003 Establish P2P Connection"

# 3. Create tasks in shared task list (14 tasks via TaskCreate)

# 4. Spawn builder
Task tool: name="builder-quic", team_name="uc-003-impl", subagent_type="general-purpose", model="sonnet", max_turns=20
  Prompt: "You are Builder-Quic for UC-003 implementation. You own transport/quic.rs.
  Read docs/tasks/uc-003-tasks.md and docs/use-cases/uc-003-establish-p2p-connection.md.
  Start with T-003-02 (TLS config). Work through tasks sequentially.
  Key guidance:
  - Use quinn 0.11 API: Endpoint::server() for listener, Endpoint::client() for initiator
  - For skip-verify TLS: implement rustls::client::danger::ServerCertVerifier
  - Use rcgen for self-signed cert generation
  - Length-prefix framing: 4-byte LE u32 + payload (matches termchat-proto codec)
  - PeerId validation: same pattern as LoopbackTransport (check remote_id on send)
  - Write TDD: tests alongside each component (T-003-08, T-003-09)
  - All tests use 127.0.0.1:0 for port allocation
  After completing each task, mark it done via TaskUpdate and check TaskList for next work."

# 5. After Gate 2, spawn reviewer
Task tool: name="reviewer", team_name="uc-003-impl", subagent_type="general-purpose", model="sonnet", max_turns=20
  Prompt: "You are the Reviewer for UC-003. You write tests against postconditions, not implementation.
  Read docs/use-cases/uc-003-establish-p2p-connection.md for postconditions and invariants.
  Read termchat/src/transport/mod.rs for the Transport trait contract.
  Your tasks: T-003-10 (comprehensive unit tests), T-003-13 (error mapping tests), T-003-14 (integration test).
  Test against POSTCONDITIONS:
  - Bidirectional QUIC connection works
  - Both peers send/recv opaque bytes
  - Transport trait fully satisfied (send, recv, is_connected, transport_type)
  - transport_type() returns P2p
  - Connection timeout enforced
  - PeerId validation works
  Write tests in transport/quic.rs (#[cfg(test)]) for T-003-10 and T-003-13.
  Write tests/integration/p2p_connection.rs for T-003-14.
  After completing each task, mark it done via TaskUpdate and check TaskList for next work."

# 6. Lead monitors progress and runs review gates
```

## Coordination Notes

- **Single file ownership**: `builder-quic` exclusively owns `transport/quic.rs` during Phases 2-3 and Phase 5. Reviewer writes tests in the same file's `#[cfg(test)]` module during Phase 4 — no overlap because builder is idle during reviewer's phase.
- **Lead-owned files**: `Cargo.toml` and `transport/mod.rs` are modified only by the lead.
- **Integration test file**: `tests/integration/p2p_connection.rs` is created by the reviewer in Phase 6. No conflicts.
- **Communication protocol**: Builder messages lead after each task. Reviewer messages lead with gate results. Lead routes rework.
- **Commit strategy**: One commit after all tasks pass Gate 3. Lead manages the commit.
- **Escalation**: If T-003-02 fails twice, Lead takes over with targeted quinn API research using the Explore agent.
