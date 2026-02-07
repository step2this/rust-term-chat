# Current Sprint: Sprint 4 — Relay Fallback

## Goal

Implement a WebSocket-based relay server and client as fallback transport when P2P (QUIC) connections fail. This completes Phase 4: Hybrid Networking.

## Status: In Progress

## Prerequisites (all met)

- [x] UC-001 Send Direct Message (Sprint 1)
- [x] UC-002 Receive Direct Message (Sprint 1)
- [x] UC-005 E2E Handshake (Sprint 2)
- [x] Phase 1 Hello Ratatui TUI (Sprint 2)
- [x] UC-003 Establish P2P Connection (Sprint 3)

## Use Cases This Sprint

| UC | Title | Status | Task Decomposition | Agent Team |
|----|-------|--------|--------------------|------------|
| UC-004 | Relay Fallback | Not Started | — | — |

## Rust Concepts to Learn

Per blueprint (Section 2.4):
- WebSockets (tokio-tungstenite)
- State machines (relay connection lifecycle)
- Enum-based transport abstraction (Transport trait, already exists)
- Trait objects (dynamic dispatch for hybrid transport)

## Process Checklist

Before starting implementation:
- [ ] Write UC-004 use case with `/uc-create`
- [ ] Review UC-004 with `/uc-review`
- [ ] Fix review issues
- [ ] Run `/task-decompose uc-004` to break into tasks
- [ ] Run `/agent-team-plan uc-004` to design agent team
- [ ] Get user approval on team plan
- [ ] Spawn team and execute
- [ ] Gate 3: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
- [ ] Verification: `cargo test --test relay_fallback`
- [ ] Commit

## Previous Sprints

| Sprint | Use Cases | Status | Retro |
|--------|-----------|--------|-------|
| Sprint 0 (Phase 0) | Forge Setup | Done | `docs/retrospectives/phase0-uc001-uc005.md` |
| Sprint 1 | UC-001, UC-002 | Done | `docs/retrospectives/phase0-uc001-uc005.md` |
| Sprint 2 | UC-005, Phase 1 TUI | Done | `docs/retrospectives/phase0-uc001-uc005.md` |
| Sprint 3 | UC-003 | Done | — |
