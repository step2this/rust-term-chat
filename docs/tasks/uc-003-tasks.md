# Tasks for UC-003: Establish P2P Connection

Generated from use case on 2026-02-07.

## Summary
- **Total tasks**: 14
- **Implementation tasks**: 8
- **Test tasks**: 4
- **Prerequisite tasks**: 1
- **Review gate tasks**: 1
- **Critical path**: T-003-01 → T-003-02 → T-003-03 → T-003-04 → T-003-05 → T-003-06 → T-003-10 → T-003-12 → T-003-14
- **Estimated total size**: L (collectively ~800-1200 lines of implementation + tests)

## Dependency Graph

```
T-003-01 (Add quinn + rustls deps)
  ├── T-003-02 (TLS config: self-signed certs, skip verification)
  │     ├── T-003-03 (QuicListener: bind + accept)
  │     │     └── T-003-08 (Test: listener bind/accept errors)
  │     └── T-003-04 (QuicTransport: connect to peer)
  │           ├── T-003-09 (Test: connect errors + timeout)
  │           └── T-003-05 (QuicTransport: send with length-prefix framing)
  │                 └── T-003-06 (QuicTransport: recv with length-prefix framing)
  │                       ├── T-003-07 (is_connected + connection drop detection)
  │                       │     └── T-003-10 (Test: unit tests for QuicTransport)
  │                       └── T-003-10 (Test: unit tests for QuicTransport)
  │
  └── T-003-11 (Wire up quic module in transport/mod.rs)

T-003-10 ──┐
T-003-11 ──┤
            └── T-003-12 (Extension: PeerId validation + error mapping)
                  └── T-003-13 (Test: PeerId routing, error mapping)
                        └── T-003-14 (Integration test: p2p_connection)

Parallel tracks after T-003-02:
  - T-003-03 (listener) and T-003-04 (connect) can be built in parallel
  - T-003-08 and T-003-09 (error tests) can run in parallel after their parents
  - T-003-11 (wire up module) can start any time after T-003-01
```

## Tasks

### T-003-01: Add quinn and rustls dependencies
- **Type**: Prerequisite
- **Module**: `termchat/Cargo.toml`
- **Description**: Add `quinn = "0.11"` and `rustls = { version = "0.23", features = ["ring"] }` as dependencies. Also add `rcgen = "0.13"` for self-signed certificate generation. Verify the workspace compiles with the new dependencies. Create the empty `termchat/src/transport/quic.rs` file with a module-level doc comment.
- **From**: Precondition 3 (QUIC runtime initialized)
- **Depends On**: None
- **Blocks**: T-003-02, T-003-11
- **Size**: S
- **Risk**: Medium (quinn version compatibility with rustls — verify the quinn 0.11 + rustls 0.23 pairing works)
- **Agent Assignment**: Lead (Cargo.toml is lead-owned)
- **Acceptance Test**: `cargo build` succeeds with quinn/rustls in the dependency tree

---

### T-003-02: Implement TLS configuration for QUIC
- **Type**: Implementation
- **Module**: `termchat/src/transport/quic.rs`
- **Description**: Create helper functions for QUIC TLS configuration:
  - `fn generate_self_signed_cert() -> Result<(rustls::pki_types::CertificateDer, rustls::pki_types::PrivateKeyDer), TransportError>` — uses `rcgen` to generate an ephemeral self-signed X.509 certificate and private key at startup.
  - `fn make_server_config(cert, key) -> Result<quinn::ServerConfig, TransportError>` — builds a quinn `ServerConfig` with the self-signed cert.
  - `fn make_client_config() -> Result<quinn::ClientConfig, TransportError>` — builds a quinn `ClientConfig` that skips certificate verification (since Noise provides real authentication in UC-005). Implement a custom `rustls::client::danger::ServerCertVerifier` that accepts all certificates.
  - Document clearly: "QUIC TLS is for transport encryption only. Peer authentication uses Noise XX (UC-005)."
- **From**: MSS Step 5 (TLS handshake), Precondition 3
- **Depends On**: T-003-01
- **Blocks**: T-003-03, T-003-04
- **Size**: M
- **Risk**: High (quinn + rustls API surface is complex; less-common crate API — research needed)
- **Agent Assignment**: Teammate:Builder-Infra
- **Acceptance Test**: Unit test: `generate_self_signed_cert()` produces a valid cert; `make_server_config()` and `make_client_config()` return `Ok`

