# Tasks for UC-001: Send Direct Message

Generated from use case on 2026-02-07.

## Summary
- **Total tasks**: 18
- **Implementation tasks**: 12
- **Test tasks**: 5
- **Prerequisite tasks**: 1
- **Critical path**: T-001-01 → T-001-02 → T-001-03 → T-001-04 → T-001-05 → T-001-06 → T-001-07 → T-001-08 → T-001-09 → T-001-10 → T-001-17
- **Estimated total size**: L (collectively ~1500-2500 lines of implementation + tests)

## Dependency Graph

```
T-001-01 (Workspace Init)
  ├── T-001-02 (Wire Format Types)
  │     ├── T-001-03 (Message Validation)
  │     │     └── T-001-14 (Ext: Empty/Oversize)
  │     ├── T-001-04 (Message Serialization)
  │     │     ├── T-001-15 (Ext: Serialization Failure)
  │     │     └── T-001-18 (Test: Serialization Round-Trip)
  │     └── T-001-05 (Crypto: Encrypt/Decrypt Pipeline)
  │           ├── T-001-16 (Ext: No Noise Session)
  │           └── T-001-06 (Transport Abstraction Trait)
  │                 ├── T-001-07 (Loopback Transport Impl)
  │                 │     └── T-001-08 (Send Pipeline: Validate→Serialize→Encrypt→Transmit)
  │                 │           ├── T-001-09 (Delivery Ack + Status Tracking)
  │                 │           │     └── T-001-10 (Local History Storage)
  │                 │           │           └── T-001-11 (Ext: Transport Fallback + Offline Queuing)
  │                 │           │                 └── T-001-12 (Ext: Retry + Timeout Logic)
  │                 │           │                       └── T-001-13 (Ext: History Write Failure)
  │                 │           └── T-001-17 (Test: Integration send_receive)
  │                 └── T-001-18 (Test: Serialization Round-Trip)
  └── T-001-14 (Ext: Empty/Oversize)

Parallel tracks after T-001-01:
  - T-001-02 (proto types) is the foundation for everything
  - T-001-03 and T-001-04 can be built in parallel after T-001-02
  - T-001-05 can start after T-001-02 (needs message types for encrypt/decrypt)
  - Extension tasks can start after their parent MSS task
```

## Tasks

### T-001-01: Initialize Cargo Workspace
- **Type**: Prerequisite
- **Module**: Root `/`
- **Description**: Create the three-crate Cargo workspace: `termchat` (binary), `termchat-relay` (binary), `termchat-proto` (library). Set up `Cargo.toml` workspace members, Rust edition 2024, and shared dependencies. Add initial dependencies: `serde`, `bincode`, `thiserror`, `tokio` (with rt, macros, net features), `tracing`. Create stub `main.rs` / `lib.rs` for each crate so the workspace compiles.
- **From**: Precondition (project must exist before any implementation)
- **Depends On**: None
- **Blocks**: All other tasks
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder
- **Acceptance Test**: `cargo build` succeeds with no errors across the workspace

---

### T-001-02: Define Wire Format Types in termchat-proto
- **Type**: Implementation
- **Module**: `termchat-proto/src/message.rs`
- **Description**: Define the core message types for the wire protocol:
  - `MessageId` (UUID v7 for time-ordering)
  - `SenderId` / `RecipientId` (public key fingerprint wrappers)
  - `ConversationId`
  - `Timestamp` (millisecond precision, UTC)
  - `MessageContent` enum (`Text(String)`, future: `File`, `Reaction`)
  - `MessageMetadata` struct (timestamp, sender_id, conversation_id, message_id)
  - `ChatMessage` struct (metadata + content)
  - `MessageStatus` enum (`Pending`, `Sent`, `Delivered`, `Failed(String)`)
  - `DeliveryAck` struct (message_id, timestamp)
  - `Envelope` enum wrapping all wire-level messages (`Chat(ChatMessage)`, `Ack(DeliveryAck)`, `Handshake(...)`)

  All types must derive `serde::Serialize`, `serde::Deserialize`, `Debug`, `Clone`. Add doc comments to all public types and fields.
