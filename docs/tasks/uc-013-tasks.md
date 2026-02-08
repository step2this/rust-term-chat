# Tasks for UC-013: Harden Dependency Hygiene

Generated from use case on 2026-02-08.

## Summary
- **Total tasks**: 12
- **Implementation tasks**: 6
- **Prerequisite tasks**: 2
- **Verification tasks**: 3
- **Investigation tasks**: 1
- **Critical path**: T-013-01 -> T-013-02 -> T-013-03 -> T-013-05 -> T-013-07 -> T-013-09 -> T-013-10 -> T-013-12
- **Estimated total size**: M (Medium) -- ~25-30 tool calls, single agent feasible

## Current State (Baseline)

Before task decomposition, `cargo deny check` was run to establish the baseline. Key findings:

| Category | Status | Details |
|----------|--------|---------|
| Advisories | 2 exempted (passing) | RUSTSEC-2024-0436 (paste via ratatui), RUSTSEC-2023-0089 (atomic-polyfill via postcard) |
| Licenses | 1 failure | `option-ext@0.2.0` uses MPL-2.0 (transitive via dirs), not in allowlist |
| Bans | 18 warnings (duplicates) | rand 0.8/0.9 split, getrandom 0.2/0.3, thiserror 1/2, windows-sys/targets dups |
| Sources | ok | All from crates.io |
| Config location | Wrong | `deny.toml` is at `termchat/deny.toml`, not workspace root |
| rand split | Solvable | `rand = "0.8"` is a direct dep of termchat; all transitive deps use rand 0.9. Snow uses rand_core 0.6 only. Upgrading `rand` to 0.9 eliminates rand/rand_core/rand_chacha/getrandom duplicates. |

## Dependency Graph

```
T-013-01 (Prereq: Install cargo-deny, verify workspace compiles)
  |
  v
T-013-02 (Run baseline: cargo deny check, capture output)
  |
  +--- T-013-03 (Move deny.toml to workspace root, fix license allowlist)
  |      |
  |      +--- T-013-04 (Investigate: Audit rand 0.8/0.9 split, check snow compatibility)
  |      |      |
  |      |      v
  |      +--- T-013-05 (Upgrade rand 0.8 -> 0.9 in termchat/Cargo.toml, fix API changes)
  |      |      |
  |      |      v
  |      +--- T-013-06 (Evaluate crdts crate vs hand-rolled merge.rs, document decision)
  |      |
  |      +--- T-013-07 (Update deny.toml: document remaining duplicates in skip/skip-tree)
  |             |
  |             v
  |      T-013-08 (Address remaining advisories: verify exemptions are still needed)
  |             |
  |             v
  |      T-013-09 (Verify: cargo deny check passes clean with updated config)
  |             |
  |             v
  |      T-013-10 (Add cargo deny check to CI workflow)
  |             |
  |             v
  |      T-013-11 (Verify: full quality gate passes)
  |             |
  |             v
  |      T-013-12 (Final verification: all postconditions met)
```

## Tasks

### T-013-01: Verify prerequisites (workspace compiles, cargo-deny installed)
- **Type**: Prerequisite
- **Description**:
  - Verify `cargo check --workspace` passes
  - Verify `cargo-deny` is installed (run `cargo deny --version`)
  - If not installed, run `cargo install cargo-deny`
  - Verify advisory database is accessible: `cargo deny fetch`
- **From**: Preconditions 1, 2, 3
- **Depends On**: None
- **Blocks**: T-013-02
- **Size**: S
- **Risk**: Low
- **Acceptance Test**: `cargo check --workspace` and `cargo deny --version` both succeed

---

### T-013-02: Run cargo deny check baseline and capture failures
- **Type**: Prerequisite
- **Description**:
  - Run `cargo deny check` without config (raw baseline) and capture full output
  - Run `cargo deny check --config termchat/deny.toml` (with existing config) and capture output
  - Document the following in a structured summary:
    - Advisory failures (IDs, crate names, dependency paths)
    - License failures (license types, crate names)
    - Ban/duplicate warnings (crate names, version pairs)
    - Source issues (if any)
  - This establishes the "before" state for the UC
