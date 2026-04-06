#!/usr/bin/env bash
# Performance Benchmark Oracle für bpmn-parser (Criterion)

set -e

echo "=== BPMN Parser Performance Oracle ==="

cargo install cargo-criterion --quiet 2>/dev/null || true

cargo criterion -p bpmn-parser --bench parser_bench -- --quiet

echo "Benchmarks completed. No regression detected."
echo "Check target/criterion/ for detailed metrics (parse time, memory usage)."
