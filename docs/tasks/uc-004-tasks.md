# Tasks for UC-004: Relay Messages via Server

Generated from use case on 2026-02-07.

## Summary
- **Total tasks**: 16
- **Implementation tasks**: 10
- **Test tasks**: 4
- **Prerequisite tasks**: 1
- **Refactor tasks**: 1
- **Critical path**: T-004-01 → T-004-02 → T-004-03 → T-004-05 → T-004-06 → T-004-07 → T-004-10 → T-004-11 → T-004-14 → T-004-16
- **Estimated total size**: XL (collectively ~1200-1800 lines of implementation + tests across 2 crates)

## Dependency Graph

```
T-004-01 (Add deps to both crates)
  ├── T-004-02 (RelayMessage protocol types in termchat-proto)
  │     ├── T-004-03 (Relay server: axum + WS handler + peer registry)
  │     │     ├── T-004-04 (Relay server: store-and-forward queue)
  │     │     │     └── T-004-05 (Relay server: message routing + queue drain)
  │     │     │           └── T-004-10 (Test: relay server unit tests)
  │     │     └── T-004-05
  │     │
  │     ├── T-004-06 (RelayTransport: connect + register)
  │     │     ├── T-004-07 (RelayTransport: send + recv via Transport trait)
  │     │     │     ├── T-004-08 (RelayTransport: is_connected + disconnect detection)
  │     │     │     │     └── T-004-11 (Test: RelayTransport unit tests)
  │     │     │     └── T-004-11
  │     │     └── T-004-09 (RelayTransport: error handling for all extensions)
  │     │           └── T-004-11
  │     │
  │     └── T-004-12 (Wire up relay module + Cargo.toml test entries)
  │
  └── T-004-12

Parallel tracks after T-004-02:
  Track A: T-004-03 → T-004-04 → T-004-05 → T-004-10 (relay server)
  Track B: T-004-06 → T-004-07 → T-004-08 → T-004-09 → T-004-11 (relay client)

After both tracks complete:
  T-004-13 (HybridTransport recv() multiplexing)
  T-004-14 (Test: hybrid recv multiplexing)
  T-004-15 (Test: store-and-forward integration)
  T-004-16 (Integration test: relay_fallback end-to-end)
```

## Tasks

### T-004-01: Add dependencies to both crates
- **Type**: Prerequisite
- **Module**: `termchat/Cargo.toml`, `termchat-relay/Cargo.toml`, `Cargo.toml` (workspace)
- **Description**: Add all new dependencies for UC-004:
  - **termchat-relay/Cargo.toml**: `axum = "0.8"`, `tokio-tungstenite = "0.26"`, `futures-util = "0.3"`, `bincode = { workspace = true }`, `tracing-subscriber = { workspace = true }` (for relay server startup logging)
  - **termchat/Cargo.toml**: `tokio-tungstenite = "0.26"`, `futures-util = "0.3"`, `url = "2"` (for relay URL parsing)
  - **Workspace Cargo.toml**: Add `futures-util`, `tokio-tungstenite`, `url` to `[workspace.dependencies]` if not already present. Add `axum` to workspace deps.
  - Update `termchat-relay/src/main.rs` to use `#[tokio::main]` async entry point (replacing the stub `fn main()`).
  - Verify workspace compiles: `cargo build`
- **From**: Precondition 3 (relay server running), Implementation Notes
- **Depends On**: None
- **Blocks**: T-004-02, T-004-12
- **Size**: S
- **Risk**: Medium (axum 0.8 + tokio-tungstenite version compatibility; check crate versions are compatible)
- **Agent Assignment**: Lead (Cargo.toml files are lead-owned)
- **Acceptance Test**: `cargo build` succeeds across all 3 crates with new deps

---