- **From**: MSS step 1, MSS step 2
- **Depends On**: T-013-01
- **Blocks**: T-013-03, T-013-04, T-013-06
- **Size**: S
- **Risk**: Low
- **Acceptance Test**: Baseline output captured; all failure categories identified

---

### T-013-03: Move deny.toml to workspace root and fix license allowlist
- **Type**: Implementation
- **Description**:
  - Move `termchat/deny.toml` to workspace root (`deny.toml`)
  - Add missing licenses to the `[licenses].allow` list:
    - `MPL-2.0` (used by `option-ext`, transitive via `dirs`). MPL-2.0 is a weak copyleft license that only applies to files containing MPL-licensed code, not to the whole project. It is safe to allow.
    - `Unlicense` (used by some transitive deps)
    - `BSL-1.0` (mentioned in UC allowlist)
    - `BSD-1-Clause` (used by `fiat-crypto` transitive dep)
  - Verify `cargo deny check licenses` passes with the updated config at workspace root
  - Note: `cargo deny check` auto-discovers `deny.toml` at the workspace root; no `--config` flag needed after this task
- **From**: MSS step 3, Extension 4a (license allowlist)
- **Depends On**: T-013-02
- **Blocks**: T-013-05, T-013-07, T-013-08
- **Size**: S
- **Risk**: Low
- **Acceptance Test**: `cargo deny check licenses` passes with zero errors

---

### T-013-04: Investigate rand 0.8/0.9 split and snow compatibility
- **Type**: Investigation
- **Description**:
  - Run `cargo tree -p rand@0.8.5 -i` to confirm only direct dep from termchat
  - Run `cargo tree -p rand@0.9.2 -i` to confirm transitive deps (quinn, tungstenite, proptest)
  - Run `cargo tree -p snow@0.9.6` to confirm snow uses rand_core 0.6 (not rand directly)
  - Check snow crate docs/changelog for rand 0.9 support status
  - Audit termchat source for `rand::` usage (known: `crypto/keys.rs` uses `OsRng`, `net.rs` uses `thread_rng().gen_range()`)
  - Determine if `rand` can be upgraded from 0.8 to 0.9 in `termchat/Cargo.toml`:
    - rand 0.9 moved `OsRng` from `rand::rngs::OsRng` to `rand::rngs::OsRng` (still available but may need import change)
    - `thread_rng()` was renamed to `rng()` in rand 0.9
    - `gen_range()` API unchanged
  - Document findings for T-013-05
- **From**: MSS step 5, Extensions 5a, 5b
- **Depends On**: T-013-02
- **Blocks**: T-013-05
- **Size**: S
- **Risk**: Medium (API changes may have ripple effects)
- **Acceptance Test**: Written analysis of feasibility; API changes enumerated

---

### T-013-05: Upgrade rand 0.8 to 0.9 in termchat/Cargo.toml
- **Type**: Implementation
- **Description**:
  - In `termchat/Cargo.toml`, change `rand = "0.8"` to `rand = "0.9"`
  - Update `termchat/src/net.rs`:
    - `rand::thread_rng()` -> `rand::rng()` (rand 0.9 API)
    - `use rand::Rng;` remains the same
  - Update `termchat/src/crypto/keys.rs`:
    - `use rand::rngs::OsRng;` -> verify still works in rand 0.9 (OsRng moved to `rand::rngs::OsRng` but re-exported)
  - Run `cargo build` to verify compilation
  - Run `cargo test --workspace` to verify no regressions
  - Run `cargo tree -d` to verify rand/getrandom/rand_core/rand_chacha duplicates eliminated
  - **Extension 5b**: If upgrading rand breaks x25519-dalek or snow:
    - Revert the rand change
    - Document incompatibility in `deny.toml` skip section
    - Proceed to T-013-07 with the duplicate documented
