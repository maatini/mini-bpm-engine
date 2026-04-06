#!/usr/bin/env bash
# Retry-Logik Test Oracle

set -e

echo "=== Agent Retry Oracle ==="

cargo test -p agent-orchestrator --test retry_logic -- --quiet || { echo "FAIL: Retry tests failed"; exit 1; }

echo "PASS: Retry and backoff logic validated"