---

### T-003-03: Implement QuicListener (bind + accept)
- **Type**: Implementation
- **Module**: `termchat/src/transport/quic.rs`
- **Description**: Implement the server side of the QUIC connection:
  - `QuicListener` struct wrapping a `quinn::Endpoint` configured as server.
  - `pub async fn bind(addr: SocketAddr) -> Result<Self, TransportError>` — creates a QUIC endpoint bound to the given address using server TLS config from T-003-02. Returns the `QuicListener`.
  - `pub fn local_addr(&self) -> Result<SocketAddr, TransportError>` — returns the bound local address (useful when binding to port 0).
  - `pub async fn accept(&self) -> Result<QuicTransport, TransportError>` — awaits the next incoming QUIC connection. On connection, accepts the first bidirectional stream. Wraps both in a `QuicTransport` and returns it. The remote `PeerId` is derived from the peer's socket address (placeholder until Noise identity is available).
  - Error mapping: quinn `ConnectionError` → `TransportError::Io`, endpoint closed → `TransportError::ConnectionClosed`.
- **From**: MSS Steps 1-2, 7 (Responder bind + accept)
- **Depends On**: T-003-02
- **Blocks**: T-003-08, T-003-10, T-003-14
- **Size**: M
- **Risk**: Medium (quinn accept API, stream acceptance)
- **Agent Assignment**: Teammate:Builder-Infra
- **Acceptance Test**: Unit test: bind to `127.0.0.1:0`, verify `local_addr()` returns valid address

---

### T-003-04: Implement QuicTransport connect (Initiator side)
- **Type**: Implementation
- **Module**: `termchat/src/transport/quic.rs`
- **Description**: Implement the client/initiator connection flow:
  - `QuicTransport` struct holding:
    - `local_id: PeerId` — local peer identity
    - `remote_id: PeerId` — remote peer identity (provided at connect time)
    - `connection: quinn::Connection` — for `is_connected()` and metadata
    - `send_stream: Mutex<quinn::SendStream>` — write half of the bidirectional stream
    - `recv_stream: Mutex<quinn::RecvStream>` — read half of the bidirectional stream
  - `pub async fn connect(endpoint: &quinn::Endpoint, addr: SocketAddr, local_id: PeerId, remote_id: PeerId) -> Result<Self, TransportError>` — dials the responder using client TLS config from T-003-02, opens one bidirectional stream, wraps it in `QuicTransport`.
  - Apply a configurable connection timeout (default 10s) using `tokio::time::timeout`.
  - Error mapping: `quinn::ConnectionError` → appropriate `TransportError` variant; timeout → `TransportError::Timeout`; unreachable → `TransportError::Unreachable`.
- **From**: MSS Steps 3-6, 8 (Initiator connect + stream open)
- **Depends On**: T-003-02
- **Blocks**: T-003-05, T-003-09, T-003-10, T-003-14
- **Size**: M
- **Risk**: Medium (quinn connect API, timeout handling)
- **Agent Assignment**: Teammate:Builder-Infra
- **Acceptance Test**: Integration test: connect to a `QuicListener` on localhost, verify connection succeeds

---

### T-003-05: Implement QuicTransport::send with length-prefix framing
- **Type**: Implementation
- **Module**: `termchat/src/transport/quic.rs`
- **Description**: Implement the `Transport::send()` method for `QuicTransport`:
  - Validate that `peer` matches `self.remote_id` — if not, return `TransportError::Unreachable(peer)`.
  - Write a 4-byte little-endian length prefix followed by the payload bytes to the QUIC send stream.
  - Lock `self.send_stream` for the duration of the write to ensure atomic message delivery.
  - Map quinn write errors to `TransportError::ConnectionClosed` or `TransportError::Io`.
  - QUIC streams are byte-oriented; length-prefix framing is necessary for message boundaries.
- **From**: MSS Step 9 (verified via send), Implementation Notes (message framing, PeerId routing)
- **Depends On**: T-003-04
- **Blocks**: T-003-06, T-003-10
- **Size**: S
- **Risk**: Low (straightforward stream write with framing)
- **Agent Assignment**: Teammate:Builder-Infra
- **Acceptance Test**: Covered by T-003-10 (unit test: send bytes, verify framing on wire)

