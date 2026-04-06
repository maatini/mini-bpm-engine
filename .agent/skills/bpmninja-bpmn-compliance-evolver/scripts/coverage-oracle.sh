#!/usr/bin/env bash
# Coverage Oracle speziell für Compliance-Tests

set -e

echo "=== BPMN Compliance Coverage Oracle ==="

cargo install cargo-llvm-cov --quiet 2>/dev/null || true
cargo llvm-cov --test bpmn_compliance --codecov --output-path coverage.json

COVERAGE=$(jq -r '.total' coverage.json)
echo "Compliance Coverage: ${COVERAGE}%"

if (( $(echo "$COVERAGE > 95" | bc -l) )); then
  echo "PASS"
else
  echo "FAIL (Coverage too low)"
  exit 1
fi
