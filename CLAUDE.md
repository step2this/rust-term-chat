# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@docs/termchat-blueprint.md

## Project State

Blueprint/planning phase. No Rust code exists yet. The project needs to be initialized with `cargo init`.

## Build & Development Commands

```bash
cargo build
cargo run
cargo run --bin termchat-relay          # relay server
cargo test                              # all tests
cargo test --test send_receive          # single integration test
cargo test --lib                        # unit tests only
cargo fmt --check
cargo clippy -- -D warnings

# Full quality gate (run before committing)
cargo fmt --check && cargo clippy -- -D warnings && cargo test
```

## Architecture Quick Reference

Three-crate workspace: **termchat** (TUI client), **termchat-relay** (axum WebSocket relay), **termchat-proto** (shared wire format library).

Four layers: TUI (ratatui + crossterm) → Application (chat/task/agent managers) → Transport (quinn QUIC P2P preferred, tungstenite WebSocket relay fallback) → Crypto (Noise XX handshake, x25519, ChaCha20-Poly1305). Relay never sees plaintext.

## Coding Standards

- Rust edition 2024
- All public functions must have doc comments
- No `unwrap()` in production code — use `Result` with `thiserror`
- Commit after each completed use case, not after each file change

## Test Strategy

- Unit tests: inline `#[cfg(test)]` modules
- Integration tests: `tests/integration/`, one file per use case
- Property tests: `tests/property/serialization.rs` (proptest for serialization round-trips)

## Requirements

Cockburn-style use cases in `docs/use-cases/`. Always check the relevant use case before implementing. Run verification commands after completing any task. See blueprint section 1.2 for the template.
