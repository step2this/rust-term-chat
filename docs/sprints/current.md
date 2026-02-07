# Current Sprint: Sprint 4 — Relay Fallback

## Goal

Implement a WebSocket-based relay server and client as fallback transport when P2P (QUIC) connections fail. This completes Phase 4: Hybrid Networking.

## Status: Done

## Prerequisites (all met)

- [x] UC-001 Send Direct Message (Sprint 1)
- [x] UC-002 Receive Direct Message (Sprint 1)
- [x] UC-005 E2E Handshake (Sprint 2)
- [x] Phase 1 Hello Ratatui TUI (Sprint 2)
- [x] UC-003 Establish P2P Connection (Sprint 3)

## Use Cases This Sprint

| UC | Title | Status | Task Decomposition | Agent Team |
|----|-------|--------|--------------------|------------|
| UC-004 | Relay Messages via Server | Done | `docs/tasks/uc-004-tasks.md` | `docs/teams/uc-004-team.md` |

## Results

- **Tests**: 247 total (57 new for UC-004)
- **New files**: 7 (relay server: main.rs, relay.rs, store.rs; relay client: relay.rs; proto: relay.rs; integration: relay_fallback.rs; relay lib.rs)
- **Modified files**: 6 (workspace Cargo.toml, termchat Cargo.toml, relay Cargo.toml, transport/mod.rs, hybrid.rs, proto/lib.rs)
- **Team**: Lead + Builder-Relay + Builder-Client + Reviewer (4 agents)
- **Parallelism**: Server and client tracks ran simultaneously (zero merge conflicts)

## Rust Concepts Learned

Per blueprint (Section 2.4):
- [x] WebSockets (tokio-tungstenite client, axum WebSocket server)
- [x] State machines (relay connection lifecycle: connect -> register -> active -> disconnect)
- [x] Enum-based transport abstraction (Transport trait, RelayTransport implements it)
- [x] tokio::select! for multiplexing (HybridTransport recv across both transports)

## Process Checklist

- [x] Write UC-004 use case with `/uc-create`
- [x] Review UC-004 with `/uc-review` (score: 9/10, 5 fixes applied)
- [x] Fix review issues
- [x] Run `/task-decompose uc-004` to break into tasks (16 tasks)
- [x] Run `/agent-team-plan uc-004` to design agent team (4 agents)
- [x] Get user approval on team plan
- [x] Spawn team and execute
- [x] Gate 1: `cargo test -p termchat-relay && cargo clippy -p termchat-relay -- -D warnings` (15 tests)
- [x] Gate 2: `cargo test -p termchat -- hybrid::tests && cargo clippy -p termchat -- -D warnings` (8 tests)
- [x] Gate 3: `cargo fmt --check && cargo clippy -- -D warnings && cargo test` (247 tests)
- [x] Verification: `cargo test --test relay_fallback` (15 tests)
- [x] Commit
- [x] Retrospective: `docs/retrospectives/sprint4-uc004.md`

## Previous Sprints

| Sprint | Use Cases | Status | Retro |
|--------|-----------|--------|-------|
| Sprint 0 (Phase 0) | Forge Setup | Done | `docs/retrospectives/phase0-uc001-uc005.md` |
| Sprint 1 | UC-001, UC-002 | Done | `docs/retrospectives/phase0-uc001-uc005.md` |
| Sprint 2 | UC-005, Phase 1 TUI | Done | `docs/retrospectives/phase0-uc001-uc005.md` |
| Sprint 3 | UC-003 | Done | — |
| Sprint 4 | UC-004 | Done | `docs/retrospectives/sprint4-uc004.md` |
