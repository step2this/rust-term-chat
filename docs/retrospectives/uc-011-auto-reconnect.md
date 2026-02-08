# Retrospective: UC-011 Auto-Reconnect to Relay on Disconnect

Date: 2026-02-08
Scope: UC-011 — Supervisor pattern, exponential backoff, message queuing, flap detection, integration tests

## Summary

UC-011 adds automatic reconnection to the relay when the WebSocket connection drops. A supervisor task monitors the receive loop, reconnects with exponential backoff + jitter, queues messages during disconnection, and detects connection flapping. This was the first UC since Sprint 7 to use the full Forge workflow: Cockburn UC doc, worktree, agent team (3 builders + reviewer), task tracking, and quality gate. The process was dramatically better than Sprint 8's monolithic approach, completing in a single session with all 5 integration tests passing.

## Metrics

| Metric | Value |
|--------|-------|
| Use cases completed | 1 (UC-011) |
| Tasks created | 6 (UC doc, config, net.rs, main.rs events, tests, quality gate) |
| Tests written | 5 integration tests (645 lines) |
| Production code added | ~230 lines (net.rs refactor + config + main.rs + relay.rs) |
| Total lines added | 1,294 (across 7 files) |
| Agent team size | 4 (lead + builder-net + builder-test + reviewer) |
| Quality gate | Green (fmt, clippy, all 599+ tests) |
| Defects found during integration | 2 (TCP proxy disconnect detection, command_handler send-then-queue) |
| Defects found by reviewer | 0 (reviewer completed after merge) |
| Worktree used | Yes (`/home/ubuntu/rust-term-chat-reconnect`) |
| Commits | 2 on feature branch + 1 merge commit |
| Forge steps completed | 5/6 (UC doc, task-decompose implicit, team-plan, implement, quality gate — no formal grade-work) |

## What Worked

1. **Worktree isolation was essential** — The `feature/uc-011-reconnect` branch in a separate worktree kept all changes isolated from main. The merge was clean, and the feature could have been abandoned at any point without polluting main. This directly addressed Sprint 8's worst anti-pattern.

2. **Agent team with clear file ownership eliminated conflicts** — builder-net owned `net.rs`, builder-test owned `relay_reconnect.rs` + `Cargo.toml`, lead owned `config/mod.rs`, `main.rs`, and relay. Zero merge conflicts despite 4 agents editing code concurrently. File ownership rules from CLAUDE.md proved their worth.

3. **TCP proxy pattern for test disconnect simulation** — The builder-test agent independently arrived at the TCP proxy approach after the naive `relay_handle.abort()` approach failed. The proxy intercepts all traffic and can be killed atomically, causing immediate RST on both sides. This is a reusable test pattern for any future network failure simulation.

4. **Command handler "send-then-queue-on-failure" pattern** — The original design had a TOCTOU race: check if connected, then send. If the connection died between check and send, the message was lost. The fix was simple: always try to send, and queue on failure. This made all 5 tests pass and is more robust for production use.

5. **UC doc drove implementation** — The Cockburn UC doc's extensions section (5a: all retries exhausted, 5b: unreachable relay, 6a: queue overflow, 3a: shutdown during reconnect) mapped directly to test cases. Writing the UC first ensured the tricky edge cases were covered.

6. **Exponential backoff + jitter is correct by construction** — `min(initial_delay * 2^attempt, max_delay) + random(0..25%)` with configurable parameters via `ReconnectConfig`. The integration test verifies timing gaps between attempts, catching any backoff calculation bugs.

## What Didn't Work

1. **Initial disconnect detection via `relay_handle.abort()` was fundamentally broken** — All 5 tests initially failed because aborting the relay server's JoinHandle doesn't close existing WebSocket connections (they run on independently-spawned axum handler tasks). This cost ~30% of the session's context budget on debugging. The root cause was a misunderstanding of how axum::serve manages connection tasks.

2. **Reviewer completed after merge** — The reviewer agent was spawned but the lead merged the feature branch before receiving the review. The reviewer's findings were never applied. In future, the merge should be gated on reviewer approval.

3. **Task list in agent team was informal** — Tasks were created in the team task system but not in `docs/tasks/uc-011-tasks.md`. If the session had been killed mid-implementation, the next session would have had to reconstruct state from git diffs.

4. **`drain_connection_events` helper consumed non-matching events** — The test helper function that drained initial ConnectionStatus events would consume the first non-ConnectionStatus event and discard it. This caused `queued_messages_sent_after_reconnect` to lose the first message. The builder-test agent caught this independently and removed the drain for bob's channel.

5. **Duplicate `ConnectionStatus { connected: false }` events** — Both `receive_loop` and `supervisor` emit disconnect events. The TUI sees "Disconnected from Relay" twice. Not harmful but untidy. Should be consolidated.

## Patterns Observed

