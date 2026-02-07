# TermChat: Project Blueprint

## Vision

**Two interlocking projects, one feedback loop.**

**Layer 1 — "The Forge"**: A Cockburn-inspired requirements, tracking, and quality assurance system built as Claude Code skills, slash commands, agent team configurations, and hooks. This is how we keep AI agents honest, on-track, and producing verified work.

**Layer 2 — "The Sword"**: A Rust/Ratatui TUI messenger called **TermChat** — real-time chat embedded in your terminal for people who live in tmux. Hybrid P2P + relay architecture, E2E encryption, AI agent participants, and team coordination features.

Layer 1 gets built first and then used to build Layer 2. The messenger becomes the proving ground for the tooling, and pain points in building it feed back into improving the tooling.

---

## Part 1: The Forge — Claude Code Meta-Tooling

### 1.1 Philosophy: Why Cockburn + Claude Code?

Alistair Cockburn's use case format solves the exact problem that plagues AI-assisted development: **ambiguity kills agents**. His key insights:

- **Extension conditions catch the 80%** — the "little niggling things" that consume most development time are captured by systematically asking "what could go wrong at each step?"
- **Goal-level hierarchy** — summary goals (cloud level), user goals (sea level), and subfunctions (fish level) map perfectly to agent team delegation. The lead gets cloud-level; teammates get sea-level tasks; subagents handle fish-level.
- **Preconditions and postconditions** are essentially assertions — they become test cases and verification gates for agents.
- **Fill in iteratively, not all at once** — Cockburn explicitly says the template is too long to complete in one pass. This maps to iterative refinement with AI.

The community is converging on PRDs + TaskMaster as the standard. We can do better by combining:
- Cockburn's structured use case rigor (especially extensions and postconditions)
- TaskMaster-style task decomposition and dependency tracking
- Agent Teams for parallel execution with review gates
- Spec Driven Development's separation of planning from execution

### 1.2 The Cockburn-for-Agents Use Case Template

Adapted from Cockburn's fully-dressed template, optimized for AI agent consumption:

```markdown
# Use Case: UC-<number> <Active Verb Phrase Goal>

## Classification
- **Goal Level**: ☁️ Summary | 🌊 User Goal | 🐟 Subfunction
- **Scope**: System (black box) | Component (white box)
- **Priority**: P0 Critical | P1 High | P2 Medium | P3 Low
- **Complexity**: 🟢 Low | 🟡 Medium | 🔴 High | ⚫ Spike needed

## Actors
- **Primary Actor**: <who initiates>
- **Supporting Actors**: <systems, services, other users involved>
- **Stakeholders & Interests**:
  - <Stakeholder>: <what they care about>

## Conditions
- **Preconditions** (must be true before starting):
  1. <condition — becomes a setup assertion>
- **Success Postconditions** (true when done right):
  1. <condition — becomes a verification assertion>
- **Failure Postconditions** (true when it fails gracefully):
  1. <condition — becomes a failure-mode test>
- **Invariants** (must remain true throughout):
  1. <condition — becomes a continuous assertion>

## Main Success Scenario
1. <Actor> <does something>
2. System <responds/validates/transforms>
3. ...
n. <Success postcondition is achieved>

## Extensions (What Can Go Wrong)
- **2a. <condition at step 2>**:
  1. System <handles it>
  2. <returns to step X | use case fails>
- **3a. <condition at step 3>**:
  1. ...

## Variations
- **1a.** <Actor> may <alternative approach> → <different path>

## Agent Execution Notes
- **Verification Command**: `<shell command to verify postconditions>`
- **Test File**: `<path to test that validates this use case>`
- **Depends On**: UC-<n>, UC-<m>
- **Blocks**: UC-<x>, UC-<y>
- **Estimated Complexity**: <T-shirt size> / <token budget hint>
- **Agent Assignment**: Lead | Teammate:<role> | Subagent

## Acceptance Criteria (for grading)
- [ ] All success postconditions verified by automated test
- [ ] All extension paths have explicit handling
- [ ] No invariant violations detected
- [ ] Code passes lint + clippy (for Rust)
- [ ] Reviewer agent approves
```

### 1.3 Slash Commands to Build

These go in `.claude/commands/` as markdown files:

