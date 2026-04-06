#!/usr/bin/env bash
# Polling & Heartbeat Test Oracle

set -e

echo "=== Agent Polling Oracle ==="

cargo test -p agent-orchestrator --test polling_integration -- --quiet || { echo "FAIL: Polling tests failed"; exit 1; }

echo "PASS: Polling and heartbeat tests successful"
