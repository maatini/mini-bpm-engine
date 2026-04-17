---
trigger: file_match
file_patterns: ["**/*.rs", "**/Cargo.toml"]
---

> **Hinweis:** Diese Datei enthält workspace-weite Rust-Regeln. Für engine-core-spezifische Regeln (Hexagonal Architecture, EvoSkills-Loop, DashMap-Concurrency) siehe `RUST_ENGINE_AGENT.md`. Bei Konflikten hat `RUST_ENGINE_AGENT.md` Vorrang für `engine-core/**`.

# Rust Best Practices for BPMNinja

**Core Rules:**
- Think step-by-step before writing code.
- Always prioritize compile-time safety and idiomatic Rust.
- Use proper error handling with `thiserror` + `anyhow`.
- Run `/lint` workflow after every major change.
- Write comprehensive unit + integration tests.
- Keep the architecture clean and modular.

## Must-Follow Rust Rules
- Use Rust 2024 edition for all workspace crates (exception: `desktop-tauri/src-tauri` uses 2021 for Tauri v1 compatibility).
- Prefer `Result<T, E>` + `?` operator.
- Custom errors with `thiserror`. Use `EngineError` enum (defined in `engine-core/src/error.rs`).
- Async: Use `tokio` with `#[tokio::main]`.
- For timers: `tokio::time::sleep` or `tokio::time::interval`.
- State machines: Use enums for task types (`BpmnElement`) and process states (`InstanceState`).
- Concurrency: Use `Arc<dyn WorkflowPersistence>` for shared persistence. Use `Arc<ProcessDefinition>` for shared definitions.
- Logging: Use the `tracing` crate (`tracing::info!`, `tracing::error!`, `tracing::debug!`, `tracing::warn!`). Initialize with `tracing-subscriber`.
- Testing: Always write `#[test]` and `#[tokio::test]`. Aim for >85% coverage on core logic.
- Clippy: Fix all warnings. Run via `/lint` workflow.
- Naming: `snake_case`, descriptive. Modules in separate files.

## Anti-Patterns (NEVER do)
- No `.unwrap()` in production paths. Use `.expect("reason")` only when the invariant is guaranteed and documented. `.unwrap()` is acceptable in tests.
- No giant `main.rs` files — use proper module structure.
- No shared mutable state without synchronization.
- No ignoring compiler warnings.
- No blocking the Tokio runtime (always `.await`, never `block_on` inside async).
- No adding dependencies directly in crate `Cargo.toml` — use `[workspace.dependencies]` (see `DEPENDENCY_MANAGEMENT.md`).
