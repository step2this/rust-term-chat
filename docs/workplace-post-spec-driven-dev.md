# Spec-Driven Development with AI Agent Swarms: What 25K Lines of Rust in 3 Days Taught Me About Verification

I've been running an experiment: building a real Rust application (encrypted terminal messenger — QUIC P2P, WebSocket relay fallback, Noise XX E2E encryption, ratatui TUI) using Claude Code's new multi-agent swarm feature. The codebase hit 25K lines across 3 crates with 715 tests in about 3 calendar days.

The interesting finding isn't the speed. It's what made it reliable — and it has nothing to do with which model you use.

**The core problem: AI agents fail at scale without verification contracts**

The default workflow most people use with coding agents is something like: describe what you want, let the agent write code, hope it works. This breaks down fast. Ambiguity kills agents — "build a chat app with E2E encryption" has infinite interpretations. There's no way for the agent to know when it's actually done. And once you exceed a single context window or need parallel work, things fall apart completely.

The community is converging on PRDs + task decomposition as the fix. I think we can do better.

**Cockburn use cases as verification contracts**

I adapted Alistair Cockburn's fully-dressed use case format (the same methodology behind the UML use case standard) specifically for AI agent consumption. The key insight: his format already has everything agents need to self-verify.

- **Preconditions** become setup assertions in tests
- **Postconditions** become verification gates — the agent knows it's done when these pass
- **Extension conditions** ("what could go wrong at step N?") systematically surface edge cases. These produce ~40% of implementation tasks. Skip them and you're writing bugs you'll find later
- **Invariants** become continuous assertions the agent must never violate

I built this into a system of slash commands in Claude Code (11 commands, 3 skills, automated quality gates). The workflow is: `/uc-create` (write the spec) → `/uc-review` (devil's advocate review) → `/task-decompose` (generate task graph) → implement → `/verify-uc` (run postcondition checks) → `/grade-work` (score against acceptance criteria).

The review step alone (`/uc-review`) caught 7 issues in 2 minutes during one sprint. Each would have cost 5-10 minutes if discovered during implementation. That's a ~12x ROI on 2 minutes of work.

**This is model-agnostic.** The verification contracts are just structured markdown. The quality gates are just `cargo fmt + clippy + test + cargo-deny`. Any sufficiently capable model can execute against them. The discipline is in the process, not the model.

**Multi-agent swarms: what actually works**

Claude Code now has a built-in agent teams feature (Opus 4.6 + experimental flag). You can spawn multiple agents that coordinate through shared task lists and message passing. Here's what the team structure looked like:

```
Lead (coordinates, never writes code)
├── Builder-Proto    (owns: wire protocol crate, chat module)
├── Builder-Agent    (owns: agent bridge module)
├── Builder-Infra    (owns: crypto, transport modules)
├── Builder-TUI      (owns: UI, app state)
└── Reviewer         (owns: integration + property tests)
```

The critical constraint: **exclusive file ownership**. No two builders touch the same file. Each works on a git worktree (isolated copy of the repo on a feature branch). The lead coordinates via messages, never edits builder-owned files directly.

Result across 3 parallel feature branches: exactly 1 merge conflict. That's the file ownership matrix working.

Some reliability rules we learned the hard way:
- **~20 tool calls max per task.** Beyond this, agents lose coherence. Decompose further
- **Force plan-before-implement.** Agents must present a plan before writing code
- **Run linting per-task, not just at the final gate.** Warnings accumulate across parallel tracks
- **Define shared types before spawning builders.** Everyone codes against the same contract
- **Reviewer is non-negotiable.** Blind review against postconditions catches real bugs. Gate merges on reviewer approval

**The numbers**

| | |
|---|---|
| Lines of Rust | 25,170 |
| Tests | 715 passing |
| Use cases completed | 17 |
| Calendar days | 3 |
| Commits | 46 (89% AI co-authored) |
| UC implementation bugs | 0 |
| Reverts | 0 |
| Merge conflicts | 1 |

Of 13 "Implement UC" commits, zero required follow-up bug-fix commits. The 3 fix commits in the repo addressed lint config, a planned dependency migration, and documentation gaps — not logic bugs.

The A/B comparison is stark: same scope, same model, same developer. Sprint 8 (no verification workflow) scored 2.4/5 on quality with multiple rework cycles. Sprint 9 (full workflow) scored 4.6/5 with zero rework. The only variable was process discipline.

**The takeaway**

The gap between "AI writes some code" and "AI reliably builds production systems" is bridged by verification, not model capability. Structured specs with testable postconditions, automated quality gates, exclusive file ownership for parallel agents, and external memory (task files, handoff docs) that survives context window limits.

If you're using AI agents for anything beyond single-file edits, the highest-ROI investment is in your verification contracts — not in prompt engineering or model selection.

Happy to share the slash command definitions and template if anyone wants to adapt this for their team's workflow.
