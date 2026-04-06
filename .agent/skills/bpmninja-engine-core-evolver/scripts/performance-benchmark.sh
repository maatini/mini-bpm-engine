#!/usr/bin/env bash
# Performance Benchmark Script für engine-core (Criterion)

set -e

echo "=== BPMNinja Engine-Core Performance Benchmark ==="

cargo install cargo-criterion --quiet 2>/dev/null || true

# Führe Benchmarks aus (benötigt benches/ Ordner im engine-core)
cargo criterion -p engine-core --bench token_engine -- --quiet

echo "Benchmarks completed. Results in target/criterion/"
echo "Check tokens-per-second, memory usage and latency metrics."