- **From**: MSS Steps 3, 7 (serialization + ack types)
- **Depends On**: T-001-01
- **Blocks**: T-001-03, T-001-04, T-001-05, T-001-06, T-001-08
- **Size**: M
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder
- **Acceptance Test**: `cargo build -p termchat-proto` succeeds; types are importable from `termchat_proto::message`

---

### T-001-03: Implement Message Validation
- **Type**: Implementation
- **Module**: `termchat-proto/src/message.rs` (or `termchat-proto/src/validation.rs`)
- **Description**: Implement validation logic for outgoing messages:
  - `ChatMessage::validate(&self) -> Result<(), ValidationError>`
  - Check non-empty content (Extension 2a)
  - Check size limit: serialized payload must be ≤ 64KB (Extension 2b)
  - `ValidationError` enum with variants `Empty`, `TooLarge { size: usize, max: usize }`
  - Define the error type using `thiserror`
- **From**: MSS Step 2, Extensions 2a, 2b
- **Depends On**: T-001-02
- **Blocks**: T-001-08, T-001-14
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder
- **Acceptance Test**: Unit tests: empty message returns `Err(Empty)`, oversized message returns `Err(TooLarge)`, valid message returns `Ok(())`

---

### T-001-04: Implement Message Serialization with Bincode
- **Type**: Implementation
- **Module**: `termchat-proto/src/codec.rs`
- **Description**: Implement serialize/deserialize functions for wire messages:
  - `encode(msg: &Envelope) -> Result<Vec<u8>, CodecError>`
  - `decode(bytes: &[u8]) -> Result<Envelope, CodecError>`
  - Use `bincode` with standard configuration (little-endian, varint length)
  - `CodecError` wrapping bincode errors via `thiserror`
  - Length-prefix framing: `[u32 length][payload]` for stream-based transports
  - `encode_framed` / `decode_framed` variants that include the length prefix
- **From**: MSS Step 3, Extension 3a
- **Depends On**: T-001-02
- **Blocks**: T-001-08, T-001-15, T-001-18
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder
- **Acceptance Test**: Round-trip test: encode then decode produces identical message; `cargo test -p termchat-proto`

---

### T-001-05: Implement Encrypt/Decrypt Pipeline (Stubbed Noise)
- **Type**: Implementation
- **Module**: `termchat/src/crypto/mod.rs`, `termchat/src/crypto/noise.rs`
- **Description**: Create the crypto layer interface and a **stubbed** implementation for UC-001. The real Noise XX handshake is UC-005; for UC-001, we need the interface and a working placeholder:
  - `CryptoSession` trait with `encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError>` and `decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError>`
  - `NoiseSession` struct (stubbed: XOR with fixed key or ChaCha20 with hardcoded key for now, clearly marked as placeholder)
  - `CryptoError` enum via `thiserror`
  - **Invariant enforcement**: the encrypt/decrypt boundary is the ONLY place plaintext is handled. Document this contract.

  Note: The real Noise implementation comes in UC-005. This task establishes the interface so the send pipeline can be built end-to-end.
- **From**: MSS Step 4, Invariant 1 (plaintext never leaves app boundary)
- **Depends On**: T-001-02
- **Blocks**: T-001-08, T-001-16
- **Size**: M
- **Risk**: Medium (interface design affects UC-005 later; get the trait right)
- **Agent Assignment**: Teammate:Builder
- **Acceptance Test**: Unit test: encrypt then decrypt round-trips to original plaintext; encrypted bytes differ from plaintext

---

