# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@docs/termchat-blueprint.md

## Project State

Active development. Three-crate workspace is initialized and building. Completed: UC-001 (Send), UC-002 (Receive), UC-005 (E2E Handshake), UC-003 (P2P Connection), UC-004 (Relay Fallback), UC-006 (Create Room), UC-007 (Agent Join), UC-008 (Share Task List), UC-009 (Typing & Presence), UC-010 (Live Relay Messaging), UC-012 (Polish for Ship), Phase 1 (Hello Ratatui TUI). Phase 7 (Task Coordination) complete. Sprint 8 (Polish & Ship) complete: clap CLI args, TOML config file, tracing-to-file logging, theme enhancement, GitHub Actions CI, package metadata.

## Build & Development Commands

```bash
cargo build
cargo run                                # launch TUI client (offline demo mode)
cargo run -- --help                      # show CLI options
cargo run -- --relay-url ws://127.0.0.1:9000/ws --peer-id alice --remote-peer bob
cargo run --bin termchat-relay           # relay server (ws://0.0.0.0:9000)
cargo run --bin termchat-relay -- --help # show relay CLI options
cargo test                               # all tests
cargo test --test send_receive           # UC-001/UC-002 integration test
cargo test --test e2e_encryption         # UC-005 integration test
cargo test --test p2p_connection         # UC-003 integration test
cargo test --test relay_fallback         # UC-004 integration test
cargo test --test room_management        # UC-006 integration test
cargo test --test task_sync              # UC-008 integration test
cargo test --test agent_bridge           # UC-007 integration test
cargo test --test presence_typing        # UC-009 integration test
cargo test --test tui_net_wiring         # UC-010 integration test
cargo test -p termchat-relay             # relay server unit tests
cargo test --lib                         # unit tests only
cargo test -p termchat-proto             # proto crate tests only
cargo fmt --check
cargo clippy -- -D warnings

# Full quality gate (run before committing)
cargo fmt --check && cargo clippy -- -D warnings && cargo test
```

## Architecture Quick Reference

Three-crate workspace: **termchat** (TUI client), **termchat-relay** (axum WebSocket relay), **termchat-proto** (shared wire format library).

Four layers: TUI (ratatui + crossterm) -> Application (chat/task/agent managers) -> Transport (quinn QUIC P2P preferred, tungstenite WebSocket relay fallback) -> Crypto (Noise XX handshake, x25519, ChaCha20-Poly1305). Relay never sees plaintext.

### Module Map

```
termchat/src/
  main.rs          # TUI event loop (ratatui + crossterm), CLI args (clap), logging init
  lib.rs           # Crate root: pub mod agent, app, chat, config, crypto, net, tasks, transport, ui
  app.rs           # App state, key event handling, panel focus, /task + /invite-agent commands
  config/mod.rs    # TOML config file: CliArgs, ClientConfig, ChatConfig, layered resolution
  net.rs           # Networking coordinator: NetCommand/NetEvent channels, spawn_net(), background tasks
  ui/              # TUI rendering (sidebar, chat_panel, task_panel, status_bar, theme)
  tasks/
    mod.rs         # TaskError enum, re-exports for merge + TaskManager
    merge.rs       # Pure CRDT merge: merge_lww, merge_task, merge_task_list, apply_field_update
    manager.rs     # TaskManager: room-scoped CRUD, apply_remote, build_full_state
  agent/
    mod.rs         # AgentError enum (thiserror)
    protocol.rs    # AgentMessage, BridgeMessage, JSON line encode/decode, validate_agent_id
    bridge.rs      # AgentBridge (Unix socket listener), AgentConnection, heartbeat_loop
    participant.rs # AgentParticipant (event loop, fan-out, room event forwarding)
  chat/
    mod.rs         # ChatManager: send/receive pipeline, ack tracking, events
    history.rs     # MessageStore trait, InMemoryStore, ResilientHistoryWriter
    room.rs        # Room struct, RoomManager, RoomEvent, validate_room_name, join flow
  crypto/
    mod.rs         # CryptoSession trait, CryptoError
    noise.rs       # StubNoiseSession (testing) + NoiseHandshake + NoiseXXSession (real Noise XX)
    keys.rs        # Identity keypairs, KeyStore trait, PeerKeyCache
  transport/
    mod.rs         # Transport trait, PeerId, TransportError
    loopback.rs    # LoopbackTransport (mpsc channels, for testing)
    quic.rs        # QuicTransport + QuicListener (QUIC via quinn, UC-003)
    hybrid.rs      # HybridTransport (preferred + fallback + offline queue, tokio::select! recv mux)
    relay.rs       # RelayTransport (WebSocket relay client, UC-004)

termchat-relay/src/
  main.rs          # axum server entry point, clap CLI args
  config.rs        # TOML config file: RelayCliArgs, RelayConfig, layered resolution
  relay.rs         # RelayState, WebSocket handler, peer registry, message routing, room message dispatch
  rooms.rs         # RoomRegistry: in-memory room directory, join request routing
  store.rs         # MessageStore: per-peer FIFO queues (1000 cap, eviction)

termchat-proto/src/
  lib.rs           # Crate root (pub mod agent, codec, message, relay, room, task)
  message.rs       # Wire format types: ChatMessage, Envelope, DeliveryAck, Nack, etc.
  codec.rs         # Postcard encode/decode with length-prefix framing
  relay.rs         # RelayMessage enum: Register, Registered, RelayPayload, Queued, Error, Room
  agent.rs         # AgentInfo, AgentCapability proto types
  room.rs          # RoomMessage enum: RegisterRoom, UnregisterRoom, ListRooms, RoomList, JoinRequest, JoinApproved, JoinDenied, MembershipUpdate
  task.rs          # TaskId, LwwRegister<T>, Task, TaskStatus, TaskFieldUpdate, TaskSyncMessage, encode/decode
  presence.rs      # PresenceStatus (Online/Away/Offline), PresenceMessage
  typing.rs        # TypingMessage (peer_id, room_id, is_typing)
```

