# TermChat Backlog

Items not assigned to a sprint yet. Pull from here when planning future work.

## UI / Visual

| # | Item | Description | Source | Priority |
|---|------|-------------|--------|----------|
| 1 | Colorful TUI theme | Make the screen colorful — richer color palette, syntax highlighting for code blocks, distinct colors per user, vibrant room/panel styling. Move beyond the current minimal theme. | User request (Sprint 6) | P2 Medium |

## Agent Integration

| # | Item | Description | Source | Priority |
|---|------|-------------|--------|----------|
| 2 | Multiple simultaneous agents per room | Support more than one agent per bridge invocation | UC-007 Out of Scope | P3 Low |
| 3 | Agent process spawning from TUI | TermChat launches agent processes instead of requiring external agent | UC-007 Out of Scope | P3 Low |
| 4 | Full crypto/transport path for agent fan-out | Exercise real Noise encryption + Transport send in agent message fan-out integration tests | Sprint 6 retro | P2 Medium |

## Relay Server

| # | Item | Description | Source | Priority |
|---|------|-------------|--------|----------|
| 5 | Fix JoinApproved/JoinDenied relay routing | Route join responses via RelayPayload for targeted delivery to joiner | Sprint 5 retro | P2 Medium |

## Testing

| # | Item | Description | Source | Priority |
|---|------|-------------|--------|----------|
| 6 | Property tests for AgentMessage/BridgeMessage JSON serialization | Add proptest round-trip tests for agent protocol types | Sprint 6 retro | P3 Low |
| 7 | Property tests for RoomMessage serialization | Add proptest round-trip tests for room protocol types | Sprint 5 retro | P3 Low |

## Infrastructure

| # | Item | Description | Source | Priority |
|---|------|-------------|--------|----------|
| 8 | Centralized config system | Runtime settings for heartbeat intervals, timeouts, relay URLs, etc. (TOML config file) | Sprint 6 retro / Blueprint Phase 8 | P2 Medium |
| 9 | Automated cargo clippy hook for builder agents | Pre-task-completion hook that runs clippy automatically | Sprint 6 retro | P3 Low |
