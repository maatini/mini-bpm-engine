---
trigger: file_match
file_patterns: ["**/*.rs", "**/Cargo.toml"]
---

# RUST_AGENT_RULES.md - Rust Skills & Best Practices for AI Agents

**Core Rules (MUST follow always):**
- Think step-by-step before writing code.
- Always prioritize compile-time safety and idiomatic Rust.
- Use proper error handling with thiserror + anyhow.
- Run `cargo clippy --workspace --all-targets -- -D warnings` after every major change.
- Write comprehensive unit + integration tests.
- Keep the architecture clean and modular.

## Must-Follow Rust Rules
- Use Rust 2024 edition.
- Prefer `Result<T, E>` + `?` operator.
- Custom errors with `thiserror`. Use `EngineError` enum (defined in `engine-core/src/error.rs`).
- Async: Use `tokio` with `#[tokio::main]`.
- For timers: `tokio::time::sleep` or `tokio::time::interval`.
- State machines: Use enums for task types (`BpmnElement`) and process states (`InstanceState`).
- Concurrency: Use `Arc<dyn WorkflowPersistence>` for shared persistence. Use `Arc<ProcessDefinition>` for shared definitions.
- Logging: Use the `tracing` crate (`tracing::info!`, `tracing::error!`, `tracing::debug!`, `tracing::warn!`). Initialize with `tracing-subscriber`.
- Testing: Always write `#[test]` and `#[tokio::test]`. Aim for >85% coverage on core logic.
- Clippy: Fix all warnings + pedantic where reasonable.
- Naming: snake_case, descriptive. Modules in separate files.

## Anti-Patterns (NEVER do)
- No `.unwrap()` in production paths (only in tests with clear context).
- No giant main.rs files — use proper module structure.
- No shared mutable state without synchronization.
- No ignoring compiler warnings.
- No blocking the Tokio runtime (always `.await`).

Follow these for maximum agent performance.
