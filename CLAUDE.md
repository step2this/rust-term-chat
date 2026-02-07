# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@docs/termchat-blueprint.md

## Project State

Active development. Three-crate workspace is initialized and building. Completed: UC-001 (Send), UC-002 (Receive), UC-005 (E2E Handshake), Phase 1 (Hello Ratatui TUI). 149 tests passing. Next: UC-003 (P2P) or UC-004 (Relay Fallback).

## Build & Development Commands

```bash
cargo build
cargo run                                # launch TUI client
cargo run --bin termchat-relay           # relay server (stub)
cargo test                               # all tests (149)
cargo test --test send_receive           # UC-001/UC-002 integration test
cargo test --test e2e_encryption         # UC-005 integration test
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
  main.rs          # TUI event loop (ratatui + crossterm)
  lib.rs           # Crate root: pub mod app, chat, crypto, transport, ui
  app.rs           # App state, key event handling, panel focus
  ui/              # TUI rendering (sidebar, chat_panel, task_panel, status_bar, theme)
  chat/
    mod.rs         # ChatManager: send/receive pipeline, ack tracking, events
    history.rs     # MessageStore trait, InMemoryStore, ResilientHistoryWriter
  crypto/
    mod.rs         # CryptoSession trait, CryptoError
    noise.rs       # StubNoiseSession (testing) + NoiseHandshake + NoiseXXSession (real Noise XX)
    keys.rs        # Identity keypairs, KeyStore trait, PeerKeyCache
  transport/
    mod.rs         # Transport trait, PeerId, TransportError
    loopback.rs    # LoopbackTransport (mpsc channels, for testing)
    hybrid.rs      # HybridTransport (preferred + fallback + offline queue)

termchat-proto/src/
  lib.rs           # Crate root
  message.rs       # Wire format types: ChatMessage, Envelope, DeliveryAck, Nack, etc.
  codec.rs         # Bincode encode/decode with length-prefix framing
```

### File Ownership (for agent teams)

When running multi-agent teams, assign module ownership to prevent merge conflicts:
- **Lead only**: Root `Cargo.toml`, `termchat/Cargo.toml`, `CLAUDE.md`
- **Builder-Proto**: `termchat-proto/`, `termchat/src/chat/`
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