| Command | Purpose | Notes |
|---------|---------|-------|
| `/uc-create` | Interactive use case creation using Cockburn template | Asks clarifying questions, scores completeness |
| `/uc-review` | Reviews a use case doc for gaps | Checks extensions coverage, postcondition testability |
| `/prd-from-usecases` | Generates a PRD by synthesizing multiple use cases | Groups by goal level, builds dependency graph |
| `/task-decompose` | Breaks a use case into implementable tasks | Maps MSS steps → tasks, extensions → additional tasks |
| `/agent-team-plan` | Designs an agent team config for a set of tasks | Assigns roles, sets up shared task list, defines review gates |
| `/verify-uc` | Runs postcondition checks for a completed use case | Executes verification commands, runs tests, reports |
| `/grade-work` | Evaluates completed work against acceptance criteria | Scoring rubric, automated + subjective checks |
| `/retrospective` | After completing a milestone, captures what worked | Feeds improvements back into CLAUDE.md and skills |

### 1.4 Agent Team Configurations

#### The Requirements Team
```
Lead: Requirements Architect
  - Owns the use case document
  - Asks clarifying questions
  - Scores completeness

Teammate 1: Devil's Advocate
  - Reads each use case and tries to find missing extensions
  - Asks "what if X fails?" for every step
  - Challenges assumptions in preconditions

Teammate 2: Test Designer
  - Reads postconditions and writes test skeletons
  - Identifies untestable requirements
  - Proposes verification commands

Teammate 3: Architecture Scout
  - Researches technical feasibility
  - Identifies crate choices and trade-offs
  - Flags complexity risks
```

#### The Implementation Team
```
Lead: Implementation Coordinator
  - Manages shared task list
  - Routes work to teammates
  - Monitors for conflicts

Teammate 1: Builder
  - Writes the actual Rust code
  - Follows TDD: test first, then implement
  - Commits after each use case completion

Teammate 2: Reviewer
  - Reviews Builder's code against use case postconditions
  - Runs clippy, tests, checks invariants
  - Sends feedback via agent messaging

Teammate 3: Documentation
  - Keeps README, architecture docs, and CLAUDE.md updated
  - Documents decisions and trade-offs
  - Maintains the use case registry
```

#### The Grading Pipeline (Quality Gates)
```
Gate 1: Lint + Format (automated hook)
  - cargo fmt --check
  - cargo clippy -- -D warnings

Gate 2: Test Suite (automated hook)
  - cargo test
  - Coverage threshold check

Gate 3: Use Case Verification (agent)
  - Run verification commands from use case
  - Check all postconditions

Gate 4: Blind Review (separate agent context)
  - Fresh agent reads ONLY the use case + the code
  - Does not see implementation history
  - Grades against acceptance criteria

Gate 5: Integration Check (agent)
  - Verify no invariant violations across use cases
  - Check dependency graph consistency
  - Run integration tests
```

### 1.5 Hooks for Automated Quality

Claude Code hooks (in `.claude/hooks/`) fire automatically:

```jsonc
// .claude/hooks/pre-commit.json
{
  "event": "pre_commit",
  "command": "cargo fmt --check && cargo clippy -- -D warnings && cargo test"
}

// .claude/hooks/task-completed.json  
{
  "event": "TaskCompleted",  // Agent Teams hook
  "command": "scripts/verify-postconditions.sh",
  "exit_code_2_feedback": "Postcondition verification failed — see output"
}
```

### 1.6 CLAUDE.md Structure

```markdown
# TermChat Project

## Architecture
@docs/architecture.md

## Use Case Registry
@docs/use-cases/README.md

## Current Sprint
@docs/sprints/current.md

## Coding Standards
- Rust edition 2024
- All public functions must have doc comments
- No unwrap() in production code — use proper error handling
- Error types use thiserror
- Async runtime: tokio
- TUI framework: ratatui + crossterm
- Networking: tokio + quinn (QUIC) for P2P, tungstenite for relay
- Crypto: noise-protocol for E2E, x25519-dalek for key exchange
- Serialization: serde + bincode for wire format, serde_json for config

## Agent Instructions
- Always check the use case before implementing
- Run verification commands after completing any task
- If a postcondition cannot be verified, flag it — do not mark complete
- Commit after each completed use case, not after each file change

## Test Strategy
- Unit tests: inline in each module
- Integration tests: tests/ directory, one file per use case
- Property tests: proptest for protocol serialization round-trips
```

