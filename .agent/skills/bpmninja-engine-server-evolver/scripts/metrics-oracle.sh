#!/usr/bin/env bash
# Metrics Endpoint Oracle – prüft Prometheus-Kompatibilität

set -e

echo "=== Engine Server Metrics Oracle ==="

# Starte Server kurz im Background (falls möglich) und prüfe /metrics
cargo test -p engine-server --test metrics_endpoint -- --quiet || { echo "FAIL: Metrics test failed"; exit 1; }

echo "PASS: Metrics endpoint is Prometheus-compatible"
