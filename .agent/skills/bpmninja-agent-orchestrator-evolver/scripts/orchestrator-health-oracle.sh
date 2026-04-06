#!/usr/bin/env bash
# External Oracle für agent-orchestrator – EvoSkills Validierung

set -e

echo "=== Agent Orchestrator Health Oracle ==="

echo "1. Running tests..."
cargo test -p agent-orchestrator || { echo "FAIL: Tests failed"; exit 1; }

echo "2. Clippy..."
cargo clippy -p agent-orchestrator --all-targets -- -D warnings || { echo "FAIL: Clippy errors"; exit 1; }

echo "3. Coverage..."
cargo install cargo-llvm-cov --quiet 2>/dev/null || true
cargo llvm-cov -p agent-orchestrator --codecov --output-path coverage.json

COVERAGE=$(jq -r '.total' coverage.json)
echo "Coverage: ${COVERAGE}%"

if (( $(echo "$COVERAGE > 90" | bc -l) )); then
  echo "PASS"
  echo "METRICS: coverage=${COVERAGE} tests=OK"
else
  echo "FAIL (Coverage too low)"
  exit 1
fi
