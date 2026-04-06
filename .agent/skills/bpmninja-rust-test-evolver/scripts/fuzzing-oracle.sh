#!/usr/bin/env bash
# Fuzzing Oracle für engine-core und parser (cargo-fuzz)

set -e

echo "=== BPMNinja Fuzzing Oracle ==="

cargo install cargo-fuzz --quiet 2>/dev/null || true

# Fuzz Token-Engine und BPMN-Parser (max 30 Sekunden pro Run)
cargo fuzz run token_execution -- -max_total_time=30 || { echo "FAIL: Fuzzing crash detected"; exit 1; }
cargo fuzz run bpmn_parser -- -max_total_time=30 || { echo "FAIL: Parser fuzzing crash"; exit 1; }

echo "PASS: No crashes in fuzzing runs"
