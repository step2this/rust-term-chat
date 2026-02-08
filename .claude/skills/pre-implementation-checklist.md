# Pre-Implementation Checklist

Reference this checklist before starting any implementation work. All "Required" items must be YES.

## Required (must be YES to proceed)

- [ ] **UC doc exists** — `docs/use-cases/uc-<NNN>-<slug>.md` is written
- [ ] **UC reviewed** — `/uc-review` has been run and issues fixed
- [ ] **Task file exists** — `docs/tasks/uc-<NNN>-tasks.md` is written via `/task-decompose`
- [ ] **Feature branch created** — NOT working on `main` directly
- [ ] **Handoff read** — if continuing from a prior session, `docs/handoff.md` has been read
- [ ] **Workspace deps added** — any new dependencies are in root `Cargo.toml` before spawning parallel agents

## Recommended

- [ ] **Worktree created** — `git worktree add ../dir -b feature/uc-NNN` for isolation
- [ ] **Agent team plan** — for complex (🔴/⚫) UCs, run `/agent-team-plan`
- [ ] **Reviewer assigned** — a reviewer agent is part of the team

## Sprint Completion

- [ ] **One commit per UC** — never bundle multiple UCs in one commit
- [ ] **Docs updated** — UC registry, sprint doc, and backlog reflect completed work
- [ ] **Retrospective written** — `/retrospective` run for the sprint
- [ ] **Handoff written** — `/session-handoff` run if work continues in another session
