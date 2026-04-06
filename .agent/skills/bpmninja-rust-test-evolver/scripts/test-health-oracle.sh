#!/usr/bin/env bash
# External Oracle für den gesamten Rust-Testprozess – EvoSkills Validierung

set -e

echo "=== BPMNinja Rust Test Health Oracle ==="

echo "1. Running full workspace tests..."
devbox run test --workspace || { echo "FAIL: Tests failed"; exit 1; }

echo "2. Clippy on all tests..."
cargo clippy --all-targets --tests -- -D warnings || { echo "FAIL: Clippy errors"; exit 1; }

echo "3. Coverage check..."
cargo install cargo-llvm-cov --quiet 2>/dev/null || true
cargo llvm-cov --workspace --codecov --output-path coverage.json

COVERAGE=$(jq -r '.total' coverage.json)
echo "Coverage: ${COVERAGE}%"

if (( $(echo "$COVERAGE > 92" | bc -l) )); then
  echo "PASS"
  echo "METRICS: coverage=${COVERAGE} tests=OK"
else
  echo "FAIL (Coverage too low)"
  exit 1
fi