---

### T-003-06: Implement QuicTransport::recv with length-prefix framing
- **Type**: Implementation
- **Module**: `termchat/src/transport/quic.rs`
- **Description**: Implement the `Transport::recv()` method for `QuicTransport`:
  - Lock `self.recv_stream` and read exactly 4 bytes for the length prefix.
  - Parse the length as `u32` little-endian.
  - Read exactly `length` bytes of payload from the stream.
  - Return `(self.remote_id.clone(), payload)`.
  - Map quinn read errors: stream closed → `TransportError::ConnectionClosed`; unexpected EOF (partial frame) → `TransportError::ConnectionClosed`.
  - If the length prefix indicates a payload > 64KB, return an error (defense against malicious/corrupted peers).
- **From**: MSS Step 9 (verified via recv), Implementation Notes (message framing)
- **Depends On**: T-003-05
- **Blocks**: T-003-07, T-003-10
- **Size**: S
- **Risk**: Low (straightforward stream read with framing)
- **Agent Assignment**: Teammate:Builder-Infra
- **Acceptance Test**: Covered by T-003-10 (unit test: full send/recv round-trip)

---

### T-003-07: Implement is_connected and connection drop detection
- **Type**: Implementation
- **Module**: `termchat/src/transport/quic.rs`
- **Description**: Complete the `Transport` trait implementation:
  - `fn is_connected(&self, peer: &PeerId) -> bool` — returns `true` if `peer` matches `self.remote_id` AND the underlying `quinn::Connection` has not been closed. Use `connection.close_reason()` to check: `None` means still open.
  - `fn transport_type(&self) -> TransportType` — returns `TransportType::P2p`.
  - Ensure that when the QUIC connection is lost (remote crash, network change), `is_connected()` reflects this. Quinn uses keep-alives (configurable) for detection.
  - Enable QUIC keep-alive in the transport config (e.g., 15s interval) so stale connections are detected within a reasonable window.
- **From**: Extension 9a (connection drop), Invariant 3 (teardown detectable), Postcondition 4
- **Depends On**: T-003-06
- **Blocks**: T-003-10
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder-Infra
- **Acceptance Test**: Covered by T-003-10 (unit test: drop one side, verify is_connected returns false)

---

### T-003-08: Test — QuicListener bind and accept errors
- **Type**: Test
- **Module**: `termchat/src/transport/quic.rs` (inline `#[cfg(test)]`)
- **Description**: Unit tests for QuicListener error paths:
  - `bind()` to a valid address succeeds, `local_addr()` returns valid port
  - `bind()` to an address already in use returns `TransportError::Io`
  - `accept()` returns `QuicTransport` when a client connects
  - `accept()` on a closed endpoint returns `TransportError::ConnectionClosed`
- **From**: Extensions 1a, 1b, 2a
- **Depends On**: T-003-03
- **Blocks**: None
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder-Infra
- **Acceptance Test**: `cargo test -p termchat -- quic::tests` — listener tests pass

---

### T-003-09: Test — QuicTransport connect errors and timeout
- **Type**: Test
- **Module**: `termchat/src/transport/quic.rs` (inline `#[cfg(test)]`)
- **Description**: Unit tests for Initiator connect error paths:
  - Connect to a listening peer succeeds
  - Connect to a non-listening address returns error (mapped to `Unreachable` or `Io`)
  - Connect with a short timeout to unreachable address returns `TransportError::Timeout`
  - Connect after endpoint closed returns error
- **From**: Extensions 4a, 5a, 5b
- **Depends On**: T-003-04
- **Blocks**: None
- **Size**: S
- **Risk**: Medium (timeout tests can be flaky if not carefully designed — use very short timeouts to unreachable addresses)
- **Agent Assignment**: Teammate:Builder-Infra
- **Acceptance Test**: `cargo test -p termchat -- quic::tests` — connect error tests pass

---

