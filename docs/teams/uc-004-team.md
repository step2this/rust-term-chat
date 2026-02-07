# Agent Team Plan: UC-004 Relay Messages via Server

Generated on 2026-02-07.

## Design Rationale

UC-004 has 16 tasks split across **two independent crates**: `termchat-relay/` (server) and `termchat/src/transport/relay.rs` (client). Unlike UC-003 where everything was in one file, the relay server and relay client have **zero file overlap**, making parallel builders highly effective.

The shared dependency is `termchat-proto/src/relay.rs` (RelayMessage types, T-004-02), which must complete before either builder starts. Both tracks also need an in-process relay server for tests, so the server must complete first for testing purposes — but the client implementation can happen in parallel with the server.

**Team size: 4** (Lead + 2 Builders + 1 Reviewer). Larger than UC-003's 3-agent team, justified by genuine parallelism across two crates.

**Model selection**: Sonnet for all teammates. Tasks are well-specified with acceptance criteria. The highest-risk tasks (T-004-03 axum WebSocket, T-004-06 tokio-tungstenite) are both isolated in separate files, so failures don't block each other.

**Max turns**: 25 per builder (XL complexity — more code than UC-003), 20 for reviewer. Per retrospective learning, this prevents context kills.

**Execution strategy**: Lead handles prerequisites (T-004-01, T-004-02, T-004-12), then spawns both builders simultaneously. Reviewer starts after both tracks complete their implementation + unit tests. This maximizes parallelism while keeping test authorship independent from implementation.

## Team Composition

| Role | Agent Name | Model | Responsibilities |
|------|-----------|-------|-----------------|
| Lead | `lead` | (current session) | Prerequisites (T-004-01, T-004-02, T-004-12), task routing, review gates, commit |
| Builder-Relay | `builder-relay` | Sonnet | Relay server: `termchat-relay/src/{main.rs, relay.rs, store.rs}` (T-004-03, T-004-04, T-004-05, T-004-10) |
| Builder-Client | `builder-client` | Sonnet | Relay client: `termchat/src/transport/relay.rs` + hybrid refactor (T-004-06, T-004-07, T-004-08, T-004-09, T-004-13) |
| Reviewer | `reviewer` | Sonnet | All test tasks: T-004-11, T-004-14, T-004-15, T-004-16 |

### File Ownership (strict — no overlap)

| Agent | Owns (exclusive write) |
|-------|----------------------|
| Lead | `Cargo.toml` (all 3), `termchat-proto/src/relay.rs`, `termchat-proto/src/lib.rs`, `termchat/src/transport/mod.rs` |
| Builder-Relay | `termchat-relay/src/main.rs`, `termchat-relay/src/relay.rs`, `termchat-relay/src/store.rs` |
| Builder-Client | `termchat/src/transport/relay.rs`, `termchat/src/transport/hybrid.rs` |
| Reviewer | `tests/integration/relay_fallback.rs`, plus `#[cfg(test)]` sections in relay client (T-004-11) |

