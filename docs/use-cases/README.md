# Use Case Registry

## Completed

| UC | Title | Goal Level | Priority | Sprint | Commit | Tests |
|----|-------|------------|----------|--------|--------|-------|
| UC-001 | Send Direct Message | User Goal | P0 | Sprint 1 | `d0ac08e` | 114 (unit + integration) |
| UC-002 | Receive Direct Message | User Goal | P0 | Sprint 1 | `c2d368f` | 121 cumulative |
| UC-005 | Establish E2E Handshake | Subfunction | P0 | Sprint 2 | `da5f397` | 149 cumulative |
| UC-003 | Establish P2P Connection | User Goal | P1 | Sprint 3 | `69e0f17` | 190 cumulative |
| UC-004 | Relay Messages via Server | User Goal | P1 | Sprint 4 | `47334e6` | 247 cumulative |

## In Progress

| UC | Title | Goal Level | Priority | Sprint | Depends On |
|----|-------|------------|----------|--------|------------|
| UC-006 | Create Room | User Goal | P2 | Sprint 5 | UC-001, UC-002 |

## Planned
| UC-007 | Agent Join Chat | User Goal | P2 | Sprint 6 | UC-006 |
| UC-008 | Shared Task List | User Goal | P3 | Sprint 7 | UC-007 |

## Milestones

| Milestone | Status | Commit | Notes |
|-----------|--------|--------|-------|
| Phase 0: Forge Setup | Done | `4ef0fd1` | Slash commands, agent configs, hooks, use cases |
| Phase 1: Hello Ratatui | Done | `da5f397` | TUI with 3-panel layout, input, scrolling |
| Phase 2: Local Chat | Done | `c2d368f` | UC-001 + UC-002 (localhost send/receive) |
| Phase 3: E2E Encryption | Done | `da5f397` | UC-005 (Noise XX handshake) |
| Phase 4: Hybrid Networking | Done | `47334e6` | UC-003 + UC-004 |
| Phase 5: Rooms & History | Planned | — | UC-006 |
| Phase 6: Agent Integration | Planned | — | UC-007 + UC-008 |

## Dependency Graph

```
UC-001 (Send) ─────┐
                    ├── UC-006 (Rooms) ── UC-007 (Agent Join) ── UC-008 (Tasks)
UC-002 (Receive) ──┘

UC-005 (E2E) ── UC-003 (P2P) ✓ ── UC-004 (Relay) ✓
```
