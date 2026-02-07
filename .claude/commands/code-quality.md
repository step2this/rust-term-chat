# Grade Work

Run the full quality pipeline against the current codebase and produce 
a graded report.

## Steps

1. Run `cargo clippy --message-format=json 2>&1` and capture output
2. Run `rust-code-analysis-cli -m -p ./src -O json` and capture output  
3. Run `cargo coupling analyze --format markdown` and capture output
4. Run `cargo modules dependencies --lib --acyclic --no-fns --no-types 2>&1` 
   to check for circular dependencies
5. Run `cargo deny check` and capture output
6. Run `tokei ./src --output json` for LOC stats

## Analysis

Using the outputs above, grade the codebase against these criteria:

### Architectural Quality (A-F)
- Does the module structure follow hexagonal architecture? 
  (domain has zero deps on infra, ports are traits, adapters are concrete)
- Are there circular dependencies? (cargo-modules --acyclic output)
- What's the coupling grade from cargo-coupling?
- Are module boundaries clean? (pub(crate) vs pub leaking)

### Code Quality (A-F)  
- Any clippy warnings at pedantic level?
- Functions with cyclomatic complexity > 10? (rust-code-analysis)
- Functions with cognitive complexity > 15?
- Maintainability index below 50 for any module?
- Any unwrap/expect usage?

### Wheel Reinvention Check
- For each module with complexity > 10, check: could this be replaced 
  by a well-maintained crate on crates.io?
- Flag any hand-rolled implementations of: serialization, error handling, 
  async patterns, crypto primitives, protocol parsing
- Suggest specific crate alternatives with rationale

### Security & Dependencies (A-F)
- cargo-deny advisory check results
- cargo-geiger unsafe code count
- Any duplicate dependency versions?

## Output Format
Produce a markdown report with:
- Overall grade (weighted: Architecture 30%, Code 30%, Dependencies 20%, 
  Wheel Reinvention 20%)
- Per-module breakdown table
- Top 5 action items ranked by impact
- Comparison to last grading if previous report exists in ./reports/
