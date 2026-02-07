# Current Sprint: Sprint 5 — Rooms & History

## Goal

Implement chat room creation, joining, and persistent message history. This begins Phase 5: Rooms & History.

## Status: In Progress

## Prerequisites (all met)

- [x] UC-001 Send Direct Message (Sprint 1)
- [x] UC-002 Receive Direct Message (Sprint 1)
- [x] UC-005 E2E Handshake (Sprint 2)
- [x] Phase 1 Hello Ratatui TUI (Sprint 2)
- [x] UC-003 Establish P2P Connection (Sprint 3)
- [x] UC-004 Relay Messages via Server (Sprint 4)

## Use Cases This Sprint

| UC | Title | Status | Task Decomposition | Agent Team |
|----|-------|--------|--------------------|------------|
| UC-006 | Create Room | In Progress | — | — |

## Rust Concepts to Learn

Per blueprint (Section 2.4):
- [ ] SQLite (rusqlite) for persistent message history
- [ ] Lifetimes (database connections, iterators over query results)
- [ ] Iterators (transforming DB rows into domain types)
- [ ] Builder pattern (room configuration, query construction)

## Process Checklist

- [ ] Write UC-006 use case with `/uc-create`
- [ ] Review UC-006 with `/uc-review`
- [ ] Fix review issues
- [ ] Run `/task-decompose uc-006` to break into tasks
- [ ] Run `/agent-team-plan uc-006` to design agent team
- [ ] Get user approval on team plan
- [ ] Spawn team and execute
- [ ] Quality gates
- [ ] Verification
- [ ] Commit
- [ ] Retrospective

## Previous Sprints

| Sprint | Use Cases | Status | Retro |
|--------|-----------|--------|-------|
| Sprint 0 (Phase 0) | Forge Setup | Done | `docs/retrospectives/phase0-uc001-uc005.md` |
| Sprint 1 | UC-001, UC-002 | Done | `docs/retrospectives/phase0-uc001-uc005.md` |
| Sprint 2 | UC-005, Phase 1 TUI | Done | `docs/retrospectives/phase0-uc001-uc005.md` |
| Sprint 3 | UC-003 | Done | — |
| Sprint 4 | UC-004 | Done | `docs/retrospectives/sprint4-uc004.md` |
