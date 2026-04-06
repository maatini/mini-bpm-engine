#!/usr/bin/env bash
# External Oracle für Documentation – EvoSkills Validierung

set -e

echo "=== Documentation Health Oracle ==="

echo "1. Checking Markdown files..."
find . -name "*.md" -not -path "./target/*" | xargs -I {} markdownlint {} || { echo "FAIL: Markdown issues"; exit 1; }

echo "2. Checking rustdoc..."
cargo doc --workspace --no-deps --quiet || { echo "FAIL: rustdoc failed"; exit 1; }

echo "PASS: Documentation health check successful"
