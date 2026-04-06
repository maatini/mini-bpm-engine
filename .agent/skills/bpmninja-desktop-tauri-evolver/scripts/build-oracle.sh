#!/usr/bin/env bash
# Tauri Build Oracle – prüft Debug- und Release-Build

set -e

echo "=== Tauri Build Oracle ==="

echo "Building debug version..."
cargo tauri build --debug || { echo "FAIL: Debug build failed"; exit 1; }

echo "Building release version (quick check)..."
cargo tauri build -- --features production || { echo "FAIL: Release build failed"; exit 1; }

echo "PASS: Tauri builds successful on all platforms"
