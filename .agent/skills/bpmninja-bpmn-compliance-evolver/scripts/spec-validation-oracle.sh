#!/usr/bin/env bash
# Spec Validation Oracle – prüft gegen BPMN 2.0 Spec

set -e

echo "=== BPMN Spec Validation Oracle ==="

# Führt alle spec-konformen Tests aus
cargo test --test bpmn_spec_validation -- --quiet || { echo "FAIL: Spec validation failed"; exit 1; }

echo "PASS: All tests conform to BPMN 2.0 Specification"
