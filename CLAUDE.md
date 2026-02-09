# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@docs/termchat-blueprint.md

## Project State

Active development. Three-crate workspace initialized and building. Completed: UC-001 through UC-016 (Send, Receive, P2P, Relay, E2E, Rooms, Agent, Tasks, Presence, Live Relay, Auto-Reconnect, Polish, Dependency Hygiene, ChatManager Refactor, Agent Crypto Fan-Out, Join Relay Routing), Phase 1 (Hello Ratatui TUI). Sprint 10 complete: parallel feature branches for refactoring, integration testing, and relay routing. 699 tests passing.

## MANDATORY: Forge Workflow & Delegation Rules

**STOP. Before writing ANY code, verify ALL of these are true:**

1. **A UC document exists** in `docs/use-cases/` for the work you're about to do
   - If not: run `/uc-create` first. No exceptions.
2. **The UC has been reviewed** via `/uc-review` and all CRITICAL issues fixed
   - If not: run `/uc-review` first. Do not skip this — it catches 40% of waste.
3. **Tasks have been decomposed** in `docs/tasks/uc-NNN-tasks.md`
   - If not: run `/task-decompose` first.
4. **A team plan exists** in `docs/teams/uc-NNN-team.md` (for High/XL complexity UCs)
   - If not: run `/agent-team-plan` first.
5. **You are on a feature branch worktree**, not main
   - If not: `git worktree add ../rust-term-chat-uc-NNN -b feature/uc-NNN-slug`
6. **You are delegating implementation to subagents**, not writing code yourself
   - The Lead NEVER writes production code directly. Use the `Task` tool.
   - This applies even for "quick fixes" and "small changes" — delegate them.
   - The Lead's job: coordinate, create tasks, spawn agents, run gates, integrate.

**Common failure modes to watch for (these have happened before):**

- "This is small, I'll just do it myself" → NO. Spawn a subagent. Always.
- "I'll start coding while planning the team" → NO. Plan first, delegate second.
- "I'll edit this one file quickly" → NO. Even single-file edits go through subagents.
- "T-017-01 is a prerequisite, the Lead should handle it" → NO. Delegate to a subagent.
- "I already read the file, might as well edit it" → NO. Reading is fine. Editing is not.

**The Lead's permitted actions (exhaustive list):**

- Read files (Glob, Grep, Read) — for understanding and coordination
- Run shell commands (cargo build, cargo test, git) — for gates and integration
- Create/update docs (CLAUDE.md, docs/, sprint files) — for project management
- Spawn subagents (Task tool) — for ALL implementation work
- Create teams (TeamCreate) — for multi-agent coordination
- Create/update tasks (TaskCreate, TaskUpdate) — for tracking
- Send messages (SendMessage) — for team coordination
- Edit ONLY `main.rs` and `Cargo.toml` during integration phases (2C/3C) — and ONLY after builder tracks complete, via subagent when possible

**If you catch yourself about to use Edit or Write on a .rs file: STOP and spawn a subagent instead.**

## Build & Development Commands

```bash
cargo build
cargo run                                # launch TUI client (offline demo mode)
cargo run -- --help                      # show CLI options
cargo run -- --relay-url ws://127.0.0.1:9000/ws --peer-id alice --remote-peer bob
cargo run --bin termchat-relay           # relay server (ws://0.0.0.0:9000)
cargo run --bin termchat-relay -- --help # show relay CLI options
cargo test                               # all tests
cargo test --test send_receive           # UC-001/UC-002 integration test
cargo test --test e2e_encryption         # UC-005 integration test
cargo test --test p2p_connection         # UC-003 integration test
cargo test --test relay_fallback         # UC-004 integration test
cargo test --test room_management        # UC-006 integration test
cargo test --test task_sync              # UC-008 integration test
cargo test --test agent_bridge           # UC-007 integration test
cargo test --test presence_typing        # UC-009 integration test
cargo test --test tui_net_wiring         # UC-010 integration test
cargo test --test relay_reconnect        # UC-011 integration test
cargo test -p termchat-relay             # relay server unit tests
cargo test --lib                         # unit tests only
cargo test -p termchat-proto             # proto crate tests only
cargo fmt --check
cargo clippy -- -D warnings

# Full quality gate (run before committing)
cargo fmt --check && cargo clippy -- -D warnings && cargo test && cargo deny check
```