---

## Part 2: The Sword — TermChat Architecture

### 2.1 What Is TermChat?

A terminal-native messenger for developers who live in tmux. Think "Slack, but it's a TUI panel in your terminal multiplexer." 

Key differentiators:
- **Terminal-first**: Ratatui rendering, keyboard-driven, works over SSH
- **Hybrid networking**: P2P (QUIC/UDP hole-punching) when peers are reachable, lightweight relay server as fallback
- **E2E encrypted**: Noise protocol (XX handshake pattern), perfect forward secrecy
- **AI-native**: Claude Code agents can join as chat participants, post status updates, ask for human input
- **Team coordination**: Shared task lists, status updates, and notification channels embedded in the chat UX

### 2.2 High-Level Architecture

```
┌─────────────────────────────────────────────────┐
│                  TermChat TUI                    │
│  ┌───────────┐ ┌──────────────┐ ┌────────────┐  │
│  │ Sidebar   │ │  Chat Panel  │ │ Task Panel │  │
│  │ - Rooms   │ │  - Messages  │ │ - Tasks    │  │
│  │ - DMs     │ │  - Input     │ │ - Status   │  │
│  │ - Agents  │ │  - Reactions │ │ - Agents   │  │
│  └───────────┘ └──────────────┘ └────────────┘  │
├─────────────────────────────────────────────────┤
│              Application Layer                   │
│  ┌──────────┐ ┌──────────┐ ┌─────────────────┐  │
│  │ Chat     │ │ Task     │ │ Agent Bridge    │  │
│  │ Manager  │ │ Manager  │ │ (Claude ↔ Chat) │  │
│  └──────────┘ └──────────┘ └─────────────────┘  │
├─────────────────────────────────────────────────┤
│              Transport Layer                     │
│  ┌──────────────┐  ┌────────────────────────┐   │
│  │ P2P Engine   │  │ Relay Client           │   │
│  │ (QUIC/quinn) │  │ (WebSocket/tungstenite)│   │
│  │ hole-punch   │  │ fallback               │   │
│  └──────────────┘  └────────────────────────┘   │
├─────────────────────────────────────────────────┤
│              Crypto Layer                        │
│  ┌───────────────────────────────────────────┐   │
│  │ Noise Protocol (XX pattern)               │   │
│  │ x25519 key exchange, ChaCha20-Poly1305    │   │
│  │ Per-session ephemeral keys, PFS           │   │
│  └───────────────────────────────────────────┘   │
└─────────────────────────────────────────────────┘

         ┌─────────────────────┐
         │   Relay Server      │
         │   (lightweight)     │
         │   - Rust/axum       │
         │   - Store & forward │
         │   - NAT traversal   │
         │   - Never sees      │
         │     plaintext       │
         └─────────────────────┘
```

### 2.3 Module Breakdown

