---
name: bpmninja-quality
description: Cross-cutting skill for testing, CI/CD, documentation, and code quality across the entire BPMNinja workspace. Implements EvoSkills co-evolutionary verification (arXiv 2604.01687).
version: 2.0.0
tags: [testing, ci-cd, documentation, coverage, benchmarks, github-actions, evoskills]
requires: [cargo]
---

# BPMNinja Quality Skill

## When to Activate
Activate for cross-cutting quality concerns:
- Writing or improving tests (unit, integration, E2E, fuzzing, property-based)
- Modifying CI/CD workflows (`.github/workflows/`)
- Updating documentation (README, architecture docs, API docs)
- Coverage analysis, mutation testing, benchmarks
- Release automation and versioning

## Scope & File Map

### Testing Infrastructure
```
engine-core/src/engine/tests/     # Engine integration tests
engine-core/src/domain/tests.rs   # Domain model tests
bpmn-parser/src/tests.rs          # Parser tests
persistence-nats/src/tests.rs     # Persistence tests (conditional on NATS)
```

### CI/CD Workflows
```
.github/workflows/
├── build-linux.yml               # Linux build matrix
├── build-macos.yml               # macOS build matrix
├── build-windows.yml             # Windows build matrix
├── dependabot-auto-merge.yml     # Auto-merge dependency updates
├── mutation-tests.yml            # cargo-mutants mutation testing
├── pages.yml                     # GitHub Pages deployment
└── release.yml                   # Release build pipeline
```

### Documentation
```
README.md                         # Project overview, badges, metrics
docs/
├── architecture.md               # Architecture overview (Mermaid diagrams)
└── (additional docs)
```

### Configuration
```
Cargo.toml                        # Workspace definition
devbox.json                       # Development environment
docker-compose.yaml               # Local NATS setup
```

## Domain Rules & Patterns

### Testing
1. **Determinism**: All tests must be deterministic. No sleep-based timing, no random seeds without control.
2. **Test Duration**: Individual tests should complete in < 30 seconds (benchmarks excluded).
3. **BPMN Compliance Tests**: Test BPMN execution semantics, not just data structures. Verify token flow, gateway routing, event triggers.
4. **Mutation Testing**: Use `cargo-mutants` to validate test effectiveness. Track mutation score.
5. **Conditional Tests**: NATS-dependent tests must be skipped when NATS is unavailable (use `#[ignore]` or runtime checks).

### CI/CD
1. **Workflow Stability**: All CI jobs must be deterministic and cache-optimized.
2. **Build Matrix**: Test on Linux, macOS, and Windows.
3. **Semantic Versioning**: Follow semver. Breaking changes require major version bump.
4. **Caching**: Use `Swatinem/rust-cache` for Cargo build caches in GitHub Actions.

### Documentation
1. **Accuracy**: All metrics in README must match actual test output. Never fabricate numbers.
2. **Architecture Diagrams**: Use Mermaid syntax. Keep diagrams in sync with actual module structure.
3. **API Documentation**: Use `cargo doc` comments (`///`) for all public types and functions.

## Co-Evolutionary Verification (EvoSkills, arXiv 2604.01687)

Every change MUST go through this loop before commit:

### Step 1 – Generate
Use the Graphify MCP Tools first to analyze the relevant Graph Communities. Only after understanding the graph boundaries, read the specific test or workflow files. Produce concrete changes.

### Step 2 – Surrogate Verification (Self-Critique)
Evaluate changes (score 0–10 each, **all must be ≥ 7**):

| # | Criterion | Question |
|---|---|---|
| 1 | Test Determinism | Are all new/modified tests free from timing dependencies, race conditions, or flaky behavior? |
| 2 | CI Stability | Do workflow changes maintain cache compatibility? Are new jobs idempotent? |
| 3 | Documentation Accuracy | Do doc changes reflect the actual current state of the codebase? No stale metrics or paths? |
| 4 | Coverage Impact | Do new tests cover meaningful code paths (branch coverage, not just line coverage)? |
| 5 | Cross-Platform | Do CI changes work on all three target platforms (Linux, macOS, Windows)? |
| 6 | BPMN Semantics | Do compliance tests verify actual BPMN execution behavior, not just data shape? |

If ANY criterion scores < 7 → return to Step 1 with actionable diagnostic.

### Step 3 – External Oracle
Run `scripts/oracle.sh`. Returns only **PASS** or **FAIL + exit code**.

### Step 4 – Evolution Decision
- **Surrogate FAIL** → Fix and retry (max 15 retries)
- **Surrogate PASS, Oracle FAIL** → Escalate surrogate criteria, retry
- **Oracle PASS** → Commit. Update Evolution Log.

## Common Pitfalls
- Tests without meaningful assertions on BPMN semantics (just checking "no panic")
- Flaky tests caused by uncontrolled async timing
- Fabricated coverage or mutation scores in documentation
- CI workflows that work on Linux but fail on Windows (path separators, line endings)
- Adding test dependencies that bloat compile time

## Evolution Log
| Date | Change | Surrogate Rounds | Oracle Result | Notes |
|---|---|---|---|---|