### T-004-02: Define RelayMessage protocol types in termchat-proto
- **Type**: Implementation
- **Module**: `termchat-proto/src/relay.rs` (new file), `termchat-proto/src/lib.rs`
- **Description**: Define the relay wire protocol as a bincode-encoded enum:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
  pub enum RelayMessage {
      Register { peer_id: String },
      Registered { peer_id: String },
      RelayPayload { from: String, to: String, payload: Vec<u8> },
      Queued { to: String, count: u32 },
      Error { reason: String },
  }
  ```
  - Add `pub mod relay;` to `termchat-proto/src/lib.rs`
  - Add encode/decode helper functions using the existing `termchat-proto` bincode codec pattern
  - Add unit tests for RelayMessage round-trip serialization (bincode encode → decode)
  - All variants must be tested
- **From**: Implementation Notes (relay wire protocol), MSS Steps 5-7
- **Depends On**: T-004-01
- **Blocks**: T-004-03, T-004-06, T-004-12
- **Size**: M
- **Risk**: Low (straightforward serde + bincode, follows existing pattern in `message.rs`)
- **Agent Assignment**: Teammate:Builder (proto specialist)
- **Acceptance Test**: `cargo test -p termchat-proto` — relay serialization tests pass

---

### T-004-03: Implement relay server: axum entry point + WebSocket handler + peer registry
- **Type**: Implementation
- **Module**: `termchat-relay/src/main.rs`, `termchat-relay/src/relay.rs` (new)
- **Description**: Build the core relay server infrastructure:
  - **`main.rs`**: axum server setup with tracing subscriber, bind to configurable address (default `0.0.0.0:9000`), WebSocket upgrade route at `/ws`.
  - **`relay.rs`**: `RelayState` shared state struct containing:
    - `connections: RwLock<HashMap<String, WebSocketSender>>` — maps PeerId → sender half of WS
    - Methods: `register(peer_id, sender)`, `unregister(peer_id)`, `get_sender(peer_id)`
  - WebSocket handler: accept connection, read first message as `RelayMessage::Register`, validate, call `RelayState::register()`, send back `RelayMessage::Registered`. Then enter a message loop reading `RelayMessage` variants.
  - Handle `RelayMessage::Register` → register peer, respond with `Registered`
  - Handle duplicate registration (ext 6b): replace old connection, close previous WebSocket
  - Handle registration rejection (ext 5a): respond with `RelayMessage::Error` if rate-limited or full
  - On WebSocket close: call `unregister(peer_id)`
  - Use `Arc<RelayState>` shared across all connections via axum state
- **From**: MSS Steps 5-6, Extensions 5a, 6a, 6b
- **Depends On**: T-004-02
- **Blocks**: T-004-04, T-004-05, T-004-10
- **Size**: L
- **Risk**: High (axum WebSocket API, concurrent state management, first real axum usage in project)
- **Agent Assignment**: Teammate:Builder-Relay
- **Acceptance Test**: Relay server starts, accepts WebSocket connections, registers peers

---

### T-004-04: Implement relay server: in-memory store-and-forward queue
- **Type**: Implementation
- **Module**: `termchat-relay/src/store.rs` (new)
- **Description**: Implement the message store for offline peers:
  - `MessageStore` struct with `queues: RwLock<HashMap<String, VecDeque<StoredMessage>>>`
  - `StoredMessage` struct: `{ from: String, payload: Vec<u8>, queued_at: Instant }`
  - `pub async fn enqueue(to: &str, from: &str, payload: Vec<u8>) -> u32` — adds to peer's queue, returns queue size. Enforces per-peer cap (1000 messages, FIFO eviction: drop oldest when full).
  - `pub async fn drain(peer_id: &str) -> Vec<StoredMessage>` — returns all queued messages for a peer and clears the queue (called when peer registers)
  - `pub async fn queue_len(peer_id: &str) -> u32` — returns current queue size for a peer
  - Per-peer FIFO eviction: when queue exceeds 1000 entries, drop the oldest
  - Optional: message TTL (1 hour default) — checked on drain, expired messages discarded
- **From**: Extensions 8a, 8b, Variation 8a-alt, Invariant 4 (in-memory only)
- **Depends On**: T-004-03
- **Blocks**: T-004-05
- **Size**: M
- **Risk**: Low (straightforward data structure, no external dependencies)
- **Agent Assignment**: Teammate:Builder-Relay
- **Acceptance Test**: Unit tests: enqueue/drain round-trip, FIFO eviction at 1000, TTL expiry

---

### T-004-05: Implement relay server: message routing + queue drain on connect
- **Type**: Implementation
- **Module**: `termchat-relay/src/relay.rs`
- **Description**: Complete the relay server message handling loop:
  - Handle `RelayMessage::RelayPayload { from, to, payload }`:
    1. **Server-side PeerId enforcement (ext 11a)**: Override `from` field with the connection's registered PeerId (prevent spoofing)
    2. Look up `to` in connection registry
    3. If recipient connected: forward payload via their WebSocket sender
    4. If recipient not connected (ext 8a/8b): enqueue in `MessageStore`, respond with `RelayMessage::Queued { to, count }`
  - Handle forwarding failure (ext 9a): if WebSocket send fails, re-queue the message, unregister the failed recipient
  - **Queue drain on register**: When a new peer registers, drain any queued messages from `MessageStore` and send them immediately
  - Payload size enforcement (ext 7b): reject payloads > 64KB with `RelayMessage::Error`
  - Log all routing decisions with tracing
- **From**: MSS Steps 7-9, Extensions 7b, 8a, 8b, 9a, 11a
- **Depends On**: T-004-03, T-004-04
- **Blocks**: T-004-10
- **Size**: M
- **Risk**: Medium (concurrent WebSocket sends, race conditions between registration and routing)
- **Agent Assignment**: Teammate:Builder-Relay
- **Acceptance Test**: Relay routes messages between two connected peers; queues messages for offline peers; drains queue on reconnect

---

### T-004-06: Implement RelayTransport: WebSocket connect + register
- **Type**: Implementation
- **Module**: `termchat/src/transport/relay.rs` (new)
- **Description**: Build the relay client that connects to the relay server:
  - `RelayTransport` struct holding:
    - `local_id: PeerId` — this client's identity
    - `relay_url: String` — the relay server URL (ws:// or wss://)
    - `ws_sender: Arc<Mutex<Option<SplitSink<...>>>>` — WebSocket write half
    - `incoming: Arc<Mutex<mpsc::Receiver<(PeerId, Vec<u8>)>>>` — channel for received messages
    - `connected: Arc<AtomicBool>` — connection state flag
  - `pub async fn new(relay_url: &str, local_id: PeerId) -> Result<Self, TransportError>`:
    1. Connect to the relay WebSocket URL using `tokio_tungstenite::connect_async()` with a 10s timeout
    2. Split the WebSocket into sender and receiver halves
    3. Send `RelayMessage::Register { peer_id: local_id }` via the sender
    4. Wait for `RelayMessage::Registered` acknowledgment (5s timeout per ext 6a)
    5. Spawn a background task to read incoming WebSocket messages, parse `RelayMessage::RelayPayload`, and push `(from_peer_id, payload)` into the `incoming` channel
    6. Set `connected` to `true`
  - Map errors: DNS/network → `TransportError::Unreachable`, timeout → `TransportError::Timeout`, TLS → `TransportError::Io`
  - Handle WebSocket close in background task: set `connected` to `false`
- **From**: MSS Steps 3-6, Extensions 3a, 4a, 4b, 4c, 6a
- **Depends On**: T-004-02
- **Blocks**: T-004-07, T-004-09, T-004-11
- **Size**: L
- **Risk**: High (tokio-tungstenite API, WebSocket lifecycle management, background task coordination)
- **Agent Assignment**: Teammate:Builder-Client
- **Acceptance Test**: RelayTransport connects to relay server, registers, receives `Registered` ack

---

### T-004-07: Implement RelayTransport: send + recv via Transport trait
- **Type**: Implementation
- **Module**: `termchat/src/transport/relay.rs`
- **Description**: Implement the `Transport` trait for `RelayTransport`:
  - **`send(peer, payload)`**:
    1. Check `connected` flag — if false, return `TransportError::ConnectionClosed`
    2. Encode `RelayMessage::RelayPayload { from: local_id, to: peer, payload }` with bincode
    3. Send as WebSocket binary frame via `ws_sender`
    4. Map errors: broken pipe → `ConnectionClosed`, other → `Io`
  - **`recv()`**:
    1. Await on the `incoming` mpsc channel receiver
    2. Return `(PeerId, Vec<u8>)` from the channel
    3. If channel is closed (background task exited), return `TransportError::ConnectionClosed`
  - **Note**: Unlike `QuicTransport` which validates PeerId against a single remote peer, `RelayTransport` can send to ANY peer (relay routes by PeerId). So `send(peer, _)` does NOT validate `peer` — the relay handles routing.
  - **`transport_type()`**: Return `TransportType::Relay`
- **From**: MSS Steps 7, 10-11, Postconditions 2-3
- **Depends On**: T-004-06
- **Blocks**: T-004-08, T-004-11
- **Size**: M
- **Risk**: Medium (mpsc channel coordination, WebSocket binary frame encoding)
- **Agent Assignment**: Teammate:Builder-Client
- **Acceptance Test**: send/recv round-trip through relay server on localhost

---

### T-004-08: Implement RelayTransport: is_connected + disconnect detection
- **Type**: Implementation
- **Module**: `termchat/src/transport/relay.rs`
- **Description**: Complete the Transport trait implementation:
  - **`is_connected(peer)`**: Returns `true` if the WebSocket connection to the relay server is active (`connected` flag is `true`). Note per postcondition 2: this indicates relay server connectivity, not whether the specific peer is registered.
  - **Disconnect detection**: The background reader task must detect WebSocket close/error and set `connected` to `false`. Also close the `incoming` mpsc channel so `recv()` returns `ConnectionClosed`.
  - **Drop cleanup**: Implement `Drop` for `RelayTransport` or provide a `close()` method that cleanly closes the WebSocket connection.
  - Handle ext 7a: if send fails mid-message, set `connected = false` and return `ConnectionClosed`
- **From**: Extensions 7a, Postcondition 2, Invariant 2
- **Depends On**: T-004-07
- **Blocks**: T-004-11
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder-Client
- **Acceptance Test**: `is_connected()` returns true when connected, false after disconnect; recv returns error after disconnect

---

### T-004-09: Implement RelayTransport: extension error handling
- **Type**: Implementation
- **Module**: `termchat/src/transport/relay.rs`
- **Description**: Handle remaining extension error paths:
  - **Ext 4c (TLS failure)**: If `wss://` URL fails TLS, map to `TransportError::Io`
  - **Ext 5a (registration rejected)**: Parse `RelayMessage::Error` during registration, return `TransportError::Io` with reason
  - **Ext 10a (malformed frame)**: In background reader task, log and skip malformed WebSocket messages instead of crashing
  - **Ext 12a (ack lost)**: No specific handling needed — caller (ChatManager) handles missing acks via retry
  - Handle `RelayMessage::Queued` responses: log that message was queued (relay-side), treat as send success (message accepted by relay)
  - Handle `RelayMessage::Error` during operation: log error, optionally surface to caller via a notification channel
