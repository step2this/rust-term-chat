# Use Case: UC-011 Auto-Reconnect to Relay on Disconnect

## Classification
- **Goal Level**: 🐟 Subfunction
- **Scope**: Component (white box) — `termchat/src/net.rs`
- **Priority**: P1 High
- **Complexity**: 🟡 Medium

## Actors
- **Primary Actor**: System (networking supervisor)
- **Supporting Actors**: Relay server, TUI main loop
- **Stakeholders & Interests**:
  - User: chat session survives transient network blips without manual restart
  - TUI: `cmd_tx`/`evt_rx` channel pair never changes (stable interface)

## Conditions
- **Preconditions**:
  1. `spawn_net()` returned successfully with a connected relay session
  2. Background tasks (receive_loop, command_handler, chat_event_forwarder) are running
- **Success Postconditions**:
  1. TUI shows status progression: "Disconnected" -> "Reconnecting (attempt N)" -> "Reconnected"
  2. Messages queued during disconnection are sent after reconnect
  3. `cmd_tx`/`evt_rx` channel pair from `spawn_net` never changes (TUI code unchanged)
  4. Backoff: 1s initial, doubling to 30s cap, max 10 attempts, random jitter
  5. Flap detection: backoff doesn't reset if connection was stable < 30s
- **Failure Postconditions**:
  1. After max retries exhausted, system enters dormant mode with 60s background poll
  2. User sees "Reconnection failed — will retry in background" message
- **Invariants**:
  1. TUI never blocks on reconnection (all reconnect work is async in background tasks)
  2. No message loss for messages sent during disconnection (up to queue cap of 100)
  3. Shutdown command always works, even during active reconnection

## Main Success Scenario
1. Relay WebSocket connection drops (server restart, network blip, etc.)
2. `receive_loop` detects connection closed and exits
3. Supervisor task detects `receive_loop` exit via `JoinHandle`
4. Supervisor sends `NetEvent::ConnectionStatus { connected: false }` to TUI
5. Supervisor sends `NetEvent::Reconnecting { attempt: 1, max_attempts: 10 }` to TUI
6. Supervisor calls `reconnect_with_backoff()` with exponential backoff + jitter
7. `RelayTransport::connect()` succeeds
8. Supervisor creates new `ChatManager` with new transport
9. Supervisor swaps `Arc<RwLock<Option<ChatManager>>>` to new instance
10. Supervisor spawns new `receive_loop` and `chat_event_forwarder` tasks
11. Supervisor drains message queue, sending each via new `ChatManager`
12. Supervisor sends `NetEvent::ConnectionStatus { connected: true }` to TUI
13. Normal operation resumes

## Extensions
- **6a. All retry attempts exhausted**:
  1. Supervisor sends `NetEvent::ReconnectFailed` to TUI
  2. Supervisor enters dormant mode: 60s sleep then single reconnect attempt, repeating
  3. If dormant attempt succeeds, returns to step 8
- **6b. Relay URL unreachable (DNS failure)**:
  1. Counted as a failed attempt, backoff continues
- **11a. Queue contains more than 100 messages**:
  1. Oldest messages beyond cap were already dropped at enqueue time
  2. Remaining messages are sent in FIFO order
- **MSS-any. Shutdown received during reconnect**:
  1. Supervisor checks shutdown flag before each reconnect attempt
  2. Supervisor exits cleanly, dropping all background tasks
- **MSS-any. Send command received during disconnect**:
  1. `command_handler` detects `ChatManager` is `None` (via `Arc<RwLock>`)
  2. Message is pushed to `VecDeque<String>` queue (capped at 100)
  3. User sees no error (message will be sent after reconnect)

## Agent Execution Notes
- **Verification Command**: `cargo test --test relay_reconnect`
- **Test File**: `tests/integration/relay_reconnect.rs`
- **Depends On**: UC-010 (Live Relay Messaging)
- **Blocks**: None
- **Estimated Complexity**: Medium / ~3000 tokens per agent turn
- **Agent Assignment**: Single agent (follows established patterns)

## Acceptance Criteria
- [ ] `cargo test --test relay_reconnect` passes
- [ ] Supervisor pattern preserves `cmd_tx`/`evt_rx` interface (no TUI changes needed)
- [ ] Reconnection with exponential backoff + jitter is implemented
- [ ] Messages queued during disconnect are delivered after reconnect
- [ ] Graceful shutdown works during active reconnection
- [ ] `cargo clippy -- -D warnings` passes
- [ ] All public functions have doc comments
