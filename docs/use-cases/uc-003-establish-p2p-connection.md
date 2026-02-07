# Use Case: UC-003 Establish P2P Connection

## Classification
- **Goal Level**: 🌊 User Goal
- **Scope**: System (black box)
- **Priority**: P1 High
- **Complexity**: 🔴 High

## Actors
- **Primary Actor**: Initiator (terminal user's TermChat client that dials the connection)
- **Supporting Actors**: Responder (the other peer's TermChat client that accepts the connection), QUIC Runtime (quinn library), TLS Layer (rustls, required by QUIC), Operating System (socket binding, network interfaces)
- **Stakeholders & Interests**:
  - Initiator: establish a reliable, low-latency connection to the peer for real-time messaging
  - Responder: accept incoming connections from authorized peers without exposing unnecessary attack surface
  - System: P2P transport satisfies the existing `Transport` trait so the rest of the pipeline (crypto, chat) works unchanged
  - Network: connections use QUIC/UDP for efficiency and future NAT traversal compatibility

## Conditions
- **Preconditions** (must be true before starting):
  1. Initiator knows the Responder's socket address (IP:port) — discovery is out of scope for this UC
  2. Responder is listening for incoming QUIC connections on the known address
  3. Both peers have the `quinn` QUIC runtime initialized with a TLS configuration (self-signed certs)
  4. No firewall or NAT blocks direct UDP connectivity between the peers (NAT traversal is out of scope for this UC)
- **Success Postconditions** (true when done right):
  1. A bidirectional QUIC connection is established between Initiator and Responder
  2. Both peers can send and receive opaque byte payloads through the connection
  3. The connection satisfies the existing `Transport` trait (send, recv, is_connected, transport_type)
  4. `transport_type()` returns `TransportType::P2p`
  5. The QUIC connection uses a single bidirectional stream per connection for message exchange (multiplexed multi-stream is a latent QUIC capability but not exercised in this UC)
  6. The QUIC connection uses a self-signed TLS certificate (the real authentication happens via Noise in UC-005, QUIC TLS is for transport encryption only)
  7. Connection establishment completes within a configurable timeout (default 10s)
- **Failure Postconditions** (true when it fails gracefully):
  1. Initiator receives a clear error (via `TransportError`) indicating why the connection failed
  2. No partial connection state leaks (all QUIC state is cleaned up)
  3. The Initiator's QUIC endpoint remains usable for future connection attempts
  4. Failed connection attempts are logged with the peer address and error reason
- **Invariants** (must remain true throughout):
  1. Payloads passed through the P2P transport are treated as opaque bytes — the transport never inspects or modifies content
  2. The `Transport` trait contract is fully satisfied (same guarantees as `LoopbackTransport`)
  3. Connection teardown (either side dropping) is detectable via `is_connected()` returning `false`
  4. The QUIC endpoint can handle multiple concurrent connections to different peers

## Main Success Scenario
1. Responder creates a `QuicListener` that binds a QUIC endpoint to a local socket address (e.g., `0.0.0.0:0` or a configured port)
2. Responder calls `QuicListener::accept()` which awaits incoming QUIC connections
3. Initiator creates a QUIC endpoint bound to a local socket address
4. Initiator dials the Responder's known address via `quinn::Endpoint::connect()`, providing the Responder's `PeerId`
5. QUIC handshake completes (TLS 1.3 with self-signed certificates, certificate verification skipped since Noise provides real auth)
6. Initiator opens a bidirectional QUIC stream for message exchange
7. Responder's `accept()` returns a new `QuicTransport` wrapping the accepted connection and stream
8. Initiator wraps its side of the connection in a `QuicTransport` struct
9. Both peers now hold a `QuicTransport` implementing `Transport` — connection is established

## Extensions (What Can Go Wrong)
- **1a. Responder cannot bind to the requested port (port in use)**:
  1. System returns `TransportError::Io` with the underlying `AddrInUse` error
  2. Responder may retry with a different port or port 0 (OS-assigned)
  3. Use case fails if no port is available
- **1b. Responder cannot create QUIC endpoint (TLS config error)**:
  1. System returns `TransportError::Io` wrapping the rustls/quinn config error
  2. This indicates a programming error (bad cert generation) — logged at error level
  3. Use case fails
- **2a. Responder's accept is interrupted by application shutdown**:
  1. The `accept()` future is cancelled (dropped)
  2. QUIC endpoint is closed gracefully, rejecting any in-flight connections
  3. Use case fails cleanly; no partial state
- **3a. Initiator's local endpoint cannot bind (no network interface)**:
  1. System returns `TransportError::Io` with the binding error
  2. Use case fails
- **4a. Initiator cannot reach the Responder's address (network unreachable)**:
  1. `quinn::Endpoint::connect()` returns a connection error
  2. System maps this to `TransportError::Unreachable(peer_id)`
  3. Use case fails; caller (UC-001) may fall back to relay (UC-004)
- **5a. QUIC handshake times out (Responder unreachable or firewalled)**:
  1. `quinn::Connecting` future times out after configurable duration (default 10s)
  2. System returns `TransportError::Timeout`
  3. Use case fails; caller may retry or fall back to relay
- **5b. QUIC handshake fails (TLS error, protocol mismatch)**:
  1. quinn returns a `ConnectionError`
  2. System maps to `TransportError::Io` with descriptive message
  3. All partial state is cleaned up
  4. Use case fails
- **6a. Stream open fails (connection dropped between handshake and stream open)**:
  1. System detects connection loss
  2. Returns `TransportError::ConnectionClosed`
  3. Use case fails; all state cleaned up
- **7a. Responder's accept detects connection but stream negotiation fails**:
  1. Responder rejects the connection (max streams exceeded, stream accept error)
  2. Initiator receives stream rejection mapped to `TransportError::ConnectionClosed`
  3. Use case fails
- **8a. PeerId mismatch — Initiator provided a PeerId that doesn't match the connected peer**:
  1. `QuicTransport` stores the PeerId provided at connection time
  2. Subsequent `send(wrong_peer, _)` calls return `TransportError::Unreachable(wrong_peer)`
  3. PeerId validation follows the same pattern as `LoopbackTransport`
- **9a. Connection drops after establishment (network change, peer crash)**:
  1. QUIC detects connection loss via keep-alive timeout
  2. `is_connected()` returns `false`
  3. Subsequent send/recv return `TransportError::ConnectionClosed`
  4. Caller handles reconnection or fallback

## Variations
- **1a.** Responder may bind to `0.0.0.0:0` for OS-assigned port — port number returned for out-of-band sharing
- **3a.** Initiator may reuse an existing QUIC endpoint for multiple outgoing connections to different peers

## Agent Execution Notes
- **Verification Command**: `cargo test --test p2p_connection`
- **Test File**: `tests/integration/p2p_connection.rs`
- **Depends On**: None (foundational transport — the Noise handshake from UC-005 runs *over* this transport, not the reverse)
- **Blocks**: UC-004 (Relay Fallback — needs P2P to exist for the HybridTransport preferred slot)
- **Estimated Complexity**: L / ~3000 tokens per agent turn
- **Agent Assignment**: Teammate:Builder (transport specialist)

## Implementation Notes
- **Crate**: `quinn` (QUIC implementation for Rust, built on `rustls`)
- **New dependency**: `quinn = "0.11"` and `rustls = { version = "0.23", features = ["ring"] }` in `termchat/Cargo.toml`
- **New files**:
  - `termchat/src/transport/quic.rs` — `QuicTransport` (implements `Transport`) and `QuicListener` (accepts connections)
- **Existing trait**: `Transport` in `termchat/src/transport/mod.rs` — the P2P implementation must satisfy this exactly
- **TLS strategy**: Generate ephemeral self-signed certificates at startup. The QUIC TLS layer provides transport encryption, but the real peer authentication happens via the Noise XX handshake (UC-005). Configure quinn to skip certificate verification.
- **Stream strategy (design decision)**: Use a single bidirectional QUIC stream per connection. The Initiator opens it after the QUIC handshake; the Responder accepts it. Both sides split the stream into a send half and recv half. This maps cleanly to the `Transport` trait's `send()`/`recv()` methods. Multi-stream multiplexing is deferred — it's a latent QUIC capability available if needed later.
- **Message framing**: Use length-prefixed messages on the QUIC stream (consistent with `termchat-proto` codec). Each `send()` writes a 4-byte LE length prefix + payload; each `recv()` reads the length prefix then the payload bytes. This is necessary because QUIC streams are byte-oriented, not message-oriented.
- **PeerId routing**: `QuicTransport` wraps a single point-to-point QUIC connection and stores the remote `PeerId` provided at connection time. `send(peer, payload)` validates that `peer` matches the stored remote PeerId — if not, returns `TransportError::Unreachable(peer)`. This follows the same pattern as `LoopbackTransport`.
- **Accept-to-application flow**: `QuicListener` wraps a `quinn::Endpoint` and provides an `async fn accept() -> Result<QuicTransport, TransportError>` method. Each call awaits the next incoming connection, accepts the first bidirectional stream, and returns a ready-to-use `QuicTransport`. The application calls `accept()` in a loop (or spawns a background task) to handle multiple incoming peers.
- **Connection management**: `QuicTransport` holds the `quinn::Connection` (for `is_connected()` checks) and the split send/recv halves of the bidirectional stream. The send half is behind a `Mutex` for thread safety; the recv half is behind a `Mutex` for exclusive read access.
- **Concurrency**: The `quinn::Endpoint` natively supports multiple concurrent connections. Each connection produces its own `QuicTransport` instance. No shared mutable state between connections.

## Acceptance Criteria (for grading)
- [ ] All success postconditions verified by automated test
- [ ] All extension paths have explicit handling
- [ ] No invariant violations detected
- [ ] Code passes lint + clippy (`cargo clippy -- -D warnings`)
- [ ] Reviewer agent approves
- [ ] Two TermChat instances connect via QUIC on localhost
- [ ] `QuicTransport` implements the `Transport` trait fully (send, recv, is_connected, transport_type)
- [ ] `QuicListener::accept()` returns a ready-to-use `QuicTransport` for each incoming connection
- [ ] Message round-trip works through the QUIC transport (send from A, recv on B, and vice versa)
- [ ] Multiple messages preserve ordering (FIFO within a connection)
- [ ] Connection timeout is enforced (10s default)
- [ ] Connection drop is detected by `is_connected()` returning `false`
- [ ] Send/recv after disconnect return `TransportError::ConnectionClosed`
- [ ] `send(wrong_peer, _)` returns `TransportError::Unreachable` (PeerId validation)
- [ ] `QuicListener` can accept multiple concurrent connections from different peers
- [ ] `transport_type()` returns `TransportType::P2p`
- [ ] Integration test passes: `cargo test --test p2p_connection`
