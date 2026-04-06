---
description: Verify the project
---
// turbo-all
1. cargo build --workspace
2. cargo clippy --workspace --all-targets --all-features -- -D warnings
3. cargo test --workspace

If any step fails:
- Read the full error output carefully
- Fix the issue in the relevant crate
- Re-run the failing step before continuing to the next
