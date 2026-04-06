#!/usr/bin/env bash
# External Oracle für BPMN Compliance – EvoSkills Validierung

set -e

echo "=== BPMN Compliance Oracle ==="

echo "1. Running compliance test suite..."
cargo test --test bpmn_compliance || { echo "FAIL: Compliance tests failed"; exit 1; }

echo "2. Clippy on compliance tests..."
cargo clippy --tests -- -D warnings || { echo "FAIL: Clippy errors"; exit 1; }

echo "PASS: BPMN Compliance tests successful"