**Note on T-004-10 and T-004-11**: The task decomposition assigns server unit tests (T-004-10) to Reviewer, but since these tests go inside `termchat-relay/src/relay.rs` and `store.rs` (Builder-Relay's files), **Builder-Relay should write server unit tests inline** as TDD. Reviewer then writes the _integration_ tests (T-004-15, T-004-16) and the client unit tests (T-004-11) — which go into `relay.rs`'s `#[cfg(test)]` module after Builder-Client is done.

**Revised assignment**: T-004-10 → Builder-Relay (TDD inline tests), T-004-11 → Reviewer (blind validation after client implementation complete).

## Task Assignment

| Task | Owner | Phase | Review Gate | Est. Turns |
|------|-------|-------|-------------|------------|
| T-004-01: Add dependencies | `lead` | 1 | — | 3-4 |
| T-004-02: RelayMessage types | `lead` | 1 | — | 5-8 |
| T-004-12: Wire up relay module | `lead` | 1 | — | 2-3 |
| T-004-03: Relay server core | `builder-relay` | 2A | — | 10-15 |
| T-004-04: Store-and-forward queue | `builder-relay` | 2A | — | 5-8 |
| T-004-05: Message routing + drain | `builder-relay` | 2A | — | 8-12 |
| T-004-10: Relay server unit tests | `builder-relay` | 2A | **Gate 1** | 5-8 |
| T-004-06: RelayTransport connect | `builder-client` | 2B | — | 10-15 |
| T-004-07: RelayTransport send/recv | `builder-client` | 2B | — | 8-12 |
| T-004-08: is_connected + disconnect | `builder-client` | 2B | — | 3-5 |
| T-004-09: Extension error handling | `builder-client` | 2B | **Gate 2** | 3-5 |
| T-004-13: HybridTransport recv mux | `builder-client` | 3 | — | 5-8 |
| T-004-11: RelayTransport unit tests | `reviewer` | 4 | — | 8-12 |
| T-004-14: Hybrid recv mux tests | `reviewer` | 4 | — | 3-5 |
| T-004-15: Store-and-forward integ. | `reviewer` | 4 | — | 8-12 |
| T-004-16: Integration test (e2e) | `reviewer` | 5 | **Gate 3** | 10-15 |

## Execution Phases

### Phase 1: Prerequisites (Lead only)
- **Tasks**: T-004-01, T-004-02, T-004-12
- **Who**: Lead handles directly
- **Actions**:
  1. T-004-01: Add `axum`, `tokio-tungstenite`, `futures-util`, `url` to workspace Cargo.toml. Update `termchat-relay/Cargo.toml` and `termchat/Cargo.toml`. Update relay `main.rs` to `#[tokio::main]`.
  2. T-004-02: Create `termchat-proto/src/relay.rs` with `RelayMessage` enum + bincode encode/decode helpers + round-trip unit tests. Add `pub mod relay;` to `lib.rs`.
  3. T-004-12: Add `pub mod relay;` to `termchat/src/transport/mod.rs`. Add `[[test]]` section for `relay_fallback`.
- **Gate**: `cargo build` passes across all 3 crates; proto relay tests pass
- **Output**: Both builders are unblocked

### Phase 2A: Relay Server Track (Builder-Relay) — runs in parallel with 2B
- **Tasks**: T-004-03 → T-004-04 → T-004-05 → T-004-10
- **Who**: `builder-relay`
- **Actions**:
  1. T-004-03: axum server with WebSocket upgrade, `RelayState` with peer registry, `Register`/`Registered` flow, duplicate registration handling
  2. T-004-04: `MessageStore` with per-peer FIFO queues, 1000-message cap, optional TTL
  3. T-004-05: `RelayPayload` routing — look up recipient, forward or queue, PeerId spoofing enforcement, queue drain on register
  4. T-004-10: Inline unit tests (TDD) — registry CRUD, message store round-trip, FIFO eviction, routing between two clients
- **TDD pattern**: Builder writes tests alongside each component, not deferred
- **Key guidance for builder**:
  - Use `axum::extract::ws::WebSocket` for the handler
  - Share state via `axum::extract::State(Arc<RelayState>)`
  - Use `tokio::sync::RwLock` for concurrent registry access
  - Bind to `0.0.0.0:9000` in production, `127.0.0.1:0` in tests
  - Server tests: start server in-process with `tokio::spawn`, get bound port, connect via tokio-tungstenite

### Phase 2B: Relay Client Track (Builder-Client) — runs in parallel with 2A
- **Tasks**: T-004-06 → T-004-07 → T-004-08 → T-004-09
- **Who**: `builder-client`
- **Actions**:
  1. T-004-06: `RelayTransport::new()` — WebSocket connect, register, background reader task
  2. T-004-07: Implement `Transport` trait — `send()` encodes `RelayPayload` as bincode binary frame, `recv()` reads from mpsc channel fed by background task, `transport_type()` returns `Relay`
  3. T-004-08: `is_connected()` via `AtomicBool`, disconnect detection in background task, cleanup on Drop
  4. T-004-09: Error mapping — TLS failures, registration rejection, malformed frames, `Queued` responses
- **Key guidance for builder**:
  - Use `tokio_tungstenite::connect_async()` for WebSocket connection
  - Split WebSocket with `futures_util::StreamExt` + `SinkExt`
  - Background reader: `tokio::spawn` reads from WebSocket stream, parses `RelayMessage`, pushes `(PeerId, Vec<u8>)` into mpsc channel
  - `recv()` just awaits on mpsc receiver
  - `send()` locks the ws_sender, encodes RelayMessage, sends binary frame
  - All tests need in-process relay server — builder should implement a minimal test helper or coordinate with Builder-Relay's server

### Phase 3: HybridTransport Refactor (Builder-Client)
- **Tasks**: T-004-13
- **Who**: `builder-client` (owns `hybrid.rs`)
- **Actions**:
  1. Replace `self.preferred.recv().await` with `tokio::select!` across both transports
  2. **RPITIT risk**: Transport trait uses `impl Future` return. `tokio::select!` needs futures that are `Unpin`. Two approaches:
     - Try direct `tokio::select!` — may work if compiler can resolve the RPITIT
     - Fallback: `Box::pin(self.preferred.recv())` and `Box::pin(self.fallback.recv())`
  3. Update doc comment
  4. Verify existing hybrid tests still pass
- **Depends on**: T-004-12 (module wiring must exist)
- **No dependency on Phase 2A/2B**: This is a refactor of existing code, can run after Phase 1

### Phase 4: Reviewer Tests (Reviewer)
- **Tasks**: T-004-11, T-004-14, T-004-15
- **Who**: `reviewer` — writes tests blind against postconditions
- **Depends on**: Phase 2A + 2B + 3 complete (need working server + client + hybrid mux)
- **Actions**:
  1. T-004-11: RelayTransport unit tests — connect, register, send/recv round-trip, is_connected, disconnect, timeout, error paths
  2. T-004-14: HybridTransport recv mux tests — recv from fallback works, interleaved sends, one transport closed
  3. T-004-15: Store-and-forward integration — send while offline, connect, receive queued messages, FIFO order
- **Commands**: `cargo test -p termchat -- relay::tests && cargo test -p termchat -- hybrid::tests`

### Phase 5: Integration Test + Final Gate (Reviewer)
- **Tasks**: T-004-16
- **Who**: `reviewer`
- **Actions**: Write `tests/integration/relay_fallback.rs` — the UC-004 verification test covering all postconditions
- **Gate 3 (Final)**: Full quality gate

## Review Gates

### Gate 1: Relay Server Contract
- **After**: T-004-03, T-004-04, T-004-05, T-004-10 (all server tasks)
- **Reviewer checks**:
  - Relay server starts and accepts WebSocket connections
  - Peer registry works (register, unregister, duplicate replacement)
  - Message routing: connected peer receives immediately, offline peer gets queued
  - Queue drain on connect works
  - PeerId spoofing enforcement: `from` field overwritten by server
  - 64KB payload limit enforced
  - 1000-message queue cap with FIFO eviction
  - All inline unit tests pass
- **Commands**: `cargo test -p termchat-relay && cargo clippy -p termchat-relay -- -D warnings`
- **Pass criteria**: All tests green, clippy clean
- **On failure**: Lead identifies failing tests and assigns fix to `builder-relay`

### Gate 2: Relay Client Contract
- **After**: T-004-06, T-004-07, T-004-08, T-004-09 (all client tasks)
- **Reviewer checks**:
  - `RelayTransport` implements all 4 `Transport` trait methods
  - `transport_type()` returns `TransportType::Relay`
  - Connect/register flow works
  - Send/recv round-trip through relay
  - `is_connected()` detects disconnect
  - Error mapping covers all extension paths (timeout, unreachable, TLS, registration rejection)
- **Commands**: `cargo test -p termchat -- relay && cargo clippy -p termchat -- -D warnings`
- **Pass criteria**: Implementation compiles, builder's basic smoke tests pass, clippy clean
- **On failure**: Lead identifies issue and assigns fix to `builder-client`

### Gate 3: Final UC-004 Verification
- **After**: T-004-16 (integration test) + all tasks complete
- **Reviewer checks**:
  - All 8 success postconditions verified by test
  - All 4 failure postconditions have code paths
  - All 4 invariants maintained
  - All 16 extension error paths handled
  - 18 acceptance criteria checked
  - Code quality: fmt, clippy, full test suite
- **Commands**: `cargo fmt --check && cargo clippy -- -D warnings && cargo test && cargo test --test relay_fallback`
- **Pass criteria**: All commands exit 0
- **On failure**: Specific rework tasks created and assigned to the responsible builder

## Parallelization Opportunities

```
Timeline (phases →)

Phase:    1              2A+2B (parallel)              3       4           5
         ┌────────────┐ ┌──────────────────────────┐ ┌─────┐ ┌─────────┐ ┌──────────┐
lead:    │01+02+12    │ │  Gates 1&2 (review)      │ │coord│ │coord    │ │Gate 3    │
         └────────────┘ └──────────────────────────┘ └─────┘ └─────────┘ └──────────┘
                        ┌──────────────────────────┐
b-relay:                │03 → 04 → 05 → 10        │  (done)
                        └──────────────────────────┘
                        ┌──────────────────────────┐ ┌─────┐
b-client:               │06 → 07 → 08 → 09        │ │ 13  │ (done)
                        └──────────────────────────┘ └─────┘
                                                             ┌─────────┐ ┌──────────┐
reviewer:                                                    │11+14+15 │ │ 16       │
                                                             └─────────┘ └──────────┘
```

**High parallelism**: Phases 2A and 2B run simultaneously — the two builders work on completely separate files in separate crates. This cuts ~40% off the wall-clock time compared to sequential execution.

**Builder-Client gets a head start on Phase 3**: T-004-13 (hybrid recv mux) only depends on T-004-12 (module wiring, done in Phase 1), not on the relay server. Builder-Client can start it immediately after finishing Phase 2B, while Builder-Relay may still be finishing Phase 2A.

## Risk Mitigation

| Risk | Task(s) | Mitigation |
|------|---------|------------|
| axum 0.8 WebSocket API changes | T-004-03 | Lead verifies axum version compiles in T-004-01. Builder should reference axum 0.8 WebSocket upgrade examples. Key: `axum::extract::ws::WebSocketUpgrade` handler. |
| tokio-tungstenite 0.26 API | T-004-06 | Lead verifies version in T-004-01. Key API: `connect_async(url)` returns `(WebSocketStream, Response)`. Use `futures_util::stream::StreamExt::split()` to get sink/stream halves. |
| RPITIT + tokio::select! incompatibility | T-004-13 | Try direct select! first. If RPITIT futures aren't Unpin, use `Box::pin()`. Fallback: spawn recv tasks to mpsc channels and select on receivers instead. |
| In-process server testing complexity | T-004-10, T-004-11 | Both builders need test helpers to start relay in-process. Builder-Relay writes a `pub async fn start_test_server() -> (SocketAddr, JoinHandle)` helper. Builder-Client imports this for client tests, OR writes a minimal equivalent. |
| Builder-Client needs relay server for tests but server might not be ready | T-004-11 | Reviewer writes T-004-11 _after_ both tracks complete (Phase 4). Builder-Client only writes implementation (no unit tests) — Reviewer validates all client behavior. Builder-Client can write basic compile-check tests without a server. |
| Port conflicts in parallel test execution | T-004-10, T-004-11 | All tests MUST use `127.0.0.1:0` (OS-assigned ports). Never hardcode ports. |
| axum + tokio-tungstenite use different tungstenite versions | T-004-01 | Check that `axum`'s internal tungstenite version matches `tokio-tungstenite`'s. If not, use `axum`'s built-in WebSocket support only on server side, `tokio-tungstenite` only on client side — they don't need to share types. |

## Spawn Commands

```
# 1. Lead completes Phase 1 directly (T-004-01, T-004-02, T-004-12)
# No team needed — lead edits Cargo.toml, proto, and mod.rs directly

# 2. Create the team
TeamCreate: team_name="uc-004-impl", description="UC-004 Relay Messages via Server"

# 3. Create tasks in shared task list (16 tasks via TaskCreate)

# 4. Spawn BOTH builders simultaneously (parallel Phase 2A + 2B)
Task tool: name="builder-relay", team_name="uc-004-impl", subagent_type="general-purpose", model="sonnet", max_turns=25
  Prompt: "You are Builder-Relay for UC-004 implementation. You exclusively own all files in `termchat-relay/src/`.
  Read docs/tasks/uc-004-tasks.md and docs/use-cases/uc-004-relay-messages-via-server.md.
  Your tasks: T-004-03, T-004-04, T-004-05, T-004-10 (relay server track).
  Key guidance:
  - axum 0.8 WebSocket: use `axum::extract::ws::{WebSocket, WebSocketUpgrade, Message}`
  - Share state via `axum::extract::State(Arc<RelayState>)`
  - Peer registry: `RwLock<HashMap<String, mpsc::UnboundedSender<Message>>>`
  - MessageStore: `RwLock<HashMap<String, VecDeque<StoredMessage>>>` with 1000-cap
  - PeerId enforcement: override `from` field in RelayPayload with registered PeerId
  - Write inline `#[cfg(test)]` tests for each component (TDD)
  - Test helper: `pub async fn start_test_server() -> (SocketAddr, JoinHandle<()>)` using 127.0.0.1:0
  - Bincode encode/decode RelayMessage using termchat-proto relay module
  After completing each task, mark it done via TaskUpdate and check TaskList for next work."

Task tool: name="builder-client", team_name="uc-004-impl", subagent_type="general-purpose", model="sonnet", max_turns=25
  Prompt: "You are Builder-Client for UC-004 implementation. You exclusively own `termchat/src/transport/relay.rs` and `termchat/src/transport/hybrid.rs`.
  Read docs/tasks/uc-004-tasks.md and docs/use-cases/uc-004-relay-messages-via-server.md.
  Your tasks: T-004-06, T-004-07, T-004-08, T-004-09 (relay client track), then T-004-13 (hybrid recv mux).
  Key guidance:
  - tokio-tungstenite: `connect_async(url)` to connect, `StreamExt::split()` for sink/stream
  - Background reader: `tokio::spawn` reading from stream, pushing to mpsc channel
  - send(): lock ws_sender, encode RelayMessage::RelayPayload as bincode, send as Binary frame
  - recv(): await on mpsc receiver
  - is_connected(): AtomicBool, set to false in background task on WS close
  - T-004-13: replace hybrid.rs recv() TODO with tokio::select! across both transports. If RPITIT prevents select!, use Box::pin().
  - Do NOT write extensive unit tests — Reviewer will write those. Focus on implementation + compile checks.
  After completing each task, mark it done via TaskUpdate and check TaskList for next work."

# 5. After Gates 1+2, spawn reviewer
Task tool: name="reviewer", team_name="uc-004-impl", subagent_type="general-purpose", model="sonnet", max_turns=20
  Prompt: "You are the Reviewer for UC-004. You write tests against postconditions, not implementation.
  Read docs/use-cases/uc-004-relay-messages-via-server.md for postconditions and invariants.
  Read termchat/src/transport/mod.rs for the Transport trait contract.
  Your tasks: T-004-11 (relay client unit tests), T-004-14 (hybrid recv mux tests), T-004-15 (store-and-forward integration), T-004-16 (relay_fallback end-to-end integration test).
  Test against POSTCONDITIONS:
  - transport_type() returns Relay
  - Message round-trips through relay
  - Store-and-forward works (offline → connect → receive)
  - HybridTransport falls back to relay
  - FIFO ordering preserved
  - Disconnect detection works
  - Multiple concurrent peers
  Write T-004-11 in termchat/src/transport/relay.rs #[cfg(test)] module.
  Write T-004-14 in termchat/src/transport/hybrid.rs #[cfg(test)] module (append to existing tests).
  Write T-004-15 and T-004-16 in tests/integration/relay_fallback.rs.
  After completing each task, mark it done via TaskUpdate and check TaskList for next work."

# 6. Lead monitors progress and runs review gates
```

## Coordination Notes

- **Strict file ownership**: Builder-Relay never touches `termchat/`, Builder-Client never touches `termchat-relay/`. This eliminates merge conflicts entirely.
- **Shared dependency**: Both builders consume `termchat-proto::relay::RelayMessage` (read-only). Lead creates this first.
- **Test server helper**: Builder-Relay creates a `pub` test server helper function. Reviewer (and possibly Builder-Client) can import it. If Builder-Client needs it before Relay is done, they can write a minimal standalone helper.
- **Communication protocol**: Builders message lead after each task completion. Lead runs gate checks. Reviewer messages lead with test results.
- **Commit strategy**: One commit after all tasks pass Gate 3. Lead manages the commit.
- **Escalation**: If axum WebSocket or tokio-tungstenite APIs cause persistent failures, Lead can research docs and provide specific code snippets to unblock builders.