- **From**: Extensions 4c, 5a, 10a, 12a
- **Depends On**: T-004-06
- **Blocks**: T-004-11
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder-Client
- **Acceptance Test**: Error paths covered by unit tests in T-004-11

---

### T-004-10: Test — Relay server unit tests
- **Type**: Test
- **Module**: `termchat-relay/src/relay.rs`, `termchat-relay/src/store.rs` (inline `#[cfg(test)]`)
- **Description**: Unit tests for the relay server components:
  - **RelayState tests**:
    - Register a peer, verify it appears in the registry
    - Register same PeerId twice, verify old connection replaced (ext 6b)
    - Unregister a peer, verify removed
    - Get sender for registered peer returns Some; for unknown peer returns None
  - **MessageStore tests**:
    - Enqueue and drain round-trip
    - FIFO ordering preserved
    - Per-peer cap (1000): enqueue 1001, verify oldest dropped
    - Drain returns empty vec for unknown peer
    - Multiple peers have independent queues
  - **Message routing tests** (requires starting the server in-process):
    - Two clients connect, register, exchange a message via relay
    - Client A sends to offline Client B, message is queued
    - Client B connects, queued messages are drained and delivered
    - PeerId spoofing: client sends `from: "fake"`, verify server overwrites with registered PeerId
    - Oversized payload (>64KB) rejected with Error response
