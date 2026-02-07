# Current Sprint: Sprint 3 — P2P Networking

## Goal

Establish QUIC-based P2P connections between two TermChat instances. This is the first real networking sprint — previous sprints used loopback transports.

## Status: Not Started

## Prerequisites (all met)

- [x] UC-001 Send Direct Message (Sprint 1)
- [x] UC-002 Receive Direct Message (Sprint 1)
- [x] UC-005 E2E Handshake (Sprint 2)
- [x] Phase 1 Hello Ratatui TUI (Sprint 2)

## Use Cases This Sprint

| UC | Title | Status | Task Decomposition | Agent Team |
|----|-------|--------|--------------------|------------|
| UC-003 | Establish P2P Connection | Not started | Pending `/task-decompose` | Pending `/agent-team-plan` |

## Rust Concepts to Learn

Per blueprint (Section 2.4):
- Tokio networking
- quinn (QUIC)
- Futures and pinning
- `Arc<Mutex<>>`

## Process Checklist

Before starting implementation:
- [ ] Write UC-003 use case with `/uc-create`
- [ ] Review UC-003 with `/uc-review`
- [ ] Run `/task-decompose uc-003` to break into tasks
- [ ] Run `/agent-team-plan uc-003` to design agent team
- [ ] Get user approval on team plan
- [ ] Spawn team and execute

## Previous Sprints

| Sprint | Use Cases | Status | Retro |
|--------|-----------|--------|-------|
| Sprint 0 (Phase 0) | Forge Setup | Done | `docs/retrospectives/phase0-uc001-uc005.md` |
| Sprint 1 | UC-001, UC-002 | Done | `docs/retrospectives/phase0-uc001-uc005.md` |
| Sprint 2 | UC-005, Phase 1 TUI | Done | `docs/retrospectives/phase0-uc001-uc005.md` |
