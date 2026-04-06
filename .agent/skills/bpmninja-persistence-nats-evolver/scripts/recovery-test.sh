#!/usr/bin/env bash
set -e
echo "=== Recovery Test Oracle ==="
cargo test -p persistence-nats --test recovery || exit 1
echo "PASS"