- **From**: All relay server extensions, Postconditions 4-5, Invariant 3-4
- **Depends On**: T-004-03, T-004-04, T-004-05
- **Blocks**: T-004-16
- **Size**: L
- **Risk**: Medium (in-process server testing, async coordination)
- **Agent Assignment**: Teammate:Reviewer
- **Acceptance Test**: `cargo test -p termchat-relay` — all relay tests pass

---

### T-004-11: Test — RelayTransport unit tests
- **Type**: Test
- **Module**: `termchat/src/transport/relay.rs` (inline `#[cfg(test)]`)
- **Description**: Unit tests for the relay client:
  - Connect to a test relay server (start in-process), register successfully
  - `transport_type()` returns `TransportType::Relay`
  - `is_connected()` returns true after connect, false after server shutdown
  - Send/recv round-trip through relay (two RelayTransport instances)
  - Multiple messages preserve FIFO order
  - Send after disconnect returns `TransportError::ConnectionClosed`
  - Recv after disconnect returns `TransportError::ConnectionClosed`
  - Registration timeout (ext 6a): connect to server that doesn't send Registered
  - Connection failure: connect to non-existent server returns error
  - Malformed frame handling: verify background task doesn't crash (ext 10a)
- **From**: Success Postconditions 1-3, Invariants 2-3, Extensions 4a, 6a, 7a, 10a
- **Depends On**: T-004-06, T-004-07, T-004-08, T-004-09
- **Blocks**: T-004-16
- **Size**: M
- **Risk**: Medium (needs in-process relay server for testing)
- **Agent Assignment**: Teammate:Reviewer
- **Acceptance Test**: `cargo test -p termchat -- relay::tests` — all relay client tests pass