## Architecture Quick Reference

Three-crate workspace: **termchat** (TUI client), **termchat-relay** (axum WebSocket relay), **termchat-proto** (shared wire format library).

Four layers: TUI (ratatui + crossterm) -> Application (chat/task/agent managers) -> Transport (quinn QUIC P2P preferred, tungstenite WebSocket relay fallback) -> Crypto (Noise XX handshake, x25519, ChaCha20-Poly1305). Relay never sees plaintext.

### Module Map

```
termchat/src/
  main.rs          # TUI event loop (ratatui + crossterm), CLI args (clap), logging init
  lib.rs           # Crate root: pub mod agent, app, chat, config, crypto, net, tasks, transport, ui
  app.rs           # App state, key event handling, panel focus, /task + /invite-agent commands
  config/mod.rs    # TOML config file: CliArgs, ClientConfig, ChatConfig, layered resolution
  net.rs           # Networking coordinator: NetCommand/NetEvent channels, spawn_net(), background tasks
  ui/              # TUI rendering (sidebar, chat_panel, task_panel, status_bar, theme)
  tasks/
    mod.rs         # TaskError enum, re-exports for merge + TaskManager
    merge.rs       # Pure CRDT merge: merge_lww, merge_task, merge_task_list, apply_field_update
    manager.rs     # TaskManager: room-scoped CRUD, apply_remote, build_full_state
  agent/
    mod.rs         # AgentError enum (thiserror)
    protocol.rs    # AgentMessage, BridgeMessage, JSON line encode/decode, validate_agent_id
    bridge.rs      # AgentBridge (Unix socket listener), AgentConnection, heartbeat_loop
    participant.rs # AgentParticipant (event loop, fan-out, room event forwarding)
  chat/
    mod.rs         # ChatManager: send/receive pipeline, ack tracking, events
    history.rs     # MessageStore trait, InMemoryStore, ResilientHistoryWriter
    room.rs        # Room struct, RoomManager, RoomEvent, validate_room_name, join flow
  crypto/
    mod.rs         # CryptoSession trait, CryptoError
    noise.rs       # StubNoiseSession (testing) + NoiseHandshake + NoiseXXSession (real Noise XX)
    keys.rs        # Identity keypairs, KeyStore trait, PeerKeyCache
  transport/
    mod.rs         # Transport trait, PeerId, TransportError
    loopback.rs    # LoopbackTransport (mpsc channels, for testing)
    quic.rs        # QuicTransport + QuicListener (QUIC via quinn, UC-003)
    hybrid.rs      # HybridTransport (preferred + fallback + offline queue, tokio::select! recv mux)
    relay.rs       # RelayTransport (WebSocket relay client, UC-004)

termchat-relay/src/
  main.rs          # axum server entry point, clap CLI args
  config.rs        # TOML config file: RelayCliArgs, RelayConfig, layered resolution
  relay.rs         # RelayState, WebSocket handler, peer registry, message routing, room message dispatch
  rooms.rs         # RoomRegistry: in-memory room directory, join request routing
  store.rs         # MessageStore: per-peer FIFO queues (1000 cap, eviction)

termchat-proto/src/
  lib.rs           # Crate root (pub mod agent, codec, message, relay, room, task)
  message.rs       # Wire format types: ChatMessage, Envelope, DeliveryAck, Nack, etc.
  codec.rs         # Postcard encode/decode with length-prefix framing
  relay.rs         # RelayMessage enum: Register, Registered, RelayPayload, Queued, Error, Room
  agent.rs         # AgentInfo, AgentCapability proto types
  room.rs          # RoomMessage enum: RegisterRoom, UnregisterRoom, ListRooms, RoomList, JoinRequest, JoinApproved, JoinDenied, MembershipUpdate
  task.rs          # TaskId, LwwRegister<T>, Task, TaskStatus, TaskFieldUpdate, TaskSyncMessage, encode/decode
  presence.rs      # PresenceStatus (Online/Away/Offline), PresenceMessage
  typing.rs        # TypingMessage (peer_id, room_id, is_typing)
```

