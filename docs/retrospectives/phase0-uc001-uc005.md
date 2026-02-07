# Retrospective: Phase 0 + UC-001/002/005 + Phase 1 TUI

Date: 2026-02-07
Scope: All work from project inception through Phase 1 TUI and UC-005 E2E Handshake

## Summary

Built the Forge meta-tooling (Phase 0), then implemented three use cases (UC-001 Send, UC-002 Receive, UC-005 E2E Handshake) and Phase 1 Hello Ratatui TUI. The project went from zero Rust code to a three-crate workspace with 149 passing tests, a functional TUI, and real Noise XX encryption. Process discipline was high for UC-001 but degraded significantly for subsequent work items.

## Metrics
- Use cases completed: 3 (UC-001, UC-002, UC-005)
- Milestones completed: 2 (Phase 0 Forge Setup, Phase 1 Hello Ratatui)
- Task decompositions done: 1 of 4 (UC-001 only)
- Agent team plans done: 1 of 4 (UC-001 only)
- Tests written: 149
- Lines of code: ~9,000+ across 4 commits
- Agent kills (mid-work): 3 (Phase 1 TUI, UC-005 attempt 1, UC-005 attempt 2)
- Manual interventions required: 2 (fixing partial work from killed agents)

## What Worked

1. **UC-001 full team approach was excellent** — `/task-decompose` produced 18 well-scoped tasks with a clear dependency graph. The 4-agent team (lead, builder-proto, builder-infra, reviewer) with module ownership boundaries produced 5,667 lines with 114 tests and zero merge conflicts. Review gates caught type design issues early.

2. **Module ownership prevents conflicts** — Assigning `builder-proto` to `termchat-proto/` + `chat/` and `builder-infra` to `crypto/` + `transport/` meant zero file conflicts during UC-001. The same principle worked for Phase 1 (ui/) vs UC-005 (crypto/) running in parallel.

3. **Quality gate is reliable** — `cargo fmt --check && cargo clippy -- -D warnings && cargo test` caught every issue. Running it before every commit is essential. All 4 commits passed on first gate attempt.

4. **Cockburn template extensions map directly to tasks** — UC-001's 8 extension conditions became 6 implementation tasks (T-001-11 through T-001-16). This is the template's biggest value: it forces you to think about error paths systematically.

5. **Sonnet handles well-specified tasks efficiently** — All builder agents used Sonnet model. With clear task descriptions and acceptance criteria, Sonnet produced correct code in fewer turns than expected. Cost savings were significant vs. Opus for implementation work.

6. **Stubbed implementations enable incremental progress** — The `StubNoiseSession` (XOR cipher) allowed UC-001 and UC-002 to build and test the full pipeline before real crypto existed. When UC-005 replaced it, the `CryptoSession` trait interface required zero changes.

## What Didn't Work

1. **Process discipline collapsed after UC-001** — UC-002 skipped task decomposition ("it's small enough for one agent"). Phase 1 had no use case document at all. UC-005 was launched without task decomposition. This violated the blueprint's core principle: the Cockburn template catches the 80% of issues that "little niggling things" cause.

2. **Long-running agents get killed** — Three agents were killed mid-work (Phase 1 TUI, UC-005 attempts). Each kill left partial, broken code that required manual intervention to fix. The UC-005 agent had 6 failing tests when killed, and its bug fixes hadn't been applied. This is the single biggest operational risk.

3. **Concurrent agents touching shared files** — The UC-005 agent overwrote `termchat/Cargo.toml`, removing the `ratatui`/`crossterm`/`chrono` dependencies that the Phase 1 agent had added. This required manual re-addition. Shared files need explicit ownership rules.

4. **No documentation updates** — CLAUDE.md still says "Blueprint/planning phase" despite 9,000+ lines of code. No use case registry. No sprint tracking. No retrospectives until now. This means every new agent starts with stale context.

5. **snow crate API confusion** — The UC-005 agent misused `snow::Builder::generate_keypair()` with `local_private_key()` set, expecting it to derive a public key from the provided private key. It doesn't — it generates a new random keypair. This required adding `x25519-dalek` as a direct dependency. Agent knowledge of crate APIs is unreliable for less common crates.

