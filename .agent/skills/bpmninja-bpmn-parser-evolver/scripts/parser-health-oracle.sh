#!/usr/bin/env bash
# External Oracle für bpmn-parser – EvoSkills Validierung

set -e

echo "=== BPMN Parser Health Oracle ==="

echo "1. Running tests..."
cargo test -p bpmn-parser || { echo "FAIL: Tests failed"; exit 1; }

echo "2. Clippy..."
cargo clippy -p bpmn-parser --all-targets -- -D warnings || { echo "FAIL: Clippy errors"; exit 1; }

echo "3. Coverage..."
cargo install cargo-llvm-cov --quiet 2>/dev/null || true
cargo llvm-cov -p bpmn-parser --codecov --output-path coverage.json

COVERAGE=$(jq -r '.total' coverage.json)
echo "Coverage: ${COVERAGE}%"

if (( $(echo "$COVERAGE > 95" | bc -l) )); then
  echo "PASS"
  echo "METRICS: coverage=${COVERAGE} tests=OK"
else
  echo "FAIL (Coverage too low)"
  exit 1
fi