### File Ownership (for agent teams)

When running multi-agent teams, assign module ownership to prevent merge conflicts:
- **Lead only**: Root `Cargo.toml`, `termchat/Cargo.toml`, `CLAUDE.md`
- **Builder-Proto**: `termchat-proto/`, `termchat/src/chat/`
- **Builder-Agent**: `termchat/src/agent/`
- **Builder-Infra**: `termchat/src/crypto/`, `termchat/src/transport/`
- **Builder-TUI**: `termchat/src/ui/`, `termchat/src/app.rs`, `termchat/src/main.rs`
- **Reviewer**: `tests/integration/`, `tests/property/`

## Coding Standards

- Rust edition 2024
- All public functions must have doc comments
- No `unwrap()` in production code — use `Result` with `thiserror`
- Commit after each completed use case, not after each file change — never bundle multiple UCs into one commit
- **ALWAYS USE THE FORGE.** See "MANDATORY: Forge Workflow & Delegation Rules" section above. UC doc → review → task-decompose → team plan → worktree → delegate to subagents. Never skip straight to code. Never write code as Lead. Reference `@.claude/skills/pre-implementation-checklist.md` before starting.
- Always include a reviewer agent in team configurations
- Keep agent tasks scoped to <20 tool calls to avoid context kills
- Builders must run `cargo fmt` and `cargo clippy -p <crate> -- -D warnings` before marking any task complete (not just at final gate)
- When spawning builder agents, include explicit "claim task #N immediately" in the prompt
- Use delegate mode (`Shift+Tab`) when running as team lead to avoid manual approval bottlenecks
- Set `plan_mode_required: true` for teammate agents — they must present a plan before implementing

## Test Strategy

- Unit tests: inline `#[cfg(test)]` modules
- Integration tests: `tests/integration/`, one file per use case
- Property tests: `tests/property/serialization.rs` (proptest for serialization round-trips)
- Run the full quality gate before every commit

## Requirements

Cockburn-style use cases in `docs/use-cases/`. Always check the relevant use case before implementing. Run verification commands after completing any task. See blueprint section 1.2 for the template.

## Process Learnings

