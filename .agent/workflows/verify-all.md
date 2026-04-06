---
description: Full project verification (Rust + UI)
---
// turbo-all
1. cargo build --workspace
2. cargo clippy --workspace --all-targets --all-features -- -D warnings
3. cargo test --workspace
4. cd desktop-tauri && npm run build
