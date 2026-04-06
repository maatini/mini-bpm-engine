#!/usr/bin/env bash
# Detailed Coverage Oracle mit HTML-Report

set -e

echo "=== BPMNinja Coverage Oracle ==="

cargo llvm-cov --workspace --html --output-dir target/coverage -- --skip stress_
cargo llvm-cov --workspace --codecov --output-path target/coverage/coverage.json -- --skip stress_

COVERAGE=$(jq -r '.total' target/coverage/coverage.json)
echo "Total coverage: ${COVERAGE}%"
echo "HTML report generated → target/coverage/index.html"