- Extensions in Cockburn template produce ~40% of implementation tasks — never skip them
- Agent teams with clear module ownership produce zero merge conflicts
- Stubbed implementations (e.g., StubNoiseSession) enable incremental progress across UCs
- Agent reliability requires small tasks (~15-20 tool calls max per agent)
- The reviewer role is non-negotiable: blind testing against postconditions catches real bugs
- Define shared proto types first, before spawning builder agents — they code against the same contract
- Create `lib.rs` for any crate that integration tests need to import (e.g., `start_server()`)
- Check transitive dependency versions before locking workspace deps (e.g., axum 0.8 requires tungstenite 0.28)
- Serialization: use postcard (serde-only, no extra derives) over bincode. API: `postcard::to_allocvec()` / `postcard::from_bytes()`
- Use parking_lot::Mutex instead of std::sync::Mutex — infallible lock(), no unwrap needed
- When builders change function signatures (e.g., async→sync), integration test files may break — Lead must check cross-track test files in the integration gate
- Cross-track dependencies are solvable with task ordering: instruct one builder to prioritize the shared task, give the other builder independent work meanwhile
- Clippy pedantic warnings accumulate across parallel builder tracks — Phase 2C gate catches them, but per-task clippy is better
- "Out of Scope" in use cases prevents scope creep during implementation — always list what is NOT included
- Convergence patterns (e.g., CleanupContext) emerge from Cockburn extension analysis when multiple error paths need the same handling
- Clone `self` fields before `get_*_mut()` calls to avoid borrow checker E0502: `let id = self.id.clone(); let item = self.get_mut(key)?;`
- Lead must NOT edit builder-owned files — use SendMessage to request changes instead, to avoid duplicate/race conditions
- When builders work on the same crate, run `cargo clippy` at workspace level (`cargo clippy -- -D warnings`), not per-crate
- Git worktree (`git worktree add ../dir -b feature/uc-NNN`) enables parallel UC development without conflict risk — essential when two features touch overlapping files
- Opaque envelope payloads (`Envelope::Feature(Vec<u8>)` with app-layer decode) scale indefinitely without bloating the wire format enum — standard pattern for domain-specific message types
- Always work off of a worktree to allow multiple agents — `git worktree add` for each feature branch
- Single-agent implementation (one subagent, not the Lead writing code) is sufficient for medium-complexity UCs that follow established patterns; full team workflow (TeamCreate, parallel builders) reserved for High/XL complexity work. Either way, the Lead never writes code directly — it always goes through a subagent via the Task tool
- When parallel agents create overlapping module files (e.g., `config.rs` vs `config/mod.rs`), the Lead must resolve the conflict before quality gate — Rust panics on dual module paths
- Add workspace dependencies to root Cargo.toml BEFORE spawning parallel agents — prevents Cargo.toml merge conflicts
- `cargo clippy --fix --allow-dirty` handles most lint auto-fixes; `private_interfaces` lint requires manual visibility adjustment
- Layered config (CLI > config file > env > defaults) via clap `env` attribute + `#[serde(default)]` TOML structs is a clean pattern with zero boilerplate
- Cross-session work MUST use `docs/tasks/uc-NNN-tasks.md` for progress tracking — task files are external memory that survives context kills and session boundaries
- When continuing from a prior session, read task files FIRST before examining code — avoids wasting 20-30% of context budget on archaeology
- Apply all related edits to a file atomically to avoid linter revert thrashing — plan edits first, then apply in one pass
- Always run `/uc-review` and fix issues BEFORE `/task-decompose` — review catches fabricated examples, broken references, and impossible postconditions that waste implementation time
- Use background subagents for read-only analysis (review, decompose) while lead does housekeeping — parallelize analysis, serialize execution
- Don't create worktrees speculatively — only when you're about to write code on a feature branch. Orphaned worktrees are a recurring anti-pattern
- The 20 tool-call guideline is per-task, not per-UC — a UC with 12 tasks at ~6 calls each is fine
- Update docs (UC registry, sprint doc, backlog) as part of the sprint commit, not as a separate cleanup task — doc debt compounds silently
- `cargo deny check` is now part of the quality gate; `deny.toml` at workspace root defines license allowlist and duplicate skip list
- Run `/session-handoff` before ending any session that has in-progress work — cross-session continuity prevents 20-30% context waste on archaeology
- TCP proxy pattern for testing network failures — place a proxy between client and server, abort proxy tasks to simulate disconnect (causes immediate RST on both ends). `relay_handle.abort()` does NOT close existing WebSocket connections because axum handler tasks are independently spawned
- In async shared-state code, always try-then-fallback, never check-then-act (TOCTOU race). E.g., try `send_message()`, queue on failure — don't check `is_connected` then send
- Gate merge on reviewer approval — do not merge feature branch before review completes
- **CRITICAL**: Lead agent MUST use subagents (Task tool) for ALL implementation work — never write code directly. No exceptions, not even for "small" or "quick" changes. Subagents get dedicated context windows, can be parallelized, and prevent the lead's context from bloating. If you are the Lead and about to use Edit/Write on a .rs file, STOP and spawn a subagent instead
- **CRITICAL**: Always spawn a reviewer agent alongside builder agents in multi-agent workflows — reviewer runs blind reviews against UC postconditions as each track completes, and integration gate is blocked on all reviews passing. Never merge without reviewer approval
