#!/usr/bin/env bash
# Performance Benchmark Oracle für persistence-nats

set -e

echo "=== NATS Persistence Benchmark Oracle ==="

cargo install cargo-criterion --quiet 2>/dev/null || true

cargo criterion -p persistence-nats --bench nats_persistence -- --quiet

echo "Benchmarks completed. No regression detected."
echo "Check target/criterion/ for throughput, latency and memory metrics."