```
termchat/
├── Cargo.toml
├── CLAUDE.md
├── docs/
│   ├── architecture.md
│   ├── use-cases/
│   │   ├── README.md          # Use case registry
│   │   ├── uc-001-send-message.md
│   │   ├── uc-002-receive-message.md
│   │   ├── uc-003-establish-p2p.md
│   │   ├── uc-004-relay-fallback.md
│   │   ├── uc-005-e2e-handshake.md
│   │   ├── uc-006-create-room.md
│   │   ├── uc-007-agent-join.md
│   │   └── ...
│   └── sprints/
│       └── current.md
├── src/
│   ├── main.rs
│   ├── app.rs               # Application state machine
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── chat_panel.rs     # Message display + input
│   │   ├── sidebar.rs        # Room/DM/agent list
│   │   ├── task_panel.rs     # Shared task list view
│   │   ├── status_bar.rs     # Connection status, typing indicators
│   │   └── theme.rs          # Colors, styles
│   ├── chat/
│   │   ├── mod.rs
│   │   ├── message.rs        # Message types, formatting
│   │   ├── room.rs           # Room management
│   │   └── history.rs        # Local message store (SQLite)
│   ├── transport/
│   │   ├── mod.rs
│   │   ├── p2p.rs            # QUIC-based P2P connections
│   │   ├── relay.rs          # WebSocket relay client
│   │   ├── discovery.rs      # Peer discovery (mDNS + relay registry)
│   │   └── hybrid.rs         # Transport selection logic
│   ├── crypto/
│   │   ├── mod.rs
│   │   ├── noise.rs          # Noise protocol handshake
│   │   ├── keys.rs           # Key management, identity
│   │   └── store.rs          # Encrypted key storage
│   ├── agent/
│   │   ├── mod.rs
│   │   ├── bridge.rs         # Claude Code ↔ TermChat protocol
│   │   ├── participant.rs    # Agent as chat participant
│   │   └── commands.rs       # /ask-agent, /agent-status, etc.
│   ├── tasks/
│   │   ├── mod.rs
│   │   ├── task.rs           # Task data model
│   │   ├── sync.rs           # Task list synchronization
│   │   └── display.rs        # Task rendering
│   └── config/
│       ├── mod.rs
│       └── settings.rs       # User config, key paths, relay URLs
├── termchat-relay/            # Separate binary for the relay server
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── relay.rs
│       └── store.rs           # Ephemeral message queue
├── termchat-proto/            # Shared protocol definitions
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── message.rs         # Wire format definitions
│       ├── handshake.rs       # Protocol handshake types
│       └── task.rs            # Task sync protocol
└── tests/
    ├── integration/
    │   ├── send_receive.rs
    │   ├── p2p_connection.rs
    │   ├── relay_fallback.rs
    │   └── e2e_encryption.rs
    └── property/
        └── serialization.rs
```

### 2.4 Rust Learning Path (Integrated with Project)

Since you're new to Rust, the use cases are deliberately sequenced to introduce concepts incrementally:

| Sprint | Use Cases | Rust Concepts Learned |
|--------|-----------|----------------------|
| **Sprint 0: Hello Ratatui** | Basic TUI with hardcoded messages | Ownership, borrowing, structs, enums, match, cargo basics, ratatui widgets |
| **Sprint 1: Local Chat** | UC-001 Send, UC-002 Receive (localhost only) | Tokio async, channels (mpsc), serde serialization, error handling with `Result` |
| **Sprint 2: Crypto Foundation** | UC-005 E2E Handshake | Traits, generics, the `noise-protocol` crate, byte manipulation, `Vec<u8>` |
| **Sprint 3: P2P Networking** | UC-003 Establish P2P | Tokio networking, quinn (QUIC), futures, pinning, `Arc<Mutex<>>` |
| **Sprint 4: Relay Fallback** | UC-004 Relay Fallback | WebSockets, state machines, enum-based transport abstraction, trait objects |
| **Sprint 5: Rooms & History** | UC-006 Create Room | SQLite (rusqlite), lifetimes, iterators, the builder pattern |
| **Sprint 6: Agent Integration** | UC-007 Agent Join | Process spawning, IPC (Unix sockets or named pipes), serde_json, protocol design |
| **Sprint 7: Task Coordination** | UC-008 Shared Tasks | CRDT basics for conflict-free sync, more complex state management |
| **Sprint 8: Polish & Ship** | Cross-cutting concerns | Logging (tracing), config (toml), CLI args (clap), packaging, CI/CD |

### 2.5 Example Use Case: UC-001 Send Direct Message

