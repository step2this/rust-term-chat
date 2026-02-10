# Current Sprint: Sprint 11 — Wire TUI to Live Backend

## Goal

Replace all hardcoded demo state in the TUI with live data from the networking layer. Two users should be able to chat end-to-end with per-conversation isolation, room creation/joining via commands, real connection/typing/presence status, and delivery confirmation.

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
- [x] UC-008 Share Task List (Sprint 7)
- [x] UC-009 Typing Indicators & Presence (Sprint 7)
- [x] UC-010 Connect to Relay & Exchange Messages (Sprint 8)
- [x] UC-011 Configure Client & Relay (Sprint 8)
- [x] UC-012 Polish for Ship (Sprint 8)
- [x] UC-013 Harden Dependency Hygiene (Sprint 9)
- [x] UC-014 Refactor ChatManager into Focused Submodules (Sprint 10)
- [x] UC-015 Agent Crypto/Transport Fan-Out (Sprint 10)
- [x] UC-016 Route JoinApproved/JoinDenied via Relay (Sprint 10)

## Use Cases This Sprint

| UC | Title | Status | Task Decomposition | Agent Team |
|----|-------|--------|--------------------|------------|
| UC-017 | Connect TUI to Live Backend State | UC Written | Pending `/task-decompose` | Pending `/agent-team-plan` |

## Process Checklist

- [ ] Write UC doc(s) with `/uc-create`
- [ ] Review UC doc(s) with `/uc-review`
- [ ] Fix review issues
- [ ] Run `/task-decompose` to break into tasks
- [ ] Run `/agent-team-plan` to design agent team
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
| Sprint 7 | UC-008, UC-009 | Done | `docs/retrospectives/sprint7-uc008.md`, `sprint7-uc009.md` |
| Sprint 8 | UC-010, UC-011, UC-012 | Done | `docs/retrospectives/sprint8-uc010-uc011.md` |
| Sprint 9 | UC-013 | Done | `docs/retrospectives/sprint9-uc013.md` |
| Sprint 10 | UC-014, UC-015, UC-016 | Done | — |
