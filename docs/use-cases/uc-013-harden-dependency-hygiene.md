# Use Case: UC-013 Harden Dependency Hygiene

## Classification
- **Goal Level**: :ocean: User Goal
- **Scope**: System (black box)
- **Priority**: P1 High
- **Complexity**: :yellow_circle: Medium

## Actors
- **Primary Actor**: Developer (maintainer preparing for release)
- **Supporting Actors**: cargo-deny (advisory scanner), Cargo resolver, crates.io registry
- **Stakeholders & Interests**:
  - Maintainer: all dependencies are audited, no known vulnerabilities, license-compliant
  - End User: installed binary contains no unmaintained or vulnerable transitive deps
  - CI Pipeline: automated gate catches future regressions

## Conditions
- **Preconditions** (must be true before starting):
  1. Workspace compiles (`cargo check --workspace` passes)
  2. `cargo-deny` is installed (`cargo install cargo-deny`)
  3. Current advisory database is accessible (network available)
- **Success Postconditions** (true when done right):
  1. `cargo deny check` passes with zero errors (advisories, bans, licenses, sources)
  2. `deny.toml` exists at workspace root with explicit license allowlist
  3. Duplicate dependency split is audited; reducible duplicates eliminated, unavoidable duplicates documented in `deny.toml`
  4. No unmaintained crate advisories in `cargo deny check advisories`
  5. CRDT implementation is evaluated; decision documented (keep hand-rolled or adopt `crdts` crate)
  6. CI workflow runs `cargo deny check` on every push/PR
- **Failure Postconditions** (true when it fails gracefully):
  1. If an upstream dep cannot be upgraded (e.g., ratatui pins paste), the advisory is explicitly exempted in `deny.toml` with a comment explaining why
  2. If rand deduplication is blocked by a transitive dep, the duplicate is documented in `deny.toml` skip list
- **Invariants** (must remain true throughout):
  1. All existing 675+ tests continue to pass
  2. `cargo clippy --workspace -- -D warnings` remains clean
  3. No production behavior changes (this is infrastructure-only)

## Main Success Scenario
1. Developer runs `cargo deny check` to establish baseline failures
2. System reports baseline failures (advisories, license violations, duplicate versions)
3. Developer creates `deny.toml` at workspace root with license allowlist (Apache-2.0, MIT, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0, Zlib, BSL-1.0, MPL-2.0)
4. Developer addresses `paste` advisory: check if ratatui has a newer version dropping paste, or add explicit exemption with rationale
5. Developer audits `rand` version split: check all rand 0.8 consumers (direct dep, snow, x25519-dalek), upgrade direct dep if possible, document unavoidable transitive duplicates
6. Developer evaluates CRDT crate: compare `crdts` crate API against hand-rolled `tasks/merge.rs`, document decision in code comment in `merge.rs`
7. Developer adds `cargo deny check` step to `.github/workflows/ci.yml`
8. Developer runs full quality gate: `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace && cargo deny check`
9. System confirms: zero clippy warnings, 675+ tests passing, cargo-deny clean

## Extensions (What Can Go Wrong)
- **4a. ratatui has no version without paste**:
  1. Developer adds exemption to `deny.toml`: `[[advisories.ignore]]` with RUSTSEC ID and comment
  2. Returns to step 5
- **4b. Upgrading ratatui introduces breaking API changes**:
  1. Developer pins current ratatui version and exempts advisory
  2. Creates follow-up issue for ratatui upgrade
  3. Returns to step 5
- **5a. snow does not support rand 0.9**:
  1. Developer documents the duplicate in `deny.toml` skip section
  2. Checks snow release schedule / open PRs for rand 0.9 support
  3. Returns to step 6
- **5b. Upgrading rand breaks quinn or x25519-dalek**:
  1. Developer reverts rand changes
  2. Documents incompatibility
  3. Returns to step 6
- **6a. `crdts` crate has unacceptable dependency weight or API mismatch**:
  1. Developer documents decision to keep hand-rolled merge.rs
  2. Adds comment to `tasks/merge.rs` explaining the evaluation
  3. Returns to step 7
- **7a. cargo-deny is not available in CI runner**:
  1. Use `EmbarkStudios/cargo-deny-action@v2` GitHub Action instead of raw cargo-deny
  2. Returns to step 8
- **3a. A transitive dependency uses a license not in the initial allowlist**:
  1. Developer evaluates the license (e.g., MPL-2.0 is weak copyleft, acceptable; GPL is not)
  2. If acceptable, adds to allowlist in `deny.toml` with comment
  3. If not acceptable, finds an alternative crate or documents the risk
  4. Returns to step 4
- **8a. Quality gate fails on clippy after dependency changes**:
  1. Developer fixes clippy warnings introduced by version bumps
  2. Returns to step 8

## Variations
- **3a.** Developer may use `cargo deny init` to generate a starter `deny.toml` instead of writing from scratch
- **6a.** If `crdts` crate is adopted, the merge.rs tests become the acceptance criteria for the migration (all existing tests must pass with the new implementation)

## Out of Scope
- ChatManager refactor (separate UC)
- Production behavior changes
- New features
- Minimum Supported Rust Version (MSRV) policy

## Agent Execution Notes
- **Verification Command**: `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace && cargo deny check`
- **Test File**: No new test file — existing tests must continue passing
- **Depends On**: Sprint 8 completion (CI workflow `.github/workflows/ci.yml` exists)
- **Blocks**: None (hygiene UC)
- **Estimated Complexity**: M (Medium) — ~30 tool calls across 3 work packages
- **Agent Assignment**: Single agent or Lead + 1 Builder (small scope)

## Acceptance Criteria (for grading)
- [ ] `deny.toml` exists at workspace root with explicit license allowlist
- [ ] `cargo deny check` passes (or all failures have documented exemptions)
- [ ] CI workflow includes `cargo deny check` step
- [ ] Duplicate dependency count documented; rand split addressed or exempted
- [ ] CRDT evaluation decision documented in `tasks/merge.rs` header comment
- [ ] All 675+ existing tests still pass
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] No production behavior changes
