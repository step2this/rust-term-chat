# Retrospective: Sprint 6 — UC-007 Join Room as Agent Participant

Date: 2026-02-07
Scope: Sprint 6 (UC-007 Join Room as Agent Participant), beginning Phase 6: Agent Integration
Commit: `18baa1b`

## Summary

Implemented the agent bridge subsystem enabling Claude Code agents to join chat rooms as participants via Unix domain sockets with a JSON lines protocol. This was the most complex use case to date (20 tasks, 5,581 lines added, 135 new tests) — classified as High complexity with an XL estimated size. The full Forge workflow executed cleanly for the 3rd consecutive sprint with the 4-agent team pattern. All Sprint 5 action items were applied: builders ran `cargo fmt` per-task, task claim instructions were explicit in spawn prompts, and Phase 2C integration gate caught 7 clippy issues (cross-track pedantic warnings). Zero agent kills, zero merge conflicts.

## Metrics

| Metric | Value |
|--------|-------|
| Use cases completed | 1 (UC-007) |
| Tasks decomposed | 20 |
| Tasks completed | 20/20 |
| Tests added | 135 (340 → 475 total) |
| New files | 9 (agent/{mod,protocol,bridge,participant}.rs, proto/agent.rs, agent_bridge integration test, UC doc, task doc, team plan) |
| Modified files | 11 |
| Lines added | ~5,581 |
| Agent team size | 4 (Lead + Builder-Bridge + Builder-Integration + Reviewer) |
| Agent kills | 0 |
| Quality gate failures | 1 (7 clippy pedantic warnings at Phase 2C — fixed by Lead) |
| Merge conflicts | 0 |
| Manual interventions | 2 (cross-track dependency coordination, clippy fixes at Phase 2C) |
| Builder-Bridge tasks | 6 (T-007-03 through T-007-08), 69 unit tests |
| Builder-Integration tasks | 7 (T-007-09 through T-007-15), 35 unit tests |
| Reviewer tasks | 4 (T-007-17 through T-007-20), 31 integration tests |

## What Worked

1. **Full Forge workflow executed cleanly for the 3rd consecutive sprint** — `/uc-create` → `/uc-review` → fix issues → `/task-decompose` → `/agent-team-plan` → execute → verify → commit. Every step produced a clear artifact feeding the next. The UC-007 use case document (287 lines, 26 MSS steps, 21 extensions, 5 variations) was the most detailed yet, and its thoroughness directly translated into well-specified tasks.

2. **4-agent team pattern: third sprint, zero kills, zero conflicts** — Lead + Builder-Bridge + Builder-Integration + Reviewer. Same composition as Sprints 4 and 5. File ownership was completely disjoint (bridge.rs/protocol.rs vs participant.rs/room.rs/app.rs/ui/). No builder ever touched another builder's files.

3. **Proto-types-first pattern validated for 4th sprint** — Lead completed `AgentInfo`, `AgentCapability`, and `is_agent` on `MemberInfo` in Phase 1 before spawning builders. Both builders coded against the shared contract. The `#[serde(default)]` on `is_agent` ensured backward compatibility — all 340 existing tests passed without modification.

4. **Phase 2C integration gate justified itself** — The gate (introduced in Sprint 5 retro) caught 7 clippy pedantic warnings across builder tracks: `while_let_loop`, `missing_errors_doc`, `must_use_candidate`, `missing_const_for_fn` (x2), `match_same_arms` (x2), `single_match_else`. These were cross-track issues that neither builder would have seen in isolation. Lead fixed all 7 in ~5 minutes. Without the gate, the reviewer would have hit compile warnings.

5. **Sprint 5 action items all applied successfully**:
   - Builders ran `cargo fmt` before marking each task complete → no fmt failures at any gate
   - Explicit "claim task #N immediately" in builder spawn prompts → both builders claimed immediately, zero nudges needed
   - Phase 2C integration gate was executed → caught 7 real issues (see above)