```markdown
# Use Case: UC-001 Send Direct Message

## Classification
- **Goal Level**: 🌊 User Goal
- **Scope**: System (black box)
- **Priority**: P0 Critical
- **Complexity**: 🟡 Medium

## Actors
- **Primary Actor**: Sender (terminal user)
- **Supporting Actors**: Transport Layer, Crypto Layer, Recipient
- **Stakeholders & Interests**:
  - Sender: message is delivered, confirmed, and encrypted
  - Recipient: message arrives intact, sender is authenticated
  - System: no plaintext ever touches the network

## Conditions
- **Preconditions**:
  1. Sender has a valid identity keypair
  2. Sender and Recipient have completed E2E handshake (UC-005)
  3. At least one transport (P2P or relay) is available
- **Success Postconditions**:
  1. Message is stored in Sender's local history
  2. Message is delivered to Recipient (or queued at relay)
  3. Sender sees delivery confirmation in UI
  4. Message was encrypted in transit (never plaintext on wire)
- **Failure Postconditions**:
  1. Sender sees clear error message
  2. Message is saved locally as "pending"
  3. System will retry when transport is available
- **Invariants**:
  1. Plaintext message never leaves the application boundary
  2. Message ordering is preserved per-conversation

## Main Success Scenario
1. Sender types message in input box and presses Enter
2. System validates message (non-empty, within size limit)
3. System serializes message with metadata (timestamp, sender ID, room ID)
4. System encrypts serialized message using established Noise session
5. System selects best available transport (P2P preferred, relay fallback)
6. System transmits encrypted payload
7. System receives delivery acknowledgment
8. System updates local history with "delivered" status
9. System renders delivery confirmation in UI

## Extensions
- **2a. Message exceeds size limit**:
  1. System shows error: "Message too long (max 64KB)"
  2. Sender edits message, returns to step 1
- **4a. No Noise session established**:
  1. System initiates E2E handshake (UC-005)
  2. On success, returns to step 4
  3. On failure, shows error, saves as "pending"
- **5a. P2P connection fails**:
  1. System falls back to relay transport
  2. Returns to step 6
- **5b. No transport available (offline)**:
  1. System saves message as "pending" in local history
  2. System shows "queued — will send when connected"
  3. Use case ends (retry handled by background process)
- **7a. No acknowledgment within timeout (10s)**:
  1. System retries once
  2. If still no ack, marks as "sent" (not "delivered")
  3. System shows "sent but unconfirmed" in UI

## Variations
- **1a.** Sender may paste multiline text → message includes newlines
- **1b.** Sender may use /commands → routed to command handler, not sent as message

## Agent Execution Notes
- **Verification Command**: `cargo test --test send_receive`
- **Test File**: `tests/integration/send_receive.rs`
- **Depends On**: UC-005 (E2E Handshake)
- **Blocks**: UC-007 (Agent Join — agents send messages too)
- **Estimated Complexity**: Medium / ~2000 tokens per agent turn
- **Agent Assignment**: Teammate:Builder

## Acceptance Criteria
- [ ] `cargo test --test send_receive` passes
- [ ] Message round-trips between two local instances
- [ ] Wire capture shows only encrypted bytes (no plaintext)
- [ ] Offline message queuing works (send while disconnected, delivers on reconnect)
- [ ] UI shows sent → delivered status transition
- [ ] cargo clippy -- -D warnings passes
- [ ] All public functions have doc comments
```

---

## Part 3: The Roadmap

### Phase 0: Forge Setup (Week 1)
**Goal**: Layer 1 is functional — Claude Code skills, commands, and agent configs are ready.

- [ ] Create `.claude/commands/` with all slash commands from §1.3
- [ ] Create `.claude/skills/` with Cockburn template and grading rubrics
- [ ] Configure agent team definitions for Requirements and Implementation teams
- [ ] Set up hooks for automated quality gates
- [ ] Write CLAUDE.md with project standards
- [ ] Write the first 3 use cases (UC-001, UC-002, UC-005) using the tooling
- [ ] **Meta-verification**: Use the Requirements Team agent config to review your own use cases

### Phase 1: Hello Ratatui (Week 2)
**Goal**: A TUI window with hardcoded messages renders in the terminal. You can type and see your text. Nothing networked yet.

- [ ] `cargo init termchat`
- [ ] Basic ratatui app with main loop, event handling
- [ ] Three-panel layout (sidebar, chat, task panel)
- [ ] Input box with cursor, message submission
- [ ] Message list with scrolling
- [ ] Keyboard navigation (Tab between panels, vim-style scrolling)
- [ ] **Rust learning checkpoint**: Ownership, borrowing, structs, enums, pattern matching

### Phase 2: Local Chat over Localhost (Week 3-4)
**Goal**: Two instances of TermChat on the same machine can exchange messages.

- [ ] Define wire protocol in `termchat-proto`
- [ ] Tokio TCP listener + connector
- [ ] Async message send/receive with mpsc channels
- [ ] Message serialization (serde + bincode)
- [ ] Basic error handling (thiserror)
- [ ] **Rust learning checkpoint**: Async/await, tokio runtime, channels, serde, error types

### Phase 3: E2E Encryption (Week 5-6)
**Goal**: All messages are encrypted with the Noise protocol. Key identity established.

