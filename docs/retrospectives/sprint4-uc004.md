# Retrospective: Sprint 4 — UC-004 Relay Messages via Server

Date: 2026-02-07
Scope: Sprint 4 (UC-004 Relay Fallback), completing Phase 4: Hybrid Networking
Commit: `8658939`

## Summary

Implemented a WebSocket-based relay server and client as fallback transport when P2P (QUIC) connections fail. This was the most process-disciplined sprint yet: every Forge step was followed in order (`/uc-create` → `/uc-review` → `/task-decompose` → `/agent-team-plan` → execute → verify → commit), resulting in zero quality gate failures, zero agent kills, and zero merge conflicts.

## Metrics

| Metric | Value |
|--------|-------|
| Use cases completed | 1 (UC-004) |
| Tasks decomposed | 16 |
| Tests added | 57 (190 → 247 total) |
| New files | 7 |
| Modified files | 6 |
| Lines added | ~4,022 |
| Agent team size | 4 (Lead + Builder-Relay + Builder-Client + Reviewer) |
| Agent kills | 0 |
| Quality gate failures | 0 |
| Merge conflicts | 0 |
| Manual interventions | 0 |

## What Worked

1. **Full Forge workflow executed perfectly** — Every step was followed: `/uc-create` produced a thorough use case (scored 95%), `/uc-review` found 5 issues (all fixed), `/task-decompose` produced 16 well-scoped tasks, `/agent-team-plan` designed a 4-agent team with 5 phases and 3 gates. No shortcuts taken.

2. **Parallel builder tracks with zero conflicts** — Builder-Relay owned `termchat-relay/src/` (server), Builder-Client owned `termchat/src/transport/relay.rs` + `hybrid.rs` (client). They ran simultaneously with no shared files. This validated the module ownership pattern from the Phase 0 retrospective.

3. **Lead completed all shared dependencies first** — Proto types (`termchat-proto/src/relay.rs`), Cargo.toml edits, and stub files were all completed in Phase 1 before spawning builders. This eliminated the root cause of the Phase 0 Cargo.toml conflict.

4. **Reviewer as blind tester produced thorough coverage** — The reviewer wrote 30 tests (13 relay client unit, 5 hybrid mux, 12 integration) without seeing builder implementation details. Tests covered store-and-forward, PeerId spoofing enforcement, queue eviction at 1000 messages, 3-peer concurrent routing, and HybridTransport fallback — scenarios the builders might not have self-tested.

5. **`start_server()` as library function enabled in-process testing** — Builder-Relay designed `relay::start_server(addr)` to return `(SocketAddr, JoinHandle)`, making it usable from both `main.rs` and integration tests. Tests use `start_server("127.0.0.1:0")` with OS-assigned ports for isolation.

6. **RPITIT + `Box::pin()` solution for tokio::select!** — The Transport trait uses return-position `impl Future`. Builder-Client solved the `Unpin` requirement for `tokio::select!` by wrapping both recv() calls in `Box::pin()`. Clean solution, all 13 hybrid tests pass.

7. **All 8 previous retro action items were implemented** — CLAUDE.md updated, use case registry created, sprint tracking added, Cargo.toml owned by lead only, task decomposition always run, reviewer always included, agent tasks scoped to <20 tool calls, file ownership documented.

## What Didn't Work

1. **`lib.rs` not in prerequisite template** — Builder-Relay created the relay server as a binary crate. The Reviewer needed `termchat_relay::relay::start_server` in integration tests, requiring a new `termchat-relay/src/lib.rs`. This was a minor 8-line fix, but it should have been anticipated in Phase 1 (Lead prerequisites). The task decomposition template should include "create lib.rs if crate needs to be importable by tests."

2. **tokio-tungstenite version mismatch** — Initially specified `tokio-tungstenite = "0.26"` in workspace Cargo.toml. axum 0.8 uses tungstenite 0.28 internally, causing dual-version resolution. Fixed by bumping to `"0.28"`. Lesson: check transitive dependency versions before locking workspace deps.

3. **No explicit inter-track coordination gate** — The team plan had Phases 2A and 2B running in parallel, but no explicit checkpoint to verify both tracks compiled together before the Reviewer started. In practice this worked fine (both completed cleanly), but a "Phase 2C: Integration build" gate would catch issues earlier in more complex sprints.

## Patterns Observed

