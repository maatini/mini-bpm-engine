#!/usr/bin/env bash
# API Integration Test Oracle für engine-server

set -e

echo "=== Engine Server API Test Oracle ==="

cargo test -p engine-server --test api_integration -- --quiet || { echo "FAIL: API tests failed"; exit 1; }

echo "PASS: All API integration tests successful"
