#!/usr/bin/env bash
# External Oracle für EvoSkills – liefert binäre + metrische Validierung

set -e

echo "=== BPMNinja Dev Health Oracle ==="

echo "1. Running tests..."
devbox run test || { echo "FAIL: Tests failed"; exit 1; }

echo "2. Clippy..."
cargo clippy --all-targets -- -D warnings || { echo "FAIL: Clippy errors"; exit 1; }

echo "3. Building release..."
cargo build --release --workspace

echo "4. Coverage (cargo-llvm-cov)..."
cargo install cargo-llvm-cov --quiet
cargo llvm-cov --workspace --codecov --output-path coverage.json

COVERAGE=$(jq -r '.total' coverage.json)
echo "Coverage: ${COVERAGE}%"

if (( $(echo "$COVERAGE > 85" | bc -l) )); then
  echo "PASS"
  echo "METRICS: coverage=${COVERAGE} build_time=OK"
else
  echo "FAIL (Coverage too low)"
  exit 1
fi