- **From**: MSS step 5, Postcondition 3
- **Depends On**: T-013-03, T-013-04
- **Blocks**: T-013-07
- **Size**: M
- **Risk**: Medium (API migration across 2 source files; snow/x25519-dalek compatibility unknown until tested)
- **Acceptance Test**: `cargo build` and `cargo test --workspace` pass; `cargo tree -d 2>&1 | grep rand` shows zero or reduced duplication

---

### T-013-06: Evaluate crdts crate vs hand-rolled merge.rs
- **Type**: Implementation
- **Description**:
  - Review the `crdts` crate (v7.3.2) for:
    - LWW register support (does it provide `LwwRegister<T>` or equivalent?)
    - API compatibility with current `merge_lww`, `merge_task`, `merge_task_list` functions
    - Dependency weight: how many transitive deps does `crdts` pull in?
    - Feature match: does `crdts` handle the exact tiebreaking semantics (timestamp then author)?
  - Review current `termchat/src/tasks/merge.rs` (87 lines of code + 333 lines of tests):
    - Already tested with 22 unit tests covering commutativity, associativity, idempotency
    - Simple, purpose-built, no external dependencies
  - Make and document the decision:
    - If `crdts` has acceptable weight and API match, note migration path
    - If `crdts` adds unnecessary complexity/deps, document rationale for keeping hand-rolled
  - Add a comment to `termchat/src/tasks/merge.rs` header documenting the evaluation
  - Document decision in commit message when committing
  - **Extension 6a**: If `crdts` has unacceptable dependency weight or API mismatch, keep hand-rolled merge.rs with documented rationale
- **From**: MSS step 6, Postcondition 5, Extension 6a
- **Depends On**: T-013-02
- **Blocks**: None (informational)
- **Size**: S
- **Risk**: Low (evaluation only, no code change unless adopting crdts)
- **Acceptance Test**: Decision documented in merge.rs header comment; rationale covers dependency weight, API fit, and test coverage

---

### T-013-07: Update deny.toml with documented duplicates in skip list
- **Type**: Implementation
- **Description**:
  - After rand upgrade (T-013-05), re-run `cargo tree -d` to identify remaining duplicates
  - For each remaining duplicate that cannot be eliminated:
    - Add entry to `[bans].skip` with `reason` explaining why
    - Expected remaining duplicates (if rand upgrade succeeds):
      - `thiserror` 1.x/2.x (thiserror 2 is our direct dep; some transitive deps still on 1.x)
      - `hashbrown` (version split across transitive deps)
      - `unicode-width` (version split across transitive deps)
      - `windows-sys`/`windows-targets` (version churn in windows ecosystem, Linux-only target anyway)
    - If rand upgrade failed (Extension 5a/5b), also document rand 0.8/0.9 split
  - Set `multiple-versions = "deny"` to catch future regressions, with all current known dups in `skip`
  - Run `cargo deny check bans` to verify clean
- **From**: MSS step 5 (documentation), Postcondition 3, Failure Postcondition 2
- **Depends On**: T-013-05
- **Blocks**: T-013-09
- **Size**: S
- **Risk**: Low
- **Acceptance Test**: `cargo deny check bans` passes with zero errors; all skipped duplicates have documented reasons

---

### T-013-08: Verify advisory exemptions are still valid
- **Type**: Verification
- **Description**:
  - Review the two existing exemptions in `deny.toml`:
    - `RUSTSEC-2024-0436` (paste, unmaintained, transitive via ratatui): Check if ratatui 0.29 or newer drops paste dependency. If so, the exemption can be removed.
    - `RUSTSEC-2023-0089` (atomic-polyfill, unmaintained, transitive via postcard -> heapless): Check if postcard has a newer version that drops heapless/atomic-polyfill. If so, upgrade postcard and remove exemption.
  - For each exemption that is still needed:
    - Verify the `reason` string is descriptive
    - Verify the advisory ID is correct
  - If any exemption can be removed (upstream fixed), update Cargo.toml deps and remove the exemption
  - Run `cargo deny check advisories` to verify clean
  - **Extension 4a**: If ratatui has no version without paste, keep exemption
  - **Extension 4b**: If upgrading ratatui introduces breaking API changes, keep exemption
