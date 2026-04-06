#!/usr/bin/env bash
# Fuzzing Oracle für bpmn-parser (cargo-fuzz)

set -e

echo "=== BPMN Parser Fuzzing Oracle ==="

cargo install cargo-fuzz --quiet 2>/dev/null || true

# Fuzz den Parser (max 30 Sekunden pro Run)
cargo fuzz run bpmn_parser -- -max_total_time=30 || { echo "FAIL: Fuzzing crash detected"; exit 1; }

echo "PASS: No crashes in fuzzing runs"