---

### T-004-12: Wire up relay module + Cargo.toml test entries
- **Type**: Implementation
- **Module**: `termchat/src/transport/mod.rs`, `termchat/Cargo.toml`
- **Description**: Register the new relay module and test infrastructure:
  - Add `pub mod relay;` to `termchat/src/transport/mod.rs`
  - Update the module doc comment to list WebSocket relay as an available implementation
  - Add `[[test]]` section in `termchat/Cargo.toml` for `relay_fallback` integration test: `name = "relay_fallback"`, `path = "../tests/integration/relay_fallback.rs"`
  - Verify `cargo build` succeeds with new module
- **From**: Infrastructure wiring
- **Depends On**: T-004-01, T-004-02
- **Blocks**: T-004-13, T-004-16
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Lead (Cargo.toml is lead-owned)
- **Acceptance Test**: `cargo build` succeeds with relay module visible

---

### T-004-13: Update HybridTransport recv() to multiplex both transports
- **Type**: Refactor
- **Module**: `termchat/src/transport/hybrid.rs`
- **Description**: Fix the TODO in `HybridTransport::recv()` (line 207-210):
  - Replace `self.preferred.recv().await` with `tokio::select!` that races both transports:
    ```rust
    tokio::select! {
        result = self.preferred.recv() => result,
        result = self.fallback.recv() => result,
    }
    ```
  - This ensures messages arriving via relay are received alongside P2P messages
  - Update the doc comment to reflect the new multiplexed behavior
  - **Critical**: This is required for relay to actually work end-to-end. Without it, relayed messages are never received by the application layer.
  - Ensure existing hybrid transport tests still pass (they use LoopbackTransport as both preferred and fallback, so `select!` should work fine)
