#!/usr/bin/env bash
# Coverage-Report Script für EvoSkills – generiert detaillierten Coverage-Report

set -e

echo "=== BPMNinja Coverage Report ==="

cargo install cargo-llvm-cov --quiet 2>/dev/null || true
cargo llvm-cov --workspace --html --output-dir target/coverage

echo "Coverage HTML Report generated in target/coverage/index.html"
echo "Total coverage: $(jq -r '.total' <(cargo llvm-cov --workspace --codecov --output-path /tmp/cov.json 2>/dev/null || echo '{"total":0}'))%"