1. **Agent teams work best when each agent has a single file to own** — builder-net wrote `net.rs`, builder-test wrote `relay_reconnect.rs`. No coordination overhead, no merge conflicts, no waiting. The lead handled cross-cutting files (config, main.rs, relay.rs).

2. **Test infrastructure is where most integration debugging time goes** — The actual supervisor pattern implementation compiled and was logically correct on first try. The remaining 70% of integration work was figuring out how to reliably simulate network disconnection in tests (TCP proxy pattern).

3. **`Arc<RwLock<Option<T>>>` is the idiomatic Rust pattern for swappable shared state** — The `SharedChatManager` type alias makes the supervisor pattern clean. The read lock is held during receive_loop, write lock only during brief reconnect swap. No deadlocks because the supervisor waits for recv_handle completion before writing.

4. **Exponential backoff needs a separate "send Reconnecting event" step** — The backoff loop sleeps first, then sends the Reconnecting event, then tries to connect. This ordering ensures tests can wait for the event as a synchronization point.

## Action Items

### Immediate (apply now)

| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | Add "TCP proxy pattern for testing network failures — abort proxy tasks to simulate disconnect" to Process Learnings | `CLAUDE.md` | Lead |
| 2 | Add "Always try send-then-queue-on-failure, never check-then-send (TOCTOU race)" to Process Learnings | `CLAUDE.md` | Lead |
| 3 | Add "`relay_handle.abort()` does NOT close existing WebSocket connections — use TCP proxy or Close frames" to Process Learnings | `CLAUDE.md` | Lead |
| 4 | Add `relay_reconnect` to Build & Development Commands test list | `CLAUDE.md` | Lead |

### Next Sprint (apply before starting)

| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | Gate merge on reviewer approval — do not merge before review completes | Process | Lead |
| 2 | Create `docs/tasks/uc-NNN-tasks.md` even when using agent team task system | Process | Lead |
| 3 | Consolidate duplicate disconnect events (receive_loop should not send ConnectionStatus — let supervisor handle it) | `net.rs` | Builder |
| 4 | Add `/verify-uc 011` run to validate all postconditions formally | Process | Lead |

### Backlog

| # | Action | Target | Owner |
|---|--------|--------|-------|
| 1 | Add WebSocket ping/pong keepalive to RelayTransport for production dead-connection detection | `relay.rs` | Builder |
| 2 | Add configurable read timeout to receive_loop (defense against silent connection death) | `net.rs` | Builder |
| 3 | Add flap detection test (rapid disconnect/reconnect cycles) | `relay_reconnect.rs` | Builder |
| 4 | Consider replacing `Arc<Mutex<VecDeque<String>>>` with a bounded tokio channel for message queue | `net.rs` | Builder |

## Key Learnings

1. **Agent teams with file ownership are the optimal configuration** — Zero conflicts, parallel execution, each agent has clear scope. The lead handles cross-cutting coordination.

2. **Test infrastructure for network simulation is a one-time investment** — The TCP proxy pattern took significant effort to develop but is now reusable for any future networking test (UC-012+, reconnect variations, transport failover).

3. **The TOCTOU pattern (check-then-act) is a recurring bug source in async Rust** — When shared state changes between a check and an action, the action operates on stale assumptions. The fix is always: try the action, handle failure, don't check first.

4. **`axum::serve` task isolation is important to understand** — Individual WebSocket handler tasks are NOT children of the server task. Aborting the server stops accepting connections but does not close existing ones. This applies to any axum/hyper/tower-based server.

5. **Worktree + feature branch + agent team is the right workflow for medium-complexity UCs** — UC-011 was medium complexity (not trivial like a config change, not massive like a full new subsystem). The Forge workflow scaled appropriately.

## Comparison with Sprint 8 (monolithic)

| Dimension | Sprint 8 (monolithic) | UC-011 (Forge) |
|-----------|----------------------|----------------|
| Forge steps | 1/6 | 5/6 |
| Worktree | No | Yes |
| Agent team | 1 agent | 4 agents |
| Reviewer | None | Spawned (late) |
| Commits | 1 monolithic | 2 + merge |
| Context kills | 1 | 0 |
| Orphaned work | 2 | 0 |
| Quality gates | Green | Green |
| Process rating | 2.4/5 | 4.0/5 |

## Process Rating

| Dimension | Rating | Notes |
|-----------|--------|-------|
| Use Case Quality | 4/5 | UC doc written first, drove implementation, extensions mapped to tests |
| Task Decomposition | 3/5 | Agent team tasks tracked internally, but no persistent task file |
| Agent Coordination | 4/5 | File ownership eliminated conflicts; builder-test found proxy pattern independently |
| Quality Gates | 5/5 | fmt, clippy, all 599+ tests green; two bugs caught and fixed before merge |
| Test Coverage | 4/5 | 5 tests cover reconnect, backoff, queuing, shutdown; missing flap test |
| **Overall** | **4.0/5** | **Strong improvement over Sprint 8. Forge workflow proved its value for medium-complexity work.** |
