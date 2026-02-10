# Use Case Registry

## Completed

| UC | Title | Goal Level | Priority | Sprint | Commit | Tests |
|----|-------|------------|----------|--------|--------|-------|
| UC-001 | Send Direct Message | User Goal | P0 | Sprint 1 | `d0ac08e` | 114 (unit + integration) |
| UC-002 | Receive Direct Message | User Goal | P0 | Sprint 1 | `c2d368f` | 121 cumulative |
| UC-005 | Establish E2E Handshake | Subfunction | P0 | Sprint 2 | `da5f397` | 149 cumulative |
| UC-003 | Establish P2P Connection | User Goal | P1 | Sprint 3 | `69e0f17` | 190 cumulative |
| UC-004 | Relay Messages via Server | User Goal | P1 | Sprint 4 | `47334e6` | 247 cumulative |
| UC-006 | Create Room | User Goal | P2 | Sprint 5 | `d95f56f` | 318 cumulative |
| UC-007 | Join Room as Agent Participant | User Goal | P2 | Sprint 6 | `383e25b` | 405 cumulative |
| UC-008 | Share Task List | User Goal | P3 | Sprint 7 | `bd7ab88` | 499 cumulative |
| UC-009 | Typing Indicators & Presence | User Goal | P2 | Sprint 7 | `383e25b` | 499 cumulative |
| UC-010 | Connect to Relay & Exchange Messages | User Goal | P1 | Sprint 8 | `1d8567f` | 675 cumulative |
| UC-011 | Configure Client & Relay (CLI + TOML) | User Goal | P1 | Sprint 8 | `1d8567f` | 675 cumulative |
| UC-012 | Polish for Ship (Theme, CI, Logging) | Summary | P1 | Sprint 8 | `1d8567f` | 675 cumulative |
| UC-013 | Harden Dependency Hygiene | User Goal | P1 | Sprint 9 | `0613177` | 685 cumulative |
| UC-014 | Refactor ChatManager into Focused Submodules | Subfunction | P2 | Sprint 10 | `899d0a4` | 699 cumulative |
| UC-015 | Agent Crypto/Transport Fan-Out | Subfunction | P2 | Sprint 10 | `89ab0df` | 699 cumulative |
| UC-016 | Route JoinApproved/JoinDenied via Relay | Subfunction | P2 | Sprint 10 | `5e0d0d9` | 699 cumulative |

## In Progress

| UC | Title | Goal Level | Priority | Sprint | Status |
|----|-------|------------|----------|--------|--------|
| UC-017 | Connect TUI to Live Backend State | User Goal | P0 | Sprint 11 | UC Written — pending review & task decomposition |

## Planned

None currently.

## Milestones

| Milestone | Status | Commit | Notes |
|-----------|--------|--------|-------|
| Phase 0: Forge Setup | Done | `4ef0fd1` | Slash commands, agent configs, hooks, use cases |
| Phase 1: Hello Ratatui | Done | `da5f397` | TUI with 3-panel layout, input, scrolling |
| Phase 2: Local Chat | Done | `c2d368f` | UC-001 + UC-002 (localhost send/receive) |
| Phase 3: E2E Encryption | Done | `da5f397` | UC-005 (Noise XX handshake) |
| Phase 4: Hybrid Networking | Done | `47334e6` | UC-003 + UC-004 |
| Phase 5: Rooms & History | Done | `d95f56f` | UC-006 (rooms done, history later) |
| Phase 6: Agent Integration | Done | `383e25b` | UC-007 (agent join) |
| Phase 7: Task Coordination | Done | `bd7ab88` | UC-008 (shared tasks) + UC-009 (typing/presence) |
| Phase 8: Polish & Ship | Done | `1d8567f` | UC-010 + UC-011 + UC-012 (relay, config, polish) |
| Phase 9: Hardening | Done | `0613177` | UC-013 (dependency hygiene) |
| Phase 10: Integration Hardening | Done | `b2349fb` | UC-014 + UC-015 + UC-016 (refactor, crypto fan-out, relay routing) |

## Dependency Graph

```
UC-001 (Send) ─────┐
                    ├── UC-006 (Rooms) ✓ ── UC-007 (Agent) ✓ ── UC-008 (Tasks) ✓
UC-002 (Receive) ──┘                                              │
                                                                   ├── UC-009 (Presence) ✓
UC-005 (E2E) ── UC-003 (P2P) ✓ ── UC-004 (Relay) ✓              │
                                                                   ├── UC-010 (Live Relay) ✓
                                                                   │
                                              UC-012 (Polish) ✓ ──┤
                                              UC-011 (Config) ✓ ──┤
                                                                   │
                                              UC-013 (Dep Hygiene) ✓
                                                                   │
                                              UC-014 (ChatManager Refactor) ✓
                                              UC-015 (Agent Crypto Fan-Out) ✓
                                              UC-016 (Join Relay Routing) ✓
                                                                   │
UC-001 + UC-002 + UC-006 + UC-009 + UC-010 + UC-014 + UC-016 ─── UC-017 (Connect TUI to Live Backend) ⏳
```
