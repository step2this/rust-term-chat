# TermChat Backlog

Items not assigned to a sprint yet. Pull from here when planning future work.

## Architecture / Refactoring

| # | Item | Description | Source | Priority |
|---|------|-------------|--------|----------|
| 10 | ~~ChatManager refactor~~ | ~~Extract ChatManager into a cleaner architecture — currently a monolith handling send/receive pipeline, ack tracking, events, and config. Consider splitting into separate send/receive/ack modules with clearer boundaries. Code quality grade flagged 1,158-line `chat/mod.rs` with room for improvement.~~ | ~~Code Quality Report (Sprint 8)~~ | ~~Done (UC-014, Sprint 10)~~ |

## UI / Visual

| # | Item | Description | Source | Priority |
|---|------|-------------|--------|----------|
| 1 | ~~Colorful TUI theme~~ | ~~Make the screen colorful~~ | ~~User request (Sprint 6)~~ | ~~Done (Sprint 8, WP-5)~~ |

## Agent Integration

| # | Item | Description | Source | Priority |
|---|------|-------------|--------|----------|
| 2 | Multiple simultaneous agents per room | Support more than one agent per bridge invocation | UC-007 Out of Scope | P3 Low |
| 3 | Agent process spawning from TUI | TermChat launches agent processes instead of requiring external agent | UC-007 Out of Scope | P3 Low |
| 4 | ~~Full crypto/transport path for agent fan-out~~ | ~~Exercise real Noise encryption + Transport send in agent message fan-out integration tests~~ | ~~Sprint 6 retro~~ | ~~Done (UC-015, Sprint 10)~~ |

## Relay Server

| # | Item | Description | Source | Priority |
|---|------|-------------|--------|----------|
| 5 | ~~Fix JoinApproved/JoinDenied relay routing~~ | ~~Route join responses via RelayPayload for targeted delivery to joiner~~ | ~~Sprint 5 retro~~ | ~~Done (UC-016, Sprint 10)~~ |

## Testing

| # | Item | Description | Source | Priority |
|---|------|-------------|--------|----------|
| 6 | Property tests for AgentMessage/BridgeMessage JSON serialization | Add proptest round-trip tests for agent protocol types | Sprint 6 retro | P3 Low |
| 7 | Property tests for RoomMessage serialization | Add proptest round-trip tests for room protocol types | Sprint 5 retro | P3 Low |

## Infrastructure

| # | Item | Description | Source | Priority |
|---|------|-------------|--------|----------|
| 8 | ~~Centralized config system~~ | ~~Runtime settings for heartbeat intervals, timeouts, relay URLs, etc. (TOML config file)~~ | ~~Sprint 6 retro / Blueprint Phase 8~~ | ~~Done (Sprint 8, UC-011)~~ |
| 9 | Automated cargo clippy hook for builder agents | Pre-task-completion hook that runs clippy automatically | Sprint 6 retro | P3 Low |
| 11 | ~~Auto-reconnect for relay transport~~ | ~~Reconnect to relay on disconnect with exponential backoff. Orphaned worktree with partial implementation exists at `feature/uc-011-reconnect`.~~ | ~~Sprint 8 retro~~ | ~~Done (UC-011, Sprint 8)~~ |
| 12 | ~~`/session-handoff` command~~ | ~~Command that writes session state to a file for the next Claude session to read, enabling clean cross-session continuity.~~ | ~~Sprint 8 retro~~ | ~~Done (command exists)~~ |
