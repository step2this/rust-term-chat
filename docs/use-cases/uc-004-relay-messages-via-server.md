# Use Case: UC-004 Relay Messages via Server

## Classification
- **Goal Level**: 🌊 User Goal
- **Scope**: System (black box)
- **Priority**: P1 High
- **Complexity**: 🔴 High

## Actors
- **Primary Actor**: Sender (terminal user whose P2P connection has failed or is unavailable)
- **Supporting Actors**: Relay Server (axum WebSocket server, store-and-forward), Recipient (terminal user receiving the relayed message), Transport Layer (HybridTransport selects relay as fallback), Crypto Layer (Noise session encrypts before relay touches the payload)
- **Stakeholders & Interests**:
  - Sender: messages are delivered even when P2P is unavailable; the experience is seamless
  - Recipient: messages arrive intact regardless of transport path; no difference in display
  - Relay Operator: server is lightweight, stateless (no persistent storage), and never sees plaintext
  - System: relay satisfies the existing `Transport` trait so the chat pipeline works unchanged

## Conditions
- **Preconditions** (must be true before starting):
  1. Sender's TermChat client is running and has a valid identity keypair
  2. P2P transport (QUIC, UC-003) has failed or is unavailable (e.g., NAT, firewall, peer offline)
  3. Relay server is running and reachable at a known URL (e.g., `ws://relay.example.com:9000`)
  4. Sender knows the Recipient's `PeerId` (used for relay routing)
  5. E2E encryption session is established or can be established via relay (UC-005 handshake messages can be relayed)
- **Success Postconditions** (true when done right):
  1. Sender's message is delivered to the Recipient via the relay server
  2. The relay transport satisfies the `Transport` trait (send, recv, is_connected, transport_type). Note: `is_connected(peer)` returns `true` if the RelayTransport has an active WebSocket connection to the relay server — it cannot know whether the specific peer is registered at the relay. This is acceptable because `send()` may succeed even if the peer is offline (relay queues it via ext 8a).
  3. `transport_type()` returns `TransportType::Relay`
  4. The relay server never sees plaintext — it only forwards encrypted blobs
  5. Messages queued at the relay for an offline Recipient are delivered when the Recipient connects (store-and-forward)
  6. The relay client integrates with `HybridTransport` as the fallback transport
  7. Delivery acknowledgments flow back through the relay path
  8. `HybridTransport::recv()` is updated to multiplex across both P2P and relay transports using `tokio::select!`, so relayed messages are received alongside P2P messages
- **Failure Postconditions** (true when it fails gracefully):
  1. Sender receives a clear error via `TransportError` indicating relay failure
  2. If the relay is also unreachable, the message is queued in `HybridTransport`'s `PendingQueue` for later delivery
  3. No partial connection state leaks on relay disconnection
  4. Failed relay connection attempts are logged with the relay URL and error reason
- **Invariants** (must remain true throughout):
  1. Plaintext message never leaves the application boundary — the relay only handles encrypted payloads
  2. The `Transport` trait contract is fully satisfied (same guarantees as `QuicTransport` and `LoopbackTransport`)
  3. Message ordering is preserved per-peer within the relay path (FIFO)
  4. The relay server is stateless between restarts — no persistent storage of messages or metadata (in-memory queues only)

## Main Success Scenario
1. HybridTransport detects that P2P send has failed (or P2P is not connected to this peer)
2. HybridTransport delegates the send to the fallback `RelayTransport`
3. RelayTransport checks if it has an active WebSocket connection to the relay server
4. If not connected, RelayTransport establishes a WebSocket connection to the configured relay URL
5. RelayTransport registers itself with the relay server by sending a `Register` message containing its `PeerId`
6. Relay server stores the WebSocket↔PeerId mapping in its connection registry and sends a `Registered` acknowledgment back to the client
7. RelayTransport sends a `RelayPayload` message containing the recipient's `PeerId` and the encrypted payload bytes
8. Relay server looks up the Recipient's `PeerId` in its connection registry
9. Relay server forwards the `RelayPayload` to the Recipient's WebSocket connection
10. Recipient's RelayTransport receives the forwarded payload via its WebSocket connection
11. Recipient's RelayTransport returns the payload from `recv()` with the Sender's `PeerId`
12. Message flows through the Recipient's normal receive pipeline (decrypt, deserialize, display — UC-002)

