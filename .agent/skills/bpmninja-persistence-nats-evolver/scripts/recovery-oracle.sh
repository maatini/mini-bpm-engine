#!/usr/bin/env bash
# Recovery Oracle – simuliert Crash & Recovery mit NATS

set -e

echo "=== NATS Recovery Oracle ==="

# Starte lokalen NATS (falls nicht läuft)
nats-server -js -m 8222 -l /tmp/nats.log &> /dev/null &
NATS_PID=$!
sleep 2

cargo test -p persistence-nats --test recovery -- --quiet || { echo "FAIL: Recovery test failed"; kill $NATS_PID 2>/dev/null || true; exit 1; }

kill $NATS_PID 2>/dev/null || true
echo "PASS: Recovery tests successful"