### File Ownership (for agent teams)

When running multi-agent teams, assign module ownership to prevent merge conflicts:
- **Lead only**: Root `Cargo.toml`, `termchat/Cargo.toml`, `CLAUDE.md`
- **Builder-Proto**: `termchat-proto/`, `termchat/src/chat/`
- **Builder-Agent**: `termchat/src/agent/`
- **Builder-Infra**: `termchat/src/crypto/`, `termchat/src/transport/`
- **Builder-TUI**: `termchat/src/ui/`, `termchat/src/app.rs`, `termchat/src/main.rs`
- **Reviewer**: `tests/integration/`, `tests/property/`

## Coding Standards

- Rust edition 2024
- All public functions must have doc comments
- No `unwrap()` in production code — use `Result` with `thiserror`
- Commit after each completed use case, not after each file change
- Always run `/task-decompose` before implementing, even for small use cases
- Always include a reviewer agent in team configurations
- Keep agent tasks scoped to <20 tool calls to avoid context kills
- Builders must run `cargo fmt` and `cargo clippy -p <crate> -- -D warnings` before marking any task complete (not just at final gate)
- When spawning builder agents, include explicit "claim task #N immediately" in the prompt

## Test Strategy

- Unit tests: inline `#[cfg(test)]` modules
- Integration tests: `tests/integration/`, one file per use case
- Property tests: `tests/property/serialization.rs` (proptest for serialization round-trips)
- Run the full quality gate before every commit

## Requirements

Cockburn-style use cases in `docs/use-cases/`. Always check the relevant use case before implementing. Run verification commands after completing any task. See blueprint section 1.2 for the template.

## Process Learnings

- Extensions in Cockburn template produce ~40% of implementation tasks — never skip them
- Agent teams with clear module ownership produce zero merge conflicts
- Stubbed implementations (e.g., StubNoiseSession) enable incremental progress across UCs
- Agent reliability requires small tasks (~15-20 tool calls max per agent)
- The reviewer role is non-negotiable: blind testing against postconditions catches real bugs
- Define shared proto types first, before spawning builder agents — they code against the same contract
- Create `lib.rs` for any crate that integration tests need to import (e.g., `start_server()`)
- Check transitive dependency versions before locking workspace deps (e.g., axum 0.8 requires tungstenite 0.28)
- Serialization: use postcard (serde-only, no extra derives) over bincode. API: `postcard::to_allocvec()` / `postcard::from_bytes()`
- Use parking_lot::Mutex instead of std::sync::Mutex — infallible lock(), no unwrap needed
- When builders change function signatures (e.g., async→sync), integration test files may break — Lead must check cross-track test files in the integration gate
- Cross-track dependencies are solvable with task ordering: instruct one builder to prioritize the shared task, give the other builder independent work meanwhile
- Clippy pedantic warnings accumulate across parallel builder tracks — Phase 2C gate catches them, but per-task clippy is better
- "Out of Scope" in use cases prevents scope creep during implementation — always list what is NOT included
- Convergence patterns (e.g., CleanupContext) emerge from Cockburn extension analysis when multiple error paths need the same handling
- Clone `self` fields before `get_*_mut()` calls to avoid borrow checker E0502: `let id = self.id.clone(); let item = self.get_mut(key)?;`
- Lead must NOT edit builder-owned files — use SendMessage to request changes instead, to avoid duplicate/race conditions
- When builders work on the same crate, run `cargo clippy` at workspace level (`cargo clippy -- -D warnings`), not per-crate
- Git worktree (`git worktree add ../dir -b feature/uc-NNN`) enables parallel UC development without conflict risk — essential when two features touch overlapping files
- Opaque envelope payloads (`Envelope::Feature(Vec<u8>)` with app-layer decode) scale indefinitely without bloating the wire format enum — standard pattern for domain-specific message types
- Single-agent implementation is sufficient for medium-complexity UCs that follow established patterns; full Forge workflow (team, task-decompose) reserved for novel/high-complexity work
- When parallel agents create overlapping module files (e.g., `config.rs` vs `config/mod.rs`), the Lead must resolve the conflict before quality gate — Rust panics on dual module paths
- Add workspace dependencies to root Cargo.toml BEFORE spawning parallel agents — prevents Cargo.toml merge conflicts
- `cargo clippy --fix --allow-dirty` handles most lint auto-fixes; `private_interfaces` lint requires manual visibility adjustment
- Layered config (CLI > config file > env > defaults) via clap `env` attribute + `#[serde(default)]` TOML structs is a clean pattern with zero boilerplate
