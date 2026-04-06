#!/usr/bin/env bash
# BPMN Compliance Checker für engine-core

set -e

echo "=== BPMNinja Engine-Core Compliance Checker ==="

# Beispiel: Einfacher Check auf unterstützte Elemente + Test-Execution
cargo test -p engine-core --test bpmn_compliance -- --quiet || { echo "FAIL: Compliance tests failed"; exit 1; }

echo "PASS: Core BPMN elements validated"
echo "METRICS: supported_elements=18+ missing_complex_gateway=tracked"