1. **Process discipline scales with practice** — Sprint 1 followed process for UC-001 but collapsed for UC-002. Sprint 4 followed every step. The Forge workflow has become muscle memory. The key enabler: slash commands make process easy to follow (one command per step, clear outputs).

2. **4-agent teams are the sweet spot for single-UC sprints** — Lead + 2 Builders + Reviewer. The Lead handles prerequisites and coordination (~30% of work). Builders parallelize implementation (~50%). Reviewer validates independently (~20%). Adding more agents would increase coordination overhead without proportional speed gains.

3. **Proto types as shared contract** — Defining `RelayMessage` in `termchat-proto/src/relay.rs` before spawning builders meant both tracks coded against the same wire format. No integration surprises. This pattern should be replicated: always define shared types in the proto crate first.

4. **Cockburn extensions directly drove security features** — Extension 6b (duplicate registration) became duplicate-peer handling in the relay server. Extension 11a (PeerId spoofing) became server-side `from` field enforcement. Without the systematic "what could go wrong?" pass, these would likely have been missed.

5. **Test count as progress indicator** — Tracking exact test counts (190 → 247) provides a concrete measure of coverage growth. Each gate verified specific counts (Gate 1: 15, Gate 2: 8) rather than just "tests pass."

## Comparison with Previous Sprint

| Dimension | Sprint 1 (UC-001) | Sprint 4 (UC-004) | Change |
|-----------|-------------------|-------------------|--------|
| Process steps followed | 6/6 for UC-001, 2/6 for UC-002 | 6/6 | Consistent |
| Agent kills | 3 | 0 | Eliminated |
| Merge conflicts | 1 (Cargo.toml) | 0 | Eliminated |
| Manual interventions | 2 | 0 | Eliminated |
| Task decomposition | Done for UC-001 only | Done | Always |
| Reviewer included | UC-001 only | Yes | Always |
| Docs updated | Stale | Current | Fixed |

## Action Items

### Immediate (apply now)

| # | Action | Target | Status |
|---|--------|--------|--------|
| 1 | Add "create lib.rs for testable crates" to prerequisite checklist | `CLAUDE.md` | Applying |
| 2 | Add "check transitive deps before locking workspace versions" to coding standards | `CLAUDE.md` | Applying |
| 3 | Document proto-types-first pattern | `CLAUDE.md` | Applying |

### Next Sprint (apply before starting)

| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | Add inter-track integration build gate to team plan template | `/agent-team-plan` | Lead |
| 2 | Include `lib.rs` creation as prerequisite task when crate needs test imports | `/task-decompose` | Lead |
| 3 | Research SQLite crate options (rusqlite vs sqlx) before UC-006 task decomposition | Spike | Lead |

### Backlog

| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | Add version compatibility check to dependency addition workflow | Process | Lead |
| 2 | Create team plan template with explicit integration build phase | `docs/templates/` | Lead |
| 3 | Automate test count tracking across sprints | Tooling | Lead |

## Key Learnings

1. **Process discipline is a skill that improves with practice** — The Forge workflow felt rigid in Sprint 1 but natural by Sprint 4. Slash commands reduce friction to near-zero.
2. **Lead prerequisites are the foundation** — Completing shared types, dependencies, and stubs before spawning builders eliminates the entire class of coordination failures seen in earlier sprints.
3. **Blind reviewer testing catches what builders miss** — 30 tests covering edge cases (eviction, spoofing, concurrent routing) that no builder self-tested. The reviewer role has proven its value across every sprint where it was used.
4. **Module ownership + parallel tracks = zero conflicts** — This pattern has now worked across 3 sprints (UC-001, UC-003, UC-004) with 0 merge conflicts total. It should be considered a proven practice.
5. **Cockburn extensions drive security** — PeerId spoofing enforcement and duplicate registration handling were both extension-driven. The template's systematic error analysis produces real security improvements.

## Process Rating

| Dimension | Rating | Notes |
|-----------|--------|-------|
| Use Case Quality | 5/5 | 95% score, thorough review, all fixes applied |
| Task Decomposition | 5/5 | 16 tasks, clear dependencies, right granularity |
| Agent Coordination | 5/5 | Zero kills, zero conflicts, clean parallel tracks |
| Quality Gates | 5/5 | All 3 gates passed first attempt |
| Documentation | 5/5 | CLAUDE.md, registry, sprint tracking all current |
| **Overall** | **5/5** | **Best sprint yet — full process, zero issues** |