- **From**: Postcondition 8 (HybridTransport recv multiplexing)
- **Depends On**: T-004-12
- **Blocks**: T-004-14, T-004-16
- **Size**: S
- **Risk**: Medium (tokio::select! behavior with Transport trait's RPITIT — may need to pin futures or box them)
- **Agent Assignment**: Teammate:Builder-Client
- **Acceptance Test**: Existing hybrid tests pass; new test verifies recv from fallback transport works

---

### T-004-14: Test — HybridTransport recv multiplexing
- **Type**: Test
- **Module**: `termchat/src/transport/hybrid.rs` (inline `#[cfg(test)]`)
- **Description**: New tests for the updated recv multiplexing:
  - Send via fallback transport, verify `recv()` on hybrid returns the message
  - Send via preferred, verify still works (regression)
  - Interleave sends on both transports, verify all messages received
  - One transport closed, messages from the other still arrive
- **From**: Postcondition 8
- **Depends On**: T-004-13
- **Blocks**: T-004-16
- **Size**: S
- **Risk**: Low (uses existing LoopbackTransport test infrastructure)
- **Agent Assignment**: Teammate:Reviewer
- **Acceptance Test**: `cargo test -p termchat -- hybrid::tests` — all hybrid tests pass (old + new)

---

### T-004-15: Test — Store-and-forward integration
- **Type**: Test
- **Module**: `tests/integration/relay_fallback.rs` (part of integration test)
- **Description**: Integration test specifically for the store-and-forward flow:
  - Start relay server in-process
  - Client A connects and sends a message to offline Client B
  - Verify relay returns `Queued` response
  - Client B connects and registers
  - Client B receives the queued message immediately
  - Verify message content is intact, PeerId is correct
  - Send multiple messages while B is offline, verify all delivered on connect in FIFO order
- **From**: Postcondition 5, Extensions 8a, 8b
- **Depends On**: T-004-10, T-004-11
- **Blocks**: T-004-16
- **Size**: M
- **Risk**: Medium (async coordination between multiple clients and server)
- **Agent Assignment**: Teammate:Reviewer
- **Acceptance Test**: Store-and-forward test passes

---

### T-004-16: Integration test — relay_fallback end-to-end
- **Type**: Test
- **Module**: `tests/integration/relay_fallback.rs`
- **Description**: The primary integration test for UC-004 — validates all success postconditions:
  1. Start relay server in-process (bind to `127.0.0.1:0`, get port)
  2. Create two `RelayTransport` instances, both connecting to the relay
  3. Send message from Client A → Client B via relay, verify receipt
  4. Send message from Client B → Client A (bidirectional)
  5. Verify `transport_type()` returns `TransportType::Relay`
  6. Verify `is_connected()` returns `true` on both sides
  7. Send 50 messages, verify FIFO ordering preserved
  8. Shutdown relay server, verify `is_connected()` returns `false`
  9. Verify send after disconnect returns `TransportError::ConnectionClosed`
  10. Test HybridTransport integration: create `HybridTransport<LoopbackTransport, RelayTransport>` where loopback is broken (preferred fails), verify messages fall through to relay
  11. Multiple peers (3+) exchange messages through the same relay concurrently
  12. Relay message queue cap: verify relay handles 1000+ queued messages with eviction
  13. PeerId enforcement: verify relay overwrites `from` field

  This is the **verification command** test: `cargo test --test relay_fallback`
- **From**: All Success Postconditions, All Invariants, All Acceptance Criteria
- **Depends On**: T-004-10, T-004-11, T-004-13, T-004-14, T-004-15
- **Blocks**: None
- **Size**: L
- **Risk**: Medium (multi-process async coordination, port allocation)
- **Agent Assignment**: Teammate:Reviewer
- **Acceptance Test**: `cargo test --test relay_fallback` passes

---

## Implementation Order

Topologically sorted, with parallel opportunities noted:

| Order | Task | Type | Size | Depends On | Parallel Group |
|-------|------|------|------|------------|----------------|
| 1 | T-004-01: Add dependencies | Prerequisite | S | none | — |
| 2 | T-004-02: RelayMessage protocol types | Implementation | M | T-004-01 | — |
| 3a | T-004-03: Relay server core | Implementation | L | T-004-02 | A (server track) |
| 3b | T-004-06: RelayTransport connect + register | Implementation | L | T-004-02 | B (client track) |
| 3c | T-004-12: Wire up relay module | Implementation | S | T-004-01, T-004-02 | C (parallel with 3a, 3b) |
| 4a | T-004-04: Store-and-forward queue | Implementation | M | T-004-03 | A |
| 4b | T-004-07: RelayTransport send + recv | Implementation | M | T-004-06 | B |
| 5a | T-004-05: Message routing + queue drain | Implementation | M | T-004-03, T-004-04 | A |
| 5b | T-004-08: is_connected + disconnect | Implementation | S | T-004-07 | B |
| 5c | T-004-09: Extension error handling | Implementation | S | T-004-06 | B (parallel with 5b) |
| 6a | T-004-10: Relay server unit tests | Test | L | T-004-05 | A |
| 6b | T-004-11: RelayTransport unit tests | Test | M | T-004-08, T-004-09 | B |
| 7 | T-004-13: HybridTransport recv multiplexing | Refactor | S | T-004-12 | — |
| 8 | T-004-14: Hybrid recv multiplexing tests | Test | S | T-004-13 | — |
| 9 | T-004-15: Store-and-forward integration test | Test | M | T-004-10, T-004-11 | — |
| 10 | T-004-16: Integration test (relay_fallback) | Test | L | T-004-10-15 | — |

## Notes for Agent Team

- **Two parallel tracks**: The relay server (Track A: T-004-03→04→05→10) and relay client (Track B: T-004-06→07→08→09→11) can be built in parallel after T-004-02 completes. This is the primary parallelization opportunity.
- **Lead-owned files**: `Cargo.toml` files (T-004-01) and `transport/mod.rs` wiring (T-004-12) are lead-owned. Lead should complete these early to unblock builders.
- **Shared protocol first**: T-004-02 (RelayMessage types) must complete before either track can start, since both server and client depend on the wire protocol.
- **In-process testing pattern**: Both T-004-10 (server tests) and T-004-11 (client tests) need a relay server running in-process. Use `tokio::spawn` to start the axum server on `127.0.0.1:0`, extract the bound port, and connect clients to it within the same test process.
- **Builder-Relay owns `termchat-relay/`**: All relay server files (`main.rs`, `relay.rs`, `store.rs`). No conflicts with client builder.
- **Builder-Client owns `termchat/src/transport/relay.rs`** and the HybridTransport refactor (T-004-13). No conflicts with relay builder.
- **Reviewer writes T-004-10, T-004-11, T-004-14, T-004-15, T-004-16**: All test tasks. Blind testing against postconditions.
- **Review gate after T-004-05 + T-004-08**: Before integration tests, verify both tracks work independently with unit tests (T-004-10, T-004-11).
- **tokio::select! risk (T-004-13)**: The Transport trait uses RPITIT (return-position `impl Future`). `tokio::select!` may require boxing or pinning. Builder should test this carefully. Fallback approach: use `tokio::spawn` with mpsc channels if `select!` doesn't work with RPITIT.
- **Keep agent tasks small**: Each task is scoped to ~15-20 tool calls. The largest tasks (T-004-03, T-004-06) may need careful scoping to stay within budget.
- **Port allocation**: All tests must use `127.0.0.1:0` with OS-assigned ports. Extract the bound port from the listener for client connections.