### T-001-06: Define Transport Abstraction Trait
- **Type**: Implementation
- **Module**: `termchat/src/transport/mod.rs`
- **Description**: Define the async transport trait that both P2P and relay will implement:
  - `Transport` trait:
    - `async fn send(&self, peer: &PeerId, payload: &[u8]) -> Result<(), TransportError>`
    - `async fn recv(&self) -> Result<(PeerId, Vec<u8>), TransportError>`
    - `fn is_connected(&self, peer: &PeerId) -> bool`
    - `fn transport_type(&self) -> TransportType` (P2P or Relay)
  - `TransportError` enum (`ConnectionClosed`, `Timeout`, `Unreachable`, `Io(io::Error)`)
  - `TransportType` enum (`P2P`, `Relay`, `Loopback`)
  - `PeerId` type (wraps public key fingerprint)

  Use `async_trait` or native async traits (Rust 2024 edition supports `async fn` in traits behind nightly; check stability — may need `async-trait` crate).
- **From**: MSS Step 5
- **Depends On**: T-001-01
- **Blocks**: T-001-07, T-001-08, T-001-11
- **Size**: S
- **Risk**: Medium (async trait design is a key architectural decision)
- **Agent Assignment**: Teammate:Builder
- **Acceptance Test**: `cargo build` with the trait defined; no implementations needed yet

---

### T-001-07: Implement Loopback Transport for Testing
- **Type**: Implementation
- **Module**: `termchat/src/transport/loopback.rs`
- **Description**: Implement the `Transport` trait using in-process `tokio::sync::mpsc` channels. This enables UC-001 integration testing without real networking:
  - `LoopbackTransport` struct wrapping `mpsc::Sender` / `mpsc::Receiver`
  - `LoopbackTransport::create_pair() -> (LoopbackTransport, LoopbackTransport)` — creates two connected ends
  - Simulates send/recv with channel operations
  - Optional: configurable latency/failure injection for testing extensions

  This is the transport used for all UC-001 testing. Real P2P (UC-003) and relay (UC-004) come later.
- **From**: Precondition 3 (transport must be available), test infrastructure
- **Depends On**: T-001-06
- **Blocks**: T-001-08, T-001-17
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder
- **Acceptance Test**: Unit test: send bytes through one end, recv from the other; round-trip succeeds

---

### T-001-08: Build the Send Pipeline (Validate → Serialize → Encrypt → Transmit)
- **Type**: Implementation
- **Module**: `termchat/src/chat/mod.rs`, `termchat/src/chat/message.rs`
- **Description**: Implement the core send flow that chains MSS steps 2-6:
  - `ChatManager` struct (or `SendPipeline`) that holds references to `CryptoSession` and `Transport`
  - `async fn send_message(&self, content: MessageContent, conversation: ConversationId) -> Result<MessageStatus, SendError>`
    1. Build `ChatMessage` with metadata (generate ID, timestamp, sender info)
    2. Validate via `ChatMessage::validate()`
    3. Serialize via `codec::encode()`
    4. Encrypt via `CryptoSession::encrypt()`
    5. Transmit via `Transport::send()`
    6. Return `MessageStatus::Sent`
  - `SendError` enum aggregating validation, codec, crypto, and transport errors

  This is the heart of UC-001 — the pipeline that the UI will call.
- **From**: MSS Steps 2-6 (the full send flow)
- **Depends On**: T-001-03, T-001-04, T-001-05, T-001-06, T-001-07
- **Blocks**: T-001-09, T-001-17
- **Size**: M
- **Risk**: Medium (orchestration of multiple components)
- **Agent Assignment**: Teammate:Builder
- **Acceptance Test**: Unit test with loopback transport: call `send_message`, verify encrypted bytes appear on the other end, decrypt and compare

---