- **From**: MSS step 4, Extensions 4a, 4b, Failure Postcondition 1
- **Depends On**: T-013-03
- **Blocks**: T-013-09
- **Size**: S
- **Risk**: Low (checking, not changing unless an easy upgrade exists)
- **Acceptance Test**: `cargo deny check advisories` passes; all exemptions either removed (fixed upstream) or documented with valid rationale

---

### T-013-09: Verify cargo deny check passes clean
- **Type**: Verification
- **Description**:
  - Run `cargo deny check` (no flags, no --config) from workspace root
  - Verify output shows: `advisories ok, bans ok, licenses ok, sources ok`
  - If any check fails, investigate and fix (return to appropriate task)
  - This is the gatekeeper for T-013-10 (CI integration)
- **From**: Postconditions 1, 4
- **Depends On**: T-013-07, T-013-08
- **Blocks**: T-013-10
- **Size**: S
- **Risk**: Low
- **Acceptance Test**: `cargo deny check` exits with code 0 and reports all checks passing

---

### T-013-10: Add cargo deny check to CI workflow
- **Type**: Implementation
- **Description**:
  - Edit `.github/workflows/ci.yml` to add a `cargo deny check` step
  - Use the `EmbarkStudios/cargo-deny-action@v2` GitHub Action (per Extension 7a) instead of raw `cargo-deny` CLI, as this handles installation and caching
  - Place the step after checkout and toolchain setup, but it can run in parallel with other checks
  - Alternatively, if keeping a single job, add it as a step after the existing "Run tests" step:
    ```yaml
    - name: Dependency audit
      uses: EmbarkStudios/cargo-deny-action@v2
    ```
  - Verify the workflow YAML is valid
- **From**: MSS step 7, Postcondition 6, Extension 7a
- **Depends On**: T-013-09
- **Blocks**: T-013-11
- **Size**: S
- **Risk**: Low
- **Acceptance Test**: `.github/workflows/ci.yml` contains cargo-deny step; YAML syntax is valid

---

### T-013-11: Run full quality gate
- **Type**: Verification
- **Description**:
  - Run the complete quality gate command:
    ```bash
    cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace && cargo deny check
    ```
  - Verify all 675+ existing tests still pass (Invariant 1)
  - Verify clippy is clean (Invariant 2)
  - Verify no production behavior changes (Invariant 3) -- this is guaranteed by the nature of the changes (config files, dependency versions, CI workflow only)
  - **Extension 8a**: If clippy fails after dependency version changes, fix the new warnings before proceeding
- **From**: MSS steps 8-9, Invariants 1-3, Extension 8a
- **Depends On**: T-013-10
- **Blocks**: T-013-12
- **Size**: S
- **Risk**: Low
- **Acceptance Test**: All four commands exit with code 0

---

### T-013-12: Final postcondition verification
- **Type**: Verification
- **Description**:
  - Verify all UC-013 postconditions:
    1. `cargo deny check` passes with zero errors -- run and confirm
    2. `deny.toml` exists at workspace root -- `ls deny.toml`
    3. Duplicate dependency count reduced -- compare `cargo tree -d` before/after
    4. No unmaintained crate advisories (or all exempted with rationale) -- `cargo deny check advisories`
    5. CRDT evaluation documented -- check merge.rs header comment
    6. CI workflow includes cargo deny check step -- check ci.yml
  - Verify all acceptance criteria from the UC are met
  - Summarize changes for commit message
