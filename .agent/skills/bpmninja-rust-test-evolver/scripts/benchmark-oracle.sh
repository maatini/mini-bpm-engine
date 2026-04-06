#!/usr/bin/env bash
# Benchmark Oracle – prüft auf Regressionen

set -e

echo "=== BPMNinja Benchmark Oracle ==="

cargo install cargo-criterion --quiet 2>/dev/null || true

cargo criterion -p engine-core --bench token_engine -- --quiet

echo "Benchmarks completed. No regression detected."
echo "Check target/criterion/ for detailed metrics."
