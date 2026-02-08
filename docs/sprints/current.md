# Current Sprint: Sprint 7 — Shared Task Coordination

## Goal

Implement shared task lists with real-time synchronization between room members. This continues Phase 6: Agent Integration and begins Phase 7: Task Coordination.

## Status: In Progress

## Prerequisites (all met)

- [x] UC-001 Send Direct Message (Sprint 1)
- [x] UC-002 Receive Direct Message (Sprint 1)
- [x] UC-005 E2E Handshake (Sprint 2)
- [x] Phase 1 Hello Ratatui TUI (Sprint 2)
- [x] UC-003 Establish P2P Connection (Sprint 3)
- [x] UC-004 Relay Messages via Server (Sprint 4)
- [x] UC-006 Create Room (Sprint 5)
- [x] UC-007 Join Room as Agent Participant (Sprint 6)

## Use Cases This Sprint

| UC | Title | Status | Task Decomposition | Agent Team |
|----|-------|--------|--------------------|------------|
| UC-008 | Shared Task List | Pending | — | — |

## Rust Concepts to Learn

Per blueprint (Section 2.4):
- [ ] CRDT basics for conflict-free sync
- [ ] More complex state management
- [ ] Task data modeling

## Process Checklist

- [ ] Write UC-008 use case with `/uc-create`
- [ ] Review UC-008 with `/uc-review`
- [ ] Fix review issues
- [ ] Run `/task-decompose uc-008` to break into tasks
- [ ] Run `/agent-team-plan uc-008` to design agent team
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
| Sprint 5 | UC-006 | Done | `docs/retrospectives/sprint5-uc006.md` |
| Sprint 6 | UC-007 | Done | `docs/retrospectives/sprint6-uc007.md` |