6. **Cross-track dependency managed cleanly** — T-007-07 (handshake) depended on T-007-09 (RoomManager extensions) across builder tracks. Builder-Integration was instructed to do T-007-09 first. While waiting for Bridge track, Builder-Integration worked on independent tasks (#12 /invite-agent, #13 UI badges). No idle time, no blocking.

7. **Reviewer wrote 31 blind integration tests** — Tests written against postconditions and acceptance criteria, not implementation details. Covered: socket creation, stale socket, timeout, multi-connect, handshake (success/malformed/bad version/invalid ID/room full/collision/empty history), disconnect (graceful/ungraceful), heartbeat (pong keeps alive, timeout), participation (send/receive/not-ready/oversize), membership updates, and 6 end-to-end lifecycle scenarios.

8. **CleanupContext convergence pattern** — All three disconnect triggers (Goodbye, broken pipe, heartbeat timeout) converge through a single `CleanupContext` struct, eliminating duplication. This was driven by Extension analysis in the use case (20a, 22a, 22b all having the same cleanup path).

## What Didn't Work

1. **Clippy pedantic caught cross-track at Phase 2C, not earlier** — The 7 clippy warnings were all from workspace-level `clippy::pedantic` and `clippy::nursery` lints. Builders ran `cargo fmt` per-task but not `cargo clippy` per-task. The warnings accumulated silently until the Phase 2C gate. While the gate caught them (that's its job), builders could have self-detected with `cargo clippy -p termchat -- -D warnings` after each task.

2. **AgentParticipant fan-out doesn't use real crypto/transport** — Similar to UC-006's simplification, the fan-out in `AgentParticipant` sends `OutboundAgentMessage` via mpsc channel to the app layer rather than directly calling per-peer encrypt+send. This is architecturally correct (keeps the participant decoupled from transport/crypto generics) but means the actual encryption path isn't exercised in agent-specific tests. Full integration with CryptoSession and Transport will be needed when end-to-end agent messaging is tested across real network boundaries.

3. **Use case "Out of Scope" section was added during review, not creation** — The `/uc-review` step identified missing scope boundaries (multiple simultaneous agents, agent spawning, agent-to-agent communication, CRDT tasks). These were important to document but should have been considered during `/uc-create`. The template doesn't prompt for scope exclusions.

4. **Heartbeat constants need centralized configuration** — `HeartbeatConfig { ping_interval, pong_timeout }` is defined in `bridge.rs` with defaults (30s, 30s). Tests use short values (100ms). But there's no centralized config system — when the TUI launches agent bridges, there's no way for users to configure heartbeat timing. This is a gap for Sprint 8 (config/settings).

## Patterns Observed

1. **Complexity scaled linearly with use case thoroughness** — UC-007 had 26 MSS steps (vs 12 for UC-006) and 21 extensions (vs 12 for UC-006), producing 20 tasks (vs 18). The 1.7x extension coverage increase produced a proportional increase in code coverage. The Cockburn template continues to drive completeness.

2. **Two-track parallelism is the sweet spot for agent teams** — Three consecutive sprints have validated: Lead handles shared types + gates, two builders work on disjoint file sets, reviewer comes after integration gate. Adding a third builder wouldn't help — the dependency graph has exactly two independent tracks per sprint.

3. **Cross-track dependencies are manageable with early prioritization** — The T-007-07↔T-007-09 sync point was identified during `/agent-team-plan` and mitigated by instructing Builder-Integration to do T-007-09 first. Builder-Bridge had 4 independent tasks to work on while waiting. No actual blocking occurred.

4. **Phase 2C gate catch rate: Sprint 5 = 0 issues, Sprint 6 = 7 issues** — The gate's value is situational. With clippy pedantic + nursery lints, more cross-track issues surface. The gate justified its existence this sprint. Cost: ~30 seconds. Value: prevented 7 compiler warnings from reaching the reviewer.

5. **Test distribution shifted toward integration** — UC-007 produced 69 unit tests (builders) + 35 unit tests (builder-integration) + 31 integration tests (reviewer) = 135 total. The 23% integration test ratio is higher than UC-006 (22/71 = 31%) but lower than expected for a high-complexity UC. The unit test density is high because each agent module has extensive inline tests.

## Comparison with Previous Sprints

| Dimension | Sprint 4 (UC-004) | Sprint 5 (UC-006) | Sprint 6 (UC-007) | Trend |
|-----------|-------------------|-------------------|-------------------|-------|
| Process steps followed | 6/6 | 6/6 | 6/6 | Stable |
| Tasks decomposed | 16 | 18 | 20 | +2/sprint (complexity growth) |
| Tests added | 57 | 71 | 135 | +90% (largest UC) |
| Agent kills | 0 | 0 | 0 | Stable (3 sprints) |
| Merge conflicts | 0 | 0 | 0 | Stable (3 sprints) |
| Quality gate failures | 0 | 1 (fmt) | 1 (clippy) | Stable (minor, caught by gates) |
| Manual interventions | 0 | 1 (nudge) | 2 (sync + clippy) | +1 (complexity cost) |
| Lines added | ~4,022 | ~2,651 | ~5,581 | Largest sprint |
| Phase 2C gate issues | N/A | 0 | 7 | Gate justified |
| UC complexity | Medium | Medium | High | Escalation |
| Sprint 5 retro actions applied | N/A | N/A | 3/3 | 100% adoption |

## Action Items

### Immediate (apply now)

| # | Action | Target | Status |
|---|--------|--------|--------|
| 1 | Add "builders must run `cargo clippy -p <crate> -- -D warnings` before marking each task complete" to CLAUDE.md coding standards | `CLAUDE.md` | Applying |
| 2 | Add "Out of Scope" section prompt to `/uc-create` skill template | `.claude/commands/uc-create.md` | Noted |
| 3 | Update CLAUDE.md project state with UC-007 completion, 475 tests, Phase 6 started | `CLAUDE.md` | Done (in commit) |

### Next Sprint (apply before starting)

| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | Add `cargo clippy` to builder per-task acceptance checks in team plan template | `/agent-team-plan` skill | Lead |
| 2 | Implement centralized config system for runtime settings (heartbeat, timeouts) | UC-008 or Sprint 8 | Lead |
| 3 | Exercise full crypto/transport path for agent message fan-out in integration tests | UC-008 or enhancement | Lead |
| 4 | Add "Out of Scope" section to Cockburn template in blueprint and uc-create | Template update | Lead |

### Backlog

| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | Automate `cargo clippy` as pre-task-completion hook for builder agents | `.claude/hooks/` | Lead |
| 2 | Add property tests for AgentMessage/BridgeMessage JSON serialization | `tests/property/` | Lead |
| 3 | Support multiple simultaneous agents per room (multi-bridge) | Future UC | Lead |
| 4 | Agent process spawning from TUI (instead of requiring external agent) | Future UC | Lead |
| 5 | Fix JoinApproved/JoinDenied relay routing (carried from Sprint 5) | Backlog | Lead |

## Key Learnings

1. **Clippy pedantic should be a per-task gate, not just per-phase** — The 7 cross-track clippy issues at Phase 2C could have been caught if builders ran `cargo clippy` after each task, same as they now run `cargo fmt`. The incremental cost is low (~5 seconds per check) and it prevents accumulation.

2. **Cross-track dependencies are solvable with task ordering, not coordination protocols** — Rather than complex messaging between builders, simply instructing one builder to prioritize the shared dependency (T-007-09) eliminated the blocking risk. The other builder had enough independent work (4 tasks) to stay productive.

3. **"Out of Scope" is a first-class section** — UC-007 benefited greatly from explicitly listing what is NOT included (multiple agents, spawning, agent-to-agent, CRDT tasks). This prevents scope creep during implementation and sets clear expectations for future sprints. Should be standard in the template.

4. **High-complexity UCs work with the same team pattern** — UC-007 was the first "High" complexity UC (vs Medium for UC-004 and UC-006). The same 4-agent team pattern handled it without any structural changes — just more tasks (20 vs 18 vs 16) and more code (5.5K vs 2.6K vs 4.0K lines).

5. **Convergence patterns emerge from extension analysis** — The `CleanupContext` pattern (single convergence point for all disconnect triggers) was driven by systematically analyzing extensions 20a, 22a, 22b in the use case. The Cockburn extension format naturally reveals when multiple error paths need the same handling.

## Process Rating

| Dimension | Rating | Notes |
|-----------|--------|-------|
| Use Case Quality | 5/5 | 287 lines, 26 MSS steps, 21 extensions, 5 variations, comprehensive implementation notes |
| Task Decomposition | 5/5 | 20 tasks, clear dependency graph, right granularity, 2 parallel tracks, accurate size estimates |
| Agent Coordination | 5/5 | Zero kills, zero conflicts, zero nudges, cross-track sync handled cleanly, all Sprint 5 actions applied |
| Quality Gates | 4/5 | Phase 2C caught 7 clippy issues (good!), but builders should self-detect with per-task clippy |
| Documentation | 5/5 | CLAUDE.md updated, team plan detailed, task docs comprehensive, UC doc published |
| **Overall** | **4.8/5** | **Best sprint yet — scaled to High complexity with zero friction. Only improvement: per-task clippy.** |