### T-003-10: Test — QuicTransport unit tests (send, recv, round-trip)
- **Type**: Test
- **Module**: `termchat/src/transport/quic.rs` (inline `#[cfg(test)]`)
- **Description**: Comprehensive unit tests for the Transport trait implementation:
  - Send/recv round-trip: send bytes from A, recv on B, verify content matches
  - Bidirectional: A→B and B→A both work
  - Multiple messages preserve FIFO order (send 10, recv 10, verify order)
  - Large payload (e.g., 32KB) round-trips correctly
  - Empty payload round-trips correctly
  - `transport_type()` returns `TransportType::P2p`
  - `is_connected()` returns `true` for remote peer, `false` for unknown peer
  - After dropping one side: `is_connected()` returns `false`, send returns `ConnectionClosed`, recv returns `ConnectionClosed`
  - `send(wrong_peer, _)` returns `TransportError::Unreachable`
- **From**: Success Postconditions 1-4, Invariants 1-3, Extension 8a (PeerId mismatch)
- **Depends On**: T-003-03, T-003-04, T-003-05, T-003-06, T-003-07
- **Blocks**: T-003-12, T-003-14
- **Size**: M
- **Risk**: Low
- **Agent Assignment**: Teammate:Reviewer
- **Acceptance Test**: `cargo test -p termchat -- quic::tests` — all transport tests pass

---

### T-003-11: Wire up quic module in transport/mod.rs
- **Type**: Implementation
- **Module**: `termchat/src/transport/mod.rs`
- **Description**: Register the new quic module:
  - Add `pub mod quic;` to `transport/mod.rs`
  - Update the module doc comment to list QUIC as an available implementation
  - Add `[[test]]` section in `termchat/Cargo.toml` for the `p2p_connection` integration test: `name = "p2p_connection"`, `path = "../tests/integration/p2p_connection.rs"`
- **From**: Infrastructure wiring
- **Depends On**: T-003-01
- **Blocks**: T-003-12
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Lead (Cargo.toml is lead-owned)
- **Acceptance Test**: `cargo build` succeeds with the new module visible

---

