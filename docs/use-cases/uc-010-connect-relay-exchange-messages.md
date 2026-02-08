# Use Case: UC-010 Connect to Relay and Exchange Live Messages

## Classification
- **Goal Level**: 🌊 User Goal
- **Scope**: System (black box)
- **Priority**: P0 Critical
- **Complexity**: 🟡 Medium

## Actors
- **Primary Actor**: Terminal User (person launching TermChat to chat)
- **Supporting Actors**: Relay Server (termchat-relay, store-and-forward), Remote Peer (another TermChat instance), Transport Layer, Crypto Layer (stub for MVP)
- **Stakeholders & Interests**:
  - Terminal User: wants to launch the TUI and exchange messages with another person in real-time, seeing messages appear as they're sent
  - Remote Peer: wants the same experience from the other end
  - System: messages must flow through the existing encrypted pipeline (even if stub crypto for now), relay never sees plaintext

## Conditions
- **Preconditions** (must be true before starting):
  1. Relay server is running and reachable (e.g., `cargo run --bin termchat-relay`)
  2. User knows their peer ID and the remote peer's ID (env vars for MVP)
  3. TUI is running and responsive (from Phase 1)
  4. ChatManager, RelayTransport, and StubNoiseSession are implemented and tested (UC-001 through UC-005)
- **Success Postconditions** (true when done right):
  1. TUI connects to relay server on startup and shows "Connected via Relay" system message
  2. User can type a message and press Enter; the message is sent through RelayTransport to the remote peer
  3. When remote peer sends a message, it appears in the local chat panel with their peer ID as sender
  4. Message delivery status updates from Sent → Delivered when ack arrives
  5. `cargo run` with no env vars still works as offline demo mode (backwards compatible)
  6. The main event loop is non-blocking (poll-based, ~20 FPS) so the UI stays responsive while networking runs in background
- **Failure Postconditions** (true when it fails gracefully):
  1. If relay is unreachable on startup, TUI launches in offline demo mode with a "Could not connect to relay" system message
  2. If connection drops mid-session, a "Disconnected from relay" system message appears; typed messages show Failed status
  3. If a received message fails to decrypt/decode, it is logged and dropped; UI continues normally