### T-001-09: Implement Delivery Acknowledgment and Status Tracking
- **Type**: Implementation
- **Module**: `termchat/src/chat/message.rs`, `termchat/src/chat/mod.rs`
- **Description**: Implement the ack flow (MSS steps 7-9):
  - Receiving side: on receiving a `ChatMessage`, send back a `DeliveryAck`
  - Sending side: listen for `DeliveryAck` matching the sent message ID
  - Update `MessageStatus` from `Sent` → `Delivered` when ack received
  - Status tracking: `HashMap<MessageId, MessageStatus>` in `ChatManager`
  - Expose status changes via a channel or callback for UI notification
- **From**: MSS Steps 7-9
- **Depends On**: T-001-08
- **Blocks**: T-001-10, T-001-12, T-001-17
- **Size**: M
- **Risk**: Medium (async coordination between send and ack listener)
- **Agent Assignment**: Teammate:Builder
- **Acceptance Test**: Integration test: send message via loopback, verify ack received, status transitions to `Delivered`

---

### T-001-10: Implement Local History Storage
- **Type**: Implementation
- **Module**: `termchat/src/chat/history.rs`
- **Description**: Implement local message persistence (MSS step 8, postconditions 1):
  - `MessageStore` trait: `async fn save(&self, msg: &ChatMessage, status: MessageStatus) -> Result<(), StoreError>`, `async fn update_status(&self, id: &MessageId, status: MessageStatus) -> Result<(), StoreError>`, `async fn get_conversation(&self, conv: &ConversationId, limit: usize) -> Result<Vec<(ChatMessage, MessageStatus)>, StoreError>`
  - `InMemoryStore` implementation for UC-001 (SQLite comes in Sprint 5)
  - `StoreError` via `thiserror`
  - Integrate with `ChatManager`: save after send, update on ack
- **From**: MSS Step 8, Success Postcondition 1
- **Depends On**: T-001-09
- **Blocks**: T-001-11, T-001-13, T-001-17
- **Size**: M
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder
- **Acceptance Test**: Unit test: save message, retrieve it, verify content and status

---

### T-001-11: Implement Transport Fallback and Offline Queuing
- **Type**: Implementation
- **Module**: `termchat/src/transport/hybrid.rs`, `termchat/src/chat/mod.rs`
- **Description**: Handle extensions 5a (P2P fails → relay) and 5b (offline → queue):
  - `HybridTransport` struct wrapping a preferred and fallback `Transport`
  - On `send()` failure of preferred transport, automatically try fallback
  - If all transports fail, save message as `Pending` in local history
  - `PendingQueue`: stores messages to retry when transport reconnects
  - Background task: periodically check connectivity and flush pending queue
  - Expose connectivity status changes via channel
- **From**: Extensions 5a, 5b; Failure Postconditions 2, 3
- **Depends On**: T-001-06, T-001-10
- **Blocks**: T-001-12
- **Size**: M
- **Risk**: Medium (async background retry logic)
- **Agent Assignment**: Teammate:Builder
- **Acceptance Test**: Test: send with disconnected transport → message saved as pending; reconnect → message delivered

---

### T-001-12: Implement Retry and Timeout Logic
- **Type**: Implementation
- **Module**: `termchat/src/chat/mod.rs`
- **Description**: Handle extensions 6a (transmission failure mid-send) and 7a (ack timeout):
  - Retry transmission once on same transport before fallback (Extension 6a)
  - Ack timeout: 10 second deadline using `tokio::time::timeout`
  - On timeout: retry once, then mark as `Sent` (not `Delivered`) (Extension 7a)
  - Configurable timeout and retry count (default: 10s, 1 retry)
- **From**: Extensions 6a, 7a
- **Depends On**: T-001-09, T-001-11
- **Blocks**: T-001-17
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder
- **Acceptance Test**: Test with loopback transport that drops acks: verify status stays `Sent` after timeout

---

