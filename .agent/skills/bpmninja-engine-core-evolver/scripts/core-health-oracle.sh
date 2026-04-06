#!/usr/bin/env bash
# External Oracle für engine-core – EvoSkills Validierung

set -e

echo "=== BPMNinja Engine-Core Health Oracle ==="

echo "1. Running engine-core tests..."
cargo test -p engine-core -- --quiet || { echo "FAIL: Tests failed"; exit 1; }

echo "2. Clippy..."
cargo clippy -p engine-core --all-targets -- -D warnings || { echo "FAIL: Clippy errors"; exit 1; }

echo "3. Building..."
cargo build -p engine-core --release

echo "4. Coverage..."
cargo install cargo-llvm-cov --quiet 2>/dev/null || true
cargo llvm-cov -p engine-core --codecov --output-path coverage.json

COVERAGE=$(jq -r '.total' coverage.json)
echo "Coverage: ${COVERAGE}%"

if (( $(echo "$COVERAGE > 90" | bc -l) )); then
  echo "PASS"
  echo "METRICS: coverage=${COVERAGE} tests=OK"
else
  echo "FAIL (Coverage too low)"
  exit 1
fi
