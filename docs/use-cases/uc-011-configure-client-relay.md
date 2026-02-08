# Use Case: UC-011 Configure Client and Relay via Config File and CLI

## Classification
- **Goal Level**: :fish: Subfunction
- **Scope**: System (black box)
- **Priority**: P1 High
- **Complexity**: :yellow_circle: Medium

## Actors
- **Primary Actor**: Terminal User (operator/developer)
- **Supporting Actors**: Filesystem (config file), Shell environment (env vars)
- **Stakeholders & Interests**:
  - User: customizable timeouts, relay URL, display settings without recompilation
  - Operator: deploy relay with different bind address and queue limits
  - Developer: sensible defaults that match existing behavior (zero-churn migration)

## Conditions
- **Preconditions**:
  1. TermChat binary is installed and runnable
  2. Config directory is writable (or missing -- not an error)
- **Success Postconditions**:
  1. CLI args override env vars override config file override compiled defaults
  2. Missing config file uses compiled defaults (not an error)
  3. Explicit --config pointing to nonexistent file produces a clear error
  4. All existing hardcoded constants are replaceable via config
  5. Existing tests pass unchanged (Default impls preserve current values)
  6. Relay server accepts --bind, --max-payload-size, --max-queue-size
  7. Client accepts --relay-url, --peer-id, --remote-peer, --config, --timestamp-format
- **Failure Postconditions**:
  1. Malformed TOML shows parse error with file path
  2. I/O error reading config shows error with path and OS error
  3. App falls back to defaults on any config load failure (unless --config was explicit)
- **Invariants**:
  1. No behavioral change when no config file exists and no CLI args passed
  2. Backward compatibility: RELAY_URL, PEER_ID, REMOTE_PEER, RELAY_ADDR env vars still work

## Main Success Scenario
1. User creates `~/.config/termchat/config.toml` with custom settings
2. User runs `termchat` (no CLI args)
3. System reads config file, merges with compiled defaults
4. System launches with custom settings applied
5. User can override any config file value via CLI args
6. User can override via env vars (for backward compat)

## Extensions
- **1a. Config file does not exist**:
  1. System uses compiled defaults for all values
  2. No error is shown
- **1b. Config file has partial settings**:
  1. System merges provided values with defaults for missing fields
- **1c. Config file has invalid TOML syntax**:
  1. System shows parse error with file path and exits
- **2a. User passes --config /explicit/path.toml that doesn't exist**:
  1. System shows "failed to read config file: /explicit/path.toml: No such file"
  2. System exits with error
- **5a. CLI arg conflicts with config file**:
  1. CLI arg wins (highest priority)
- **6a. Env var conflicts with config file**:
  1. Env var wins (clap `env` attribute handles this)

## Agent Execution Notes
- **Verification Command**: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
- **Test File**: Unit tests inline in `termchat/src/config/mod.rs` and `termchat-relay/src/config.rs`
- **Depends On**: None (cross-cutting infrastructure)
- **Blocks**: Future UCs needing persistent settings
- **Estimated Complexity**: Medium
- **Agent Assignment**: Single-agent (established patterns, medium complexity)

## Acceptance Criteria
- [ ] `cargo test` passes (all existing + new config unit tests)
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -- -D warnings` passes
- [ ] No `unwrap()` or `expect()` in production code
- [ ] All public functions have doc comments
- [ ] Default config values match all current hardcoded constants exactly
- [ ] Config file parsing handles full, partial, and empty TOML
- [ ] CLI args override config file values
- [ ] Env vars work for backward compatibility (RELAY_URL, PEER_ID, etc.)
- [ ] Missing config file is silent; explicit --config to nonexistent file errors
