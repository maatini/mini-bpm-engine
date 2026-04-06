#!/usr/bin/env bash
# Coverage + Report Oracle

set -e

echo "=== CI/CD Coverage Oracle ==="

cargo llvm-cov --workspace --html --output-dir target/coverage
cargo llvm-cov --workspace --codecov --output-path target/coverage/coverage.json

COVERAGE=$(jq -r '.total' target/coverage/coverage.json)
echo "Total coverage: ${COVERAGE}%"
echo "HTML report generated → target/coverage/index.html"
echo "PASS: Coverage report ready for Codecov"