- [ ] Identity keypair generation and storage
- [ ] Noise XX handshake implementation
- [ ] Encrypt/decrypt pipeline in transport
- [ ] Key verification UX (safety numbers or similar)
- [ ] **Rust learning checkpoint**: Traits, generics, byte manipulation, crate integration

### Phase 4: Hybrid Networking (Week 7-9)
**Goal**: P2P over QUIC when possible, relay server fallback.

- [ ] QUIC transport with quinn
- [ ] Relay server (termchat-relay) with axum + WebSocket
- [ ] NAT traversal / hole-punching via relay coordination  
- [ ] Transport abstraction (trait object: P2P or Relay behind same interface)
- [ ] Automatic fallback logic
- [ ] Peer discovery (mDNS for LAN, relay registry for WAN)
- [ ] **Rust learning checkpoint**: Trait objects, dynamic dispatch, state machines, network programming

### Phase 5: Rooms, History & Polish (Week 10-11)
**Goal**: Multi-user rooms, persistent history, proper UX.

- [ ] Room creation/joining
- [ ] SQLite message history
- [ ] Message search
- [ ] Typing indicators
- [ ] Presence (online/offline/away)
- [ ] Notifications (terminal bell, optional desktop)
- [ ] Config file support (TOML)
- [ ] CLI args with clap

### Phase 6: Agent Integration & Tasks (Week 12-14)
**Goal**: Claude Code agents can participate in chat. Shared task lists.

- [ ] Agent bridge protocol (Unix socket or named pipe)
- [ ] Agent as chat participant (posts messages, reads context)
- [ ] /ask-agent command in chat
- [ ] Shared task list with CRDT-based sync
- [ ] Task panel rendering
- [ ] Agent status updates in sidebar

### Phase 7: Ship It (Week 15-16)
**Goal**: Installable, documented, tested, and released.

- [ ] GitHub repo with CI (cargo test, clippy, fmt)
- [ ] `cargo install` support
- [ ] Homebrew formula
- [ ] README with screenshots/GIFs
- [ ] Architecture documentation
- [ ] Retrospective: what worked in the meta-tooling

---

## Part 4: Key Decisions & Trade-offs

### Why QUIC (quinn) over plain TCP?
QUIC gives us UDP-based transport (better for P2P hole-punching), built-in encryption (TLS 1.3), multiplexed streams (multiple conversations over one connection), and connection migration (handles network changes). The downside is complexity — quinn is a serious crate. But for a learning project, this is a feature, not a bug.

### Why Noise Protocol over TLS for E2E?
TLS protects transport. Noise protects messages end-to-end regardless of transport. Even when messages pass through the relay server, the relay never sees plaintext. The XX handshake pattern provides mutual authentication and forward secrecy. This is the same approach Signal uses.

### Why not Matrix?
Matrix is a fantastic protocol, but implementing a compliant client is a project unto itself. Building our own protocol from scratch teaches more and keeps the scope focused on the learning goals. We could add Matrix bridge support later as a stretch goal.

### Why Ratatui over Cursive?
Ratatui is the actively-maintained successor to tui-rs, has a larger ecosystem of widgets, better documentation, and more community support. Its immediate-mode rendering model is also more intuitive for someone coming from React.

### Wire Format: Bincode vs MessagePack vs Protobuf?
Bincode for now — it's the simplest, fastest, and most Rust-native option. Zero-copy deserialization is possible. If we need cross-language interop later (e.g., for the relay server in a different language), we can switch to MessagePack. Protobuf adds a build step with `prost` that's unnecessary complexity at this stage.

---

## Part 5: Getting Started Checklist

### Prerequisites
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Claude Code (if not already)
npm install -g @anthropic-ai/claude-code

# Enable Agent Teams
# Add to ~/.claude/settings.json:
# { "env": { "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" } }

# Verify
rustc --version
cargo --version
claude --version
```

### Day 1 Actions
1. Create the repo: `cargo init termchat && cd termchat`
2. Initialize Claude Code: `claude` → `/init`
3. Set up `.claude/commands/` with the slash commands from §1.3
4. Write the first use case (UC-001) using `/uc-create`
5. Have the Requirements Team agent config review it
6. Start Sprint 0: Hello Ratatui

---

*This document is the seed. It will evolve as we build. Every retrospective feeds improvements back into both the tooling and this blueprint.*