6. **Phase 1 has no use case or task document** — It was treated as an ad-hoc "just build a TUI" task. This meant no acceptance criteria, no extension conditions, no postconditions to verify against. The TUI works but wasn't validated against any specification.

## Patterns Observed

1. **Discipline inversely correlates with perceived simplicity** — UC-001 (complex, 18 tasks) got the full process. UC-002 ("just add receive") and Phase 1 ("just build a TUI") were treated as too simple for process. But both had issues that would have been caught by task decomposition.

2. **Agent timeout is the #1 operational risk** — Three kills in one session. The pattern: agent starts well, writes code, hits a bug during testing, enters debug loop, gets killed before finishing. Mitigation: keep individual agent tasks under ~15-20 tool calls.

3. **Process debt accumulates silently** — Skipping docs, retrospectives, and registry updates means future agents work with stale context. CLAUDE.md directing agents to "initialize with `cargo init`" when the workspace already exists wastes tokens and causes confusion.

4. **The reviewer role is undervalued** — UC-001 had a dedicated reviewer who wrote the integration test and caught issues. UC-002, Phase 1, and UC-005 had no reviewer. The integration test for UC-005 was written by the same agent that wrote the implementation — not a blind test.

## Action Items

### Immediate (apply now)

| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | Update CLAUDE.md project state, add new test commands, add module map | `CLAUDE.md` | Lead |
| 2 | Create use case registry with completion status | `docs/use-cases/README.md` | Lead |
| 3 | Create current sprint tracking | `docs/sprints/current.md` | Lead |
| 4 | Add Cargo.toml to "lead-owned files" convention | `CLAUDE.md` | Lead |

### Next Sprint (apply before starting)

| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | Always run `/task-decompose` — even for "simple" use cases | Process | All |
| 2 | Always include a reviewer agent in the team | Team config | Lead |
| 3 | Keep agent tasks scoped to <20 tool calls to avoid kills | `/agent-team-plan` | Lead |
| 4 | Write Phase 1 use case retroactively for reference | `docs/use-cases/` | Lead |
| 5 | Add `--max-turns` parameter guidance to agent-team-plan | `.claude/commands/agent-team-plan.md` | Lead |
| 6 | Assign Cargo.toml edits to lead only, not builders | Team plan template | Lead |

### Backlog (nice to have)

| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | Add pre-commit hook for quality gate | `.claude/hooks/` | Lead |
| 2 | Create `/verify-all` command to check all completed UCs at once | `.claude/commands/` | Lead |
| 3 | Add crate API research step to task decomposition for unfamiliar crates | `/task-decompose` template | Lead |
| 4 | Create agent recovery guide (how to resume from killed agent state) | `docs/` | Lead |

## Key Learnings

1. **The Cockburn template's biggest value is extensions** — they force systematic error path thinking and directly produce ~40% of implementation tasks. Never skip them.
2. **Agent teams with module ownership work** — zero merge conflicts when boundaries are clear. The investment in team planning pays off immediately.
3. **"It's too small for process" is always wrong** — UC-002 was 545 lines and still benefited from a specification. Process scales down; the template sections just get shorter.
4. **Agent reliability requires small tasks** — 15-20 tool calls per agent is the sweet spot. Larger tasks risk kills and leave broken state.
5. **The reviewer role should be non-negotiable** — blind testing against postconditions (not implementation) catches real bugs. Integration tests written by the implementor test what was built, not what was specified.
6. **Stale documentation is actively harmful** — CLAUDE.md saying "no Rust code exists yet" causes agents to waste tokens on context that's wrong. Keep docs current or they become liabilities.

## Process Rating
- Use Case Quality: 4/5 (UC-001 and UC-005 were thorough; UC-002 good; Phase 1 had none)
- Task Decomposition: 2/5 (only done for UC-001)
- Agent Coordination: 3/5 (UC-001 was excellent; later work was ad-hoc)
- Quality Gates: 5/5 (never shipped failing code; fmt+clippy+test caught everything)
- Documentation: 1/5 (CLAUDE.md stale, no registry, no sprint tracking, no retros until now)
- Overall: 3/5 (strong technical output, weak process discipline after UC-001)
