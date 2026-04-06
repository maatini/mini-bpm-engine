#!/usr/bin/env bash
# UI + bpmn-js Integration Test Oracle

set -e

echo "=== Tauri UI Test Oracle ==="

cargo test -p desktop-tauri --test ui_integration -- --quiet || { echo "FAIL: UI tests failed"; exit 1; }

echo "PASS: All UI and bpmn-js integration tests successful"