### T-001-13: Handle History Write Failure
- **Type**: Implementation
- **Module**: `termchat/src/chat/mod.rs`, `termchat/src/chat/history.rs`
- **Description**: Handle extension 8a (disk full / DB error):
  - If `MessageStore::save()` fails, log the error (don't crash)
  - Still report delivery success to the user (message was sent)
  - Queue the failed history write for retry
  - Show warning in UI: "Message delivered but could not save to history"
- **From**: Extension 8a
- **Depends On**: T-001-10
- **Blocks**: None
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder
- **Acceptance Test**: Unit test: inject store error, verify send still succeeds, warning is emitted

---

### T-001-14: Test — Message Validation (Empty and Oversize)
- **Type**: Test
- **Module**: `termchat-proto/src/message.rs` (inline `#[cfg(test)]`)
- **Description**: Write unit tests for Extension 2a and 2b:
  - Empty message text → `Err(ValidationError::Empty)`
  - Message exactly at 64KB → `Ok(())`
  - Message at 64KB + 1 byte → `Err(ValidationError::TooLarge)`
  - Normal message → `Ok(())`
  - Multiline message (Variation 1a) → `Ok(())`
- **From**: Extensions 2a, 2b; Variation 1a
- **Depends On**: T-001-03
- **Blocks**: None
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder
- **Acceptance Test**: `cargo test -p termchat-proto` — all validation tests pass

---

### T-001-15: Test — Serialization Failure Handling
- **Type**: Test
- **Module**: `termchat-proto/src/codec.rs` (inline `#[cfg(test)]`)
- **Description**: Write unit tests for Extension 3a:
  - Valid message serializes and deserializes correctly
  - Corrupted bytes fail to decode with `CodecError`
  - Truncated bytes fail to decode
  - Length-prefix framing: verify frame boundaries are correct
- **From**: Extension 3a
- **Depends On**: T-001-04
- **Blocks**: None
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder
- **Acceptance Test**: `cargo test -p termchat-proto` — all codec tests pass

---

### T-001-16: Test — No Noise Session Established
- **Type**: Test
- **Module**: `termchat/src/crypto/` (inline `#[cfg(test)]`)
- **Description**: Write unit tests for Extension 4a:
  - Attempt to encrypt with no session → triggers handshake initiation (or returns `CryptoError::NoSession`)
  - For UC-001 with the stubbed crypto: verify that the pipeline handles the case where `CryptoSession` is not yet established
  - This test will be expanded in UC-005 when real Noise is implemented
- **From**: Extension 4a
- **Depends On**: T-001-05
- **Blocks**: None
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder
- **Acceptance Test**: `cargo test -p termchat` — crypto session error tests pass

---

### T-001-17: Test — Integration Send/Receive Round-Trip
- **Type**: Test
- **Module**: `tests/integration/send_receive.rs`
- **Description**: The primary integration test for UC-001 — validates all success postconditions:
  1. Create two `ChatManager` instances connected via `LoopbackTransport`
  2. Sender sends a text message
  3. Verify: message arrives at recipient (decrypted, deserialized, content matches)
  4. Verify: sender receives delivery ack, status transitions `Sent → Delivered`
  5. Verify: message is in sender's local history with `Delivered` status
  6. Verify: encrypted bytes on the wire differ from plaintext (invariant 1)
  7. Verify: message ordering preserved when sending multiple messages (invariant 2)

  This is the **verification command** test: `cargo test --test send_receive`
- **From**: All Success Postconditions, All Invariants
- **Depends On**: T-001-08, T-001-09, T-001-10, T-001-12
- **Blocks**: None
- **Size**: M
- **Risk**: Medium (end-to-end integration; depends on everything working together)
- **Agent Assignment**: Teammate:Reviewer
- **Acceptance Test**: `cargo test --test send_receive` passes

---

### T-001-18: Test — Property-Based Serialization Round-Trip
- **Type**: Test
- **Module**: `tests/property/serialization.rs`
- **Description**: Property test using `proptest` to verify serialization invariants:
  - For any arbitrary `ChatMessage`, `encode(msg)` then `decode(bytes)` produces the original message
  - For any arbitrary `Envelope`, round-trip is lossless
  - Fuzz the decoder: random bytes should never panic (returns `Err`, not panic)

  Add `proptest` as a dev dependency.
- **From**: MSS Step 3 (serialization correctness), robustness
- **Depends On**: T-001-04
- **Blocks**: None
- **Size**: S
- **Risk**: Low
- **Agent Assignment**: Teammate:Builder
- **Acceptance Test**: `cargo test --test serialization` passes with default proptest config (256 cases)

---

## Implementation Order

Topologically sorted, with parallel opportunities noted:

| Order | Task | Type | Size | Depends On | Parallel Group |
|-------|------|------|------|------------|----------------|
| 1 | T-001-01: Initialize Cargo Workspace | Prerequisite | S | none | — |
| 2 | T-001-02: Define Wire Format Types | Implementation | M | T-001-01 | — |
| 3a | T-001-03: Message Validation | Implementation | S | T-001-02 | A |
| 3b | T-001-04: Message Serialization | Implementation | S | T-001-02 | A |
| 3c | T-001-05: Encrypt/Decrypt Pipeline | Implementation | M | T-001-02 | A |
| 3d | T-001-06: Transport Abstraction Trait | Implementation | S | T-001-01 | A |
| 4a | T-001-14: Test — Validation | Test | S | T-001-03 | B |
| 4b | T-001-15: Test — Serialization Failure | Test | S | T-001-04 | B |
| 4c | T-001-18: Test — Property Round-Trip | Test | S | T-001-04 | B |
| 4d | T-001-16: Test — No Noise Session | Test | S | T-001-05 | B |
| 4e | T-001-07: Loopback Transport | Implementation | S | T-001-06 | B |
| 5 | T-001-08: Send Pipeline | Implementation | M | T-001-03,04,05,07 | — |
| 6 | T-001-09: Delivery Ack + Status | Implementation | M | T-001-08 | — |
| 7 | T-001-10: Local History Storage | Implementation | M | T-001-09 | — |
| 8a | T-001-11: Transport Fallback + Queuing | Implementation | M | T-001-06,10 | C |
| 8b | T-001-13: History Write Failure | Implementation | S | T-001-10 | C |
| 9 | T-001-12: Retry + Timeout | Implementation | S | T-001-09,11 | — |
| 10 | T-001-17: Integration send_receive | Test | M | T-001-08,09,10,12 | — |

## Notes for Agent Team

- **Key architectural decision in T-001-06**: The `Transport` trait design affects UC-003 (P2P) and UC-004 (Relay). The trait should be general enough to accommodate both without being over-engineered. Use `async fn` in traits if edition 2024 supports it natively; otherwise use `async-trait` crate.
- **Stubbed crypto in T-001-05**: Clearly mark the stub with `// TODO: Replace with real Noise XX handshake in UC-005`. The trait interface is the important part — get that right.
- **Parallel group A** (tasks 3a-3d) can all be built simultaneously by different agents or in any order. They share no dependencies beyond T-001-02.
- **Parallel group B** (tasks 4a-4e) are independent test + implementation tasks that can run in parallel.
- **Review gate**: After T-001-08 (send pipeline), the Reviewer should verify the pipeline against MSS steps 2-6 before proceeding to ack/history tasks.
- **No UI tasks in UC-001**: The use case mentions UI rendering (MSS step 9), but the TUI is Sprint 0/Phase 1 work. UC-001 focuses on the backend pipeline. UI integration will be a separate task when the TUI exists.
- **In-memory store for now**: T-001-10 uses `InMemoryStore`, not SQLite. SQLite comes in Sprint 5 (UC-006). The `MessageStore` trait ensures this is a clean swap later.
- **Error handling convention**: All error types use `thiserror`. No `unwrap()` in any production code path. Tests may use `unwrap()` or `?` with appropriate test harness.