### T-003-12: Implement extension error mapping and TransportError additions
- **Type**: Implementation
- **Module**: `termchat/src/transport/mod.rs`, `termchat/src/transport/quic.rs`
- **Description**: Review and complete error mapping for all extension paths:
  - Verify all quinn error types map cleanly to existing `TransportError` variants
  - If any gaps exist, add new `TransportError` variants (e.g., `ConfigError(String)` for TLS setup failures if `Io` doesn't capture it well)
  - Ensure all error paths log appropriately (using `tracing::warn!` or `tracing::error!`)
  - Review that all quinn `ConnectionError`, `WriteError`, `ReadError`, and `ReadExactError` variants are handled
  - Add `#[non_exhaustive]` to `TransportError` if not already present (future-proofing)
- **From**: All extensions, Failure Postconditions 1-4
- **Depends On**: T-003-10, T-003-11
- **Blocks**: T-003-13
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder-Infra
- **Acceptance Test**: All error paths compile and are covered by tests in T-003-08, T-003-09, T-003-10

---

### T-003-13: Test — PeerId routing and comprehensive error mapping
- **Type**: Test
- **Module**: `termchat/src/transport/quic.rs` (inline `#[cfg(test)]`)
- **Description**: Targeted tests for extension error paths not covered by T-003-10:
  - PeerId validation: `send(wrong_peer, _)` returns `Unreachable` (Extension 8a)
  - Stream open failure: verify error mapping (Extension 6a) — may need mock or forced disconnect
  - Verify log output contains peer address and error reason (Failure Postcondition 4) — use `tracing-subscriber` test capture
- **From**: Extensions 6a, 7a, 8a; Failure Postcondition 4
- **Depends On**: T-003-12
- **Blocks**: T-003-14
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Teammate:Reviewer
- **Acceptance Test**: `cargo test -p termchat -- quic::tests` — error mapping tests pass

---

### T-003-14: Integration test — p2p_connection end-to-end
- **Type**: Test
- **Module**: `tests/integration/p2p_connection.rs`
- **Description**: The primary integration test for UC-003 — validates all success postconditions:
  1. Create a `QuicListener` bound to `127.0.0.1:0`
  2. Spawn an accept task in the background
  3. Create an initiator `QuicTransport` via `connect()` to the listener's address
  4. Accept task completes, producing a responder `QuicTransport`
  5. Send a message from initiator → responder, verify it arrives intact
  6. Send a message from responder → initiator, verify bidirectional works
  7. Send 100 messages, verify FIFO ordering preserved
  8. Verify `transport_type()` returns `TransportType::P2p` on both sides
  9. Verify `is_connected()` returns `true` on both sides
  10. Drop initiator, verify responder's `is_connected()` returns `false`
  11. Verify responder's `recv()` returns `TransportError::ConnectionClosed`
  12. Test connection timeout: connect to non-listening address with 1s timeout, verify `TransportError::Timeout`
  13. Test multiple concurrent connections: one listener accepts 2 different initiators

  This is the **verification command** test: `cargo test --test p2p_connection`
- **From**: All Success Postconditions, All Invariants
- **Depends On**: T-003-10, T-003-12, T-003-13
- **Blocks**: None
- **Size**: M
- **Risk**: Medium (end-to-end QUIC on localhost; port allocation, async coordination)
- **Agent Assignment**: Teammate:Reviewer
- **Acceptance Test**: `cargo test --test p2p_connection` passes

---

## Implementation Order

Topologically sorted, with parallel opportunities noted:

| Order | Task | Type | Size | Depends On | Parallel Group |
|-------|------|------|------|------------|----------------|
| 1a | T-003-01: Add quinn + rustls deps | Prerequisite | S | none | A |
| 2a | T-003-02: TLS config (self-signed certs) | Implementation | M | T-003-01 | — |
| 2b | T-003-11: Wire up quic module | Implementation | S | T-003-01 | A (parallel with T-003-02) |
| 3a | T-003-03: QuicListener (bind + accept) | Implementation | M | T-003-02 | B |
| 3b | T-003-04: QuicTransport connect | Implementation | M | T-003-02 | B |
| 4a | T-003-05: QuicTransport::send | Implementation | S | T-003-04 | — |
| 4b | T-003-08: Test — listener errors | Test | S | T-003-03 | C (parallel with 4a) |
| 5a | T-003-06: QuicTransport::recv | Implementation | S | T-003-05 | — |
| 5b | T-003-09: Test — connect errors | Test | S | T-003-04 | C (parallel with 5a) |
| 6 | T-003-07: is_connected + drop detection | Implementation | S | T-003-06 | — |
| 7 | T-003-10: Unit tests (send/recv/round-trip) | Test | M | T-003-03-07 | — |
| 8 | T-003-12: Error mapping review + additions | Implementation | S | T-003-10, T-003-11 | — |
| 9 | T-003-13: Test — PeerId + error mapping | Test | S | T-003-12 | — |
| 10 | T-003-14: Integration test (p2p_connection) | Test | M | T-003-10, T-003-12, T-003-13 | — |

## Notes for Agent Team

- **Lead-owned files**: `Cargo.toml` (T-003-01) and `transport/mod.rs` wiring (T-003-11) are lead-owned per CLAUDE.md. The lead should complete these early to unblock builders.
- **Single builder can own all of quic.rs**: Unlike UC-001 which had multiple modules, UC-003 is concentrated in one new file (`transport/quic.rs`). A single Builder-Infra agent can own T-003-02 through T-003-07 sequentially without conflict.
- **Reviewer writes T-003-10, T-003-13, T-003-14**: The comprehensive tests and integration test should be written by the Reviewer agent (blind testing against postconditions, not implementation). The builder writes the error-path unit tests (T-003-08, T-003-09) since they're tightly coupled to the implementation.
- **Review gate after T-003-07**: Before proceeding to error mapping and integration tests, the Reviewer should verify that the basic Transport trait contract works (send/recv/is_connected). This is the T-003-10 checkpoint.
- **quinn API research**: T-003-02 is the highest-risk task. The builder should research the quinn 0.11 + rustls 0.23 API before writing code. Key areas: `Endpoint::server()` vs `Endpoint::new()`, `ServerConfig` construction, custom `ServerCertVerifier` for skip-verification, bidirectional stream API (`open_bi()` / `accept_bi()`).
- **Port allocation in tests**: All tests should use `127.0.0.1:0` (OS-assigned port) to avoid port conflicts in CI. Use `listener.local_addr()` to get the actual port for the initiator.
- **Timeout tests**: Use short timeouts (1-2s) against unreachable addresses (e.g., `192.0.2.1:1` from TEST-NET) rather than against localhost, to avoid false failures.
- **Keep agent tasks small**: Per retrospective, each agent task should be ~15-20 tool calls max. The tasks above are scoped accordingly — the largest (T-003-10, T-003-14) are test-writing tasks that produce well-bounded output.
