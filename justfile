# TermChat quality pipeline
# Run `just` to see all available recipes
# Run `just quality` for the full quality gate

# Default: list available recipes
default:
    @just --list

# Full quality gate (run before committing)
quality: fmt lint deny test unwrap-check
    @echo "All quality checks passed."

# Check formatting
fmt:
    cargo fmt --check

# Clippy with deny warnings (pedantic + nursery enabled via workspace config)
lint:
    cargo clippy --workspace -- -D warnings

# Dependency audit: advisories, licenses, bans, sources
deny:
    cd termchat && cargo deny check

# Run all tests
test:
    cargo test

# Check for unwrap()/expect() in production code (not test modules)
unwrap-check:
    #!/usr/bin/env bash
    set -euo pipefail
    hits=$(grep -rn '\.unwrap()\|\.expect(' termchat/src termchat-relay/src termchat-proto/src \
        --include='*.rs' \
        | grep -v '#\[cfg(test)\]' \
        | grep -v '// test' \
        | grep -v 'unwrap_or' \
        | grep -v 'unwrap_or_default' \
        | grep -v 'unwrap_or_else' \
        | grep -v '^ *///' \
        | grep -v ':.*///' || true)
    if [ -n "$hits" ]; then
        # Filter: only flag lines that are NOT inside a #[cfg(test)] mod
        found=0
        while IFS= read -r line; do
            file=$(echo "$line" | cut -d: -f1)
            lineno=$(echo "$line" | cut -d: -f2)
            # Check if this line is inside a test module
            test_start=$(grep -n '#\[cfg(test)\]' "$file" | head -1 | cut -d: -f1)
            if [ -z "$test_start" ] || [ "$lineno" -lt "$test_start" ]; then
                echo "PRODUCTION: $line"
                found=1
            fi
        done <<< "$hits"
        if [ "$found" -eq 1 ]; then
            echo "ERROR: Found unwrap()/expect() in production code"
            exit 1
        fi
    fi
    echo "No production unwrap/expect found."

# Code complexity metrics (rust-code-analysis)
complexity:
    #!/usr/bin/env bash
    set -euo pipefail
    rust-code-analysis-cli -m -p ./termchat/src -O json 2>/dev/null \
      | python3 -c "import json,sys;lines=[json.loads(l) for l in sys.stdin if l.strip()];print(f'Files analyzed: {len(lines)}');[print(f'  LOW MI ({s[\"metrics\"][\"mi\"][\"mi_original\"]:.1f}): {s[\"name\"]}') for e in lines for s in e.get('spaces',[]) if s.get('metrics',{}).get('mi',{}).get('mi_original',100)<50]" \
      || echo "rust-code-analysis-cli not available or no issues found"

# Lines of code stats
stats:
    tokei termchat/src termchat-relay/src termchat-proto/src

# Unsafe code detection
geiger:
    cd termchat && cargo geiger 2>/dev/null || echo "cargo-geiger encountered an error"

# Extended quality: everything including slow checks
quality-full: quality complexity stats geiger
    @echo "Full quality report complete."