- **From**: All postconditions, Acceptance Criteria
- **Depends On**: T-013-11
- **Blocks**: None
- **Size**: S
- **Risk**: Low
- **Acceptance Test**: All 6 postconditions verified; ready for commit

---

## Implementation Order

| Order | Task | Type | Size | Depends On | Description |
|-------|------|------|------|------------|-------------|
| 1 | T-013-01 | Prerequisite | S | none | Verify prerequisites |
| 2 | T-013-02 | Prerequisite | S | T-013-01 | Capture baseline |
| 3a | T-013-03 | Implementation | S | T-013-02 | Move deny.toml, fix license allowlist |
| 3b | T-013-04 | Investigation | S | T-013-02 | Audit rand version split |
| 3c | T-013-06 | Implementation | S | T-013-02 | Evaluate crdts crate |
| 4a | T-013-05 | Implementation | M | T-013-03, T-013-04 | Upgrade rand 0.8 -> 0.9 |
| 4b | T-013-08 | Verification | S | T-013-03 | Verify advisory exemptions |
| 5 | T-013-07 | Implementation | S | T-013-05 | Document remaining duplicates |
| 6 | T-013-09 | Verification | S | T-013-07, T-013-08 | Verify cargo deny clean |
| 7 | T-013-10 | Implementation | S | T-013-09 | Add to CI workflow |
| 8 | T-013-11 | Verification | S | T-013-10 | Full quality gate |
| 9 | T-013-12 | Verification | S | T-013-11 | Final postcondition check |

## Notes for Agent Team Coordination

### Single-Agent Recommended

This UC follows established patterns (config files, dependency management, CI workflow) and has no novel/high-complexity work. Per CLAUDE.md guidance: "Single-agent implementation is sufficient for medium-complexity UCs that follow established patterns." All tasks are sequential with minimal parallelism opportunity.

### Parallelism Opportunities

Tasks 3a/3b/3c can run in parallel (independent investigations/changes):
- T-013-03 (move deny.toml + licenses) is independent of T-013-04 (rand audit) and T-013-06 (crdts eval)
- However, T-013-05 depends on both T-013-03 and T-013-04

### Key Risk Points

1. **T-013-05 (rand upgrade) is the highest-risk task**. The rand 0.8 -> 0.9 migration changes two APIs:
   - `thread_rng()` -> `rng()` (renamed in rand 0.9)
   - `OsRng` import path may change
   - Snow uses rand_core 0.6 (compatible with rand 0.8 only); however snow does not use `rand` directly, so the getrandom 0.2/0.3 split may persist even after upgrading. This needs verification.
   - x25519-dalek uses rand_core 0.6 as well, which may keep the getrandom split alive.
   - Fallback (Extension 5a): If the split cannot be fully resolved, document the remaining duplicates in deny.toml skip list.

2. **T-013-03 (license allowlist)**: The MPL-2.0 addition for `option-ext` is safe (weak copyleft, file-level only), but the decision should be documented.

3. **T-013-08 (advisory exemptions)**: Both exemptions are for unmaintained transitive deps with no upstream fix available. They will almost certainly need to stay, but should be verified.

### Module Ownership

All changes in this UC are infrastructure-only (no production code changes except the rand API migration in 2 files):
- `deny.toml` (new location at workspace root)
- `termchat/Cargo.toml` (rand version bump)
- `termchat/src/net.rs` (1 line: thread_rng -> rng)
- `termchat/src/crypto/keys.rs` (verify OsRng import still works)
- `termchat/src/tasks/merge.rs` (add header comment re: crdts evaluation)
- `.github/workflows/ci.yml` (add cargo-deny step)

### Commit Strategy

Single commit after all tasks complete, with a message covering:
- deny.toml creation and license policy
- rand 0.8 -> 0.9 upgrade (or documentation of why not)
- CRDT evaluation decision
- CI cargo-deny integration
- Duplicate dependency audit results