- **Invariants** (must remain true throughout):
  1. Messages are encrypted before transmission (StubNoiseSession XOR "encryption" for MVP)
  2. The TUI event loop never blocks for more than 50ms (poll timeout)
  3. No modifications to app.rs — all integration happens in main.rs and the new net.rs module
  4. No modifications to ui/* — display uses existing App fields

## Main Success Scenario
1. User sets environment variables (RELAY_URL, PEER_ID, REMOTE_PEER) and runs `cargo run`
2. System parses configuration from env vars (with sensible defaults)
3. System sets up the terminal (raw mode, alternate screen) — same as current
4. System calls `net::spawn_net(config)` which:
   a. Connects to relay via `RelayTransport::connect()`
   b. Registers the local peer ID with the relay
   c. Creates a `ChatManager` with `StubNoiseSession` and the relay transport
   d. Spawns background tokio tasks for receiving, event forwarding, and command handling
   e. Returns `(cmd_tx, evt_rx)` channel pair
5. System enters the poll-based event loop:
   a. Draw the UI frame
   b. Drain all pending `NetEvent`s from evt_rx (non-blocking)
   c. Poll for terminal input events (50ms timeout)
   d. Handle key events via `app.handle_key_event()`
6. User types a message and presses Enter
7. System intercepts the Enter key (before `handle_key_event`): sends `NetCommand::SendMessage` through cmd_tx
8. `app.handle_key_event()` adds the message to local display with Sent status (existing behavior)
9. Background command handler calls `chat_mgr.send_message()` which encrypts and sends via relay
10. Remote peer's relay transport receives the encrypted payload
11. Remote peer's background receive loop calls `chat_mgr.receive_one()`, which decrypts and emits `ChatEvent::MessageReceived`
12. Event forwarder maps this to `NetEvent::MessageReceived` on the remote peer's evt_rx
13. Remote peer's event loop drains the event: pushes a DisplayMessage to `app.messages`
14. Remote peer's next draw cycle renders the new message in the chat panel
15. Relay delivers the delivery ack back to the sender
16. Sender's event loop receives `NetEvent::StatusChanged { delivered: true }`
17. Sender updates the message status from Sent → Delivered in the UI

## Extensions (What Can Go Wrong)
- **4a. Relay server is unreachable (connection timeout or refused)**:
  1. `net::spawn_net()` returns an error
  2. System logs the error and falls back to offline demo mode
  3. System shows "Could not connect to relay — running in offline mode" as system message
  4. All UI features work normally; sent messages get Failed status
- **4b. Relay rejects registration (duplicate peer ID)**:
  1. System shows "Registration failed: peer ID already in use" system message
  2. Falls back to offline demo mode
- **7a. cmd_tx channel is full (back-pressure)**:
  1. Use `try_send()` — if full, show "Message queued, network busy" as system message
  2. Message still appears locally with Sending status
- **9a. Encryption fails (StubNoiseSession error)**:
  1. ChatManager returns SendError
  2. Background handler sends NetEvent::Error
  3. Event loop shows "Send failed: encryption error" and marks message as Failed
- **10a. Remote peer is not connected to relay**:
  1. Relay queues the message (store-and-forward, up to 1000 messages per peer)
  2. Sender receives Queued status (not Delivered)
  3. Message stays at Sent in the UI
- **11a. Received payload is too large**:
  1. ChatManager's receive_one() rejects it (existing MAX_ENCRYPTED_PAYLOAD_SIZE check)
  2. Background receive loop sends NetEvent::Error, continues listening
- **11b. Decryption/decode fails on received message**:
  1. ChatManager returns error
  2. Background receive loop sends NetEvent::Error, continues listening
  3. No message appears in UI
- **5b. Event loop encounters terminal I/O error**:
  1. Loop exits, terminal is restored, error printed to stderr
  2. Background tasks are dropped (tokio runtime shuts down)

## Variations
- **1a.** User may omit REMOTE_PEER — system connects to relay but only listens (no outbound destination); useful for testing connectivity
- **1b.** User may omit all env vars — pure offline demo mode (current behavior)
- **6a.** User may send slash commands (/task, /invite-agent) — these are handled by App::handle_command() as before, NOT sent to network

## Out of Scope
- **Room management**: This UC connects two peers directly; room join/create flows are not wired to the TUI yet
- **Real Noise XX handshake**: Uses StubNoiseSession; real key exchange is deferred
- **Peer discovery**: User must manually specify REMOTE_PEER; no relay-based discovery
- **Reconnection logic**: If relay drops, no automatic reconnect
- **Multiple peers**: One-to-one chat only; multi-peer requires room wiring
- **CLI args via clap**: Env vars only for MVP; proper CLI parsing is Phase 8
- **Persistent history**: In-memory only; SQLite is Phase 8

## Agent Execution Notes
- **Verification Command**: `cargo test --test tui_net_wiring`
- **Test File**: `tests/integration/tui_net_wiring.rs`
- **Depends On**: UC-001 (Send pipeline), UC-002 (Receive pipeline), UC-004 (Relay transport), UC-005 (Crypto session)
- **Blocks**: Future UCs that wire rooms, real Noise, reconnection to the TUI
- **Estimated Complexity**: M / ~2000 tokens per agent turn
- **Agent Assignment**: Single builder (small file set — 3 files)

## Acceptance Criteria (for grading)
- [ ] All success postconditions verified by automated test
- [ ] All extension paths have explicit handling
- [ ] No invariant violations detected
- [ ] Code passes `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
- [ ] Reviewer agent approves
- [ ] Two TUI instances exchange messages via relay in manual test
- [ ] `cargo run` with no env vars still works as offline demo (backwards compatible)
- [ ] Poll-based loop maintains ~20 FPS responsiveness
- [ ] Connection failure falls back gracefully to offline mode
- [ ] Received messages appear with correct sender and timestamp
- [ ] Delivery status transitions: Sending → Sent → Delivered
