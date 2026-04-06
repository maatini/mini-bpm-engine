#!/usr/bin/env bash
# Benchmark Runner Script für EvoSkills – führt Performance-Benchmarks aus

set -e

echo "=== BPMNinja Benchmark Runner ==="

# Beispiel: Stress-Tests und Engine-Benchmarks (erweiterbar)
cargo test --release --test stress_tests -- --nocapture

echo "Benchmarks completed. Check target/criterion/ for detailed results."