## Extensions (What Can Go Wrong)
- **1a. No relay URL configured (fallback transport not available)**:
  1. HybridTransport has no fallback transport to delegate to
  2. Message is queued in `PendingQueue` for later delivery
  3. Use case fails for this message
- **3a. WebSocket connection to relay is already established**:
  1. Skip to step 7 (send immediately)
- **4a. WebSocket connection fails (relay server unreachable, DNS failure)**:
  1. RelayTransport returns `TransportError::Unreachable` with relay info
  2. HybridTransport queues the message in `PendingQueue`
  3. Use case fails for this message; background flush will retry later
- **4b. WebSocket connection times out (relay overloaded or network issue)**:
  1. RelayTransport returns `TransportError::Timeout` after configurable duration (default 10s)
  2. HybridTransport queues the message in `PendingQueue`
  3. Use case fails for this message
- **4c. WebSocket TLS handshake fails (certificate error for wss:// URLs)**:
  1. RelayTransport returns `TransportError::Io` with TLS error details
  2. Use case fails; likely a configuration error
- **5a. Registration rejected by relay (rate limit, banned PeerId, server full)**:
  1. Relay server sends an error frame with rejection reason
  2. RelayTransport returns `TransportError::Io` with the rejection reason
  3. Use case fails
- **6a. Registration acknowledgment not received within timeout (5s)**:
  1. RelayTransport sent `Register` but relay server did not respond with `Registered`
  2. RelayTransport closes the WebSocket connection and returns `TransportError::Timeout`
  3. Use case fails; HybridTransport queues the message
- **6b. Duplicate registration (client reconnects with same PeerId)**:
  1. Relay server replaces the old WebSocket↔PeerId mapping with the new connection
  2. Previous WebSocket connection for that PeerId is closed by the server
  3. Queued messages for the PeerId are delivered via the new connection
- **7a. Send fails due to WebSocket connection drop mid-send**:
  1. RelayTransport detects the broken connection
  2. RelayTransport marks `is_connected()` as false
  3. Returns `TransportError::ConnectionClosed`
  4. HybridTransport queues the message for later delivery
- **7b. Payload exceeds relay server's maximum message size (64KB, matching protocol limit)**:
  1. Relay server drops the message and sends an error frame
  2. RelayTransport returns `TransportError::Io` with "payload too large"
  3. Use case fails (caller should not be sending oversized messages)
- **8a. Recipient is not connected to the relay (offline)**:
  1. Relay server stores the payload in an in-memory queue for the Recipient's PeerId (max 1000 messages, FIFO eviction)
  2. Relay server sends a `Queued` acknowledgment back to Sender's RelayTransport
  3. When Recipient connects and registers, relay server drains the queue and delivers all stored messages
- **8b. Recipient's PeerId is unknown to the relay (never registered)**:
  1. Same as 8a — relay queues the message; when the PeerId eventually registers, messages are delivered
- **9a. Relay-to-Recipient WebSocket connection drops during forwarding**:
  1. Relay server re-queues the undelivered payload
  2. Relay server removes the Recipient from the connection registry
  3. When Recipient reconnects, queued messages are delivered
- **10a. Recipient receives a malformed WebSocket message**:
  1. RelayTransport logs the error and drops the malformed frame
  2. Continues waiting for the next valid message (does not disconnect)
- **11a. Sender's PeerId in the relayed message is spoofed**:
  1. The relay attaches the Sender's registered PeerId to the forwarded payload (server-side enforcement)
  2. Recipient's RelayTransport uses the server-attested PeerId, not a self-reported one
  3. Additional authentication happens via Noise session (UC-005) at the application layer
- **12a. Relay connection drops after successful message delivery but before ack returns**:
  1. Sender does not receive delivery ack
  2. Sender may retry (duplicate detection at Recipient handles this — UC-002 ext 6d)

## Variations
- **4a-alt.** Relay URL may use `ws://` (plaintext WebSocket) for local/dev or `wss://` (TLS WebSocket) for production — RelayTransport handles both
- **5a-alt.** Registration may include a capability token or API key for authenticated relay access (future extension, not in this UC)
- **8a-alt.** Relay queue TTL: queued messages expire after a configurable duration (default 1 hour) to prevent unbounded memory growth

## Agent Execution Notes
- **Verification Command**: `cargo test --test relay_fallback`
- **Test File**: `tests/integration/relay_fallback.rs`
- **Depends On**: UC-003 (Establish P2P Connection — P2P must exist as the preferred transport in HybridTransport)
- **Blocks**: UC-006 (Create Room — rooms need reliable delivery via relay for multi-peer coordination)
- **Estimated Complexity**: XL / ~4000 tokens per agent turn
- **Agent Assignment**: Lead + Teammate:Builder (relay server + relay client can parallelize)

## Implementation Notes
- **Server crate**: `termchat-relay` — axum WebSocket server with peer registry and store-and-forward queue
- **New dependencies (server)**: `axum`, `axum-extra` (ws), `tokio-tungstenite`, `futures-util`
- **New dependencies (client)**: `tokio-tungstenite`, `futures-util`
- **Relay wire protocol**: A bincode-encoded enum over WebSocket binary frames (bincode for consistency with `termchat-proto` codec; both server and client are Rust so no cross-language concern):
  ```rust
  enum RelayMessage {
      Register { peer_id: String },
      Registered { peer_id: String },
      RelayPayload { from: String, to: String, payload: Vec<u8> },
      Queued { to: String, count: u32 },
      Error { reason: String },
  }
  ```
- **New files**:
  - `termchat-relay/src/main.rs` — axum server entry point with WebSocket upgrade
  - `termchat-relay/src/relay.rs` — RelayState, peer registry, message queuing
  - `termchat-relay/src/store.rs` — In-memory message store with per-peer FIFO queues
  - `termchat/src/transport/relay.rs` — `RelayTransport` implementing `Transport` trait
- **Existing integration point**: `HybridTransport<QuicTransport, RelayTransport>` — relay plugs in as the fallback `F` type parameter
- **Message framing**: WebSocket binary frames (no length prefix needed — WebSocket is message-oriented unlike QUIC streams)
- **PeerId routing**: Relay server maintains a `HashMap<String, WebSocketSender>` mapping PeerIds to active WebSocket connections. Undeliverable messages go to a per-peer `VecDeque` (capped at 1000).

## Acceptance Criteria (for grading)
- [ ] All success postconditions verified by automated test
- [ ] All extension paths have explicit handling
- [ ] No invariant violations detected
- [ ] Code passes lint + clippy (`cargo clippy -- -D warnings`)
- [ ] Reviewer agent approves
- [ ] Relay server starts and accepts WebSocket connections
- [ ] RelayTransport implements the `Transport` trait fully (send, recv, is_connected, transport_type)
- [ ] `transport_type()` returns `TransportType::Relay`
- [ ] Message round-trips through the relay: Sender → Relay → Recipient
- [ ] Store-and-forward: message sent while Recipient offline is delivered when Recipient connects
- [ ] Relay never sees plaintext — only encrypted opaque bytes are forwarded
- [ ] HybridTransport falls back to relay when P2P is unavailable
- [ ] Multiple peers can register and exchange messages through the same relay concurrently
- [ ] WebSocket disconnect is detected by `is_connected()` returning `false`
- [ ] Send/recv after disconnect return appropriate `TransportError`
- [ ] Message ordering is preserved per-peer through the relay (FIFO)
- [ ] Relay message queue has a size cap (1000) with FIFO eviction
- [ ] Integration test passes: `cargo test --test relay_fallback`
