---
name: bpmninja-engine
description: Skill for the engine-core crate covering hexagonal architecture, token-based BPMN execution, gateway routing, scripting, and BPMN 2.0 compliance. Implements EvoSkills co-evolutionary verification (arXiv 2604.01687).
version: 2.0.0
tags: [rust, engine-core, bpmn, token-engine, concurrency, hexagonal, evoskills]
requires: [cargo]
---

# BPMNinja Engine Skill

## When to Activate
Activate whenever you work on the `engine-core` crate:
- Adding or modifying BPMN elements (gateways, events, tasks, sub-processes)
- Token execution, state machine, or concurrency changes
- Rhai scripting or condition evaluator improvements
- History tracking or persistence trait modifications
- Performance benchmarks or test coverage improvements

## Scope & File Map

```
engine-core/src/
├── lib.rs                    # Public API, re-exports, backward-compat aliases
├── condition.rs              # Condition evaluator (expressions for gateways)
├── domain/                   # Pure domain models (no I/O)
│   ├── mod.rs
│   ├── definition.rs         # ProcessDefinition, BpmnElement
│   ├── element.rs            # Element types enum
│   ├── error.rs              # WorkflowError types
│   ├── file_ref.rs           # File reference model
│   ├── flow.rs               # SequenceFlow
│   ├── listener.rs           # Execution listeners
│   ├── multi_instance.rs     # Multi-instance config
│   ├── timer.rs              # Timer definitions (ISO 8601, cron)
│   ├── token.rs              # Token model
│   └── tests.rs              # Domain model tests
├── port/                     # Trait boundaries (hexagonal ports)
│   ├── mod.rs
│   └── persistence.rs        # WorkflowPersistence trait
├── adapter/                  # Trait implementations
│   ├── mod.rs
│   └── in_memory.rs          # InMemoryPersistence (for tests)
├── engine/                   # Application services
│   ├── mod.rs                # WorkflowEngine main struct
│   ├── boundary.rs           # Boundary event handling
│   ├── definition_ops.rs     # Definition management operations
│   ├── gateway.rs            # Gateway routing logic
│   ├── instance_ops.rs       # Instance lifecycle operations
│   ├── instance_store.rs     # DashMap-based concurrent store
│   ├── message_processor.rs  # Message event processing
│   ├── persistence_ops.rs    # Persistence integration
│   ├── process_start.rs      # Process instantiation logic
│   ├── registry.rs           # Definition registry
│   ├── retry_queue.rs        # Retry queue for failed operations
│   ├── service_task.rs       # Service task execution
│   ├── timer_processor.rs    # Timer event processing
│   ├── user_task.rs          # User task handling
│   ├── executor/             # Token execution engine
│   ├── handlers/             # Element-specific handlers
│   │   ├── events.rs         # Event handlers
│   │   ├── gateways.rs       # Gateway handlers
│   │   ├── sub_processes.rs  # Sub-process handlers
│   │   └── tasks.rs          # Task handlers
│   └── tests/                # Engine integration tests
├── runtime/                  # Runtime infrastructure
│   ├── mod.rs
│   ├── constants.rs          # Runtime constants
│   ├── instance.rs           # RuntimeInstance (DashMap-based)
│   ├── pending.rs            # Pending token management
│   └── stats.rs              # Runtime statistics
├── history/                  # Audit trail
│   └── (HistoryEntry, HistoryDiff, VariableDiff)
└── scripting/                # Rhai script engine
    └── mod.rs                # ScriptEngine, evaluate_script
```

## Domain Rules & Patterns

1. **Hexagonal Architecture**: Domain models in `domain/` MUST NOT depend on adapters or engine. Communication across boundaries only via traits in `port/`.
2. **Backward Compatibility**: `lib.rs` re-exports all public types. Legacy aliases (`model`, `persistence`, `timer_definition`) must be preserved until downstream crates migrate.
3. **Concurrency**: `instance_store.rs` uses `DashMap` for lock-free concurrent access. Never introduce global `Mutex`/`RwLock` on the instance store.
4. **Token Semantics**: Every BPMN element consumes input tokens and produces output tokens. Gateway routing is deterministic for the same input state.
5. **Error Handling**: Use `WorkflowError` from `domain/error.rs`. Never unwrap in production code – use `?` or explicit error mapping.
6. **Cross-Crate**: Follow `CROSS_CRATE_WORKFLOW.md` – engine-core changes come FIRST, then parser, persistence, server, desktop.

## Co-Evolutionary Verification (EvoSkills, arXiv 2604.01687)

Every change MUST go through this loop before commit:

### Step 1 – Generate
Use the Graphify MCP Tools first to analyze the relevant Graph Communities (e.g., 0, 1, 4, 11, 17, 18, 19, 25). Only after understanding the graph boundaries, read the specific source files. Produce concrete, diff-ready Rust code changes. Maintain persistent context from previous iterations (accumulated feedback from surrogate and oracle).

### Step 2 – Surrogate Verification (Self-Critique)
Before running the oracle, evaluate your changes against these criteria (score 0–10 each, **all must be ≥ 7**):

| # | Criterion | Question |
|---|---|---|
| 1 | Lock-Free Safety | Does the change avoid introducing global locks? Is DashMap usage correct (no nested locks, no iterator + mutation)? |
| 2 | BPMN 2.0 Compliance | Does the implementation match the BPMN 2.0 spec semantics (token flow, gateway merge/split, event triggers)? |
| 3 | Backward Compatibility | Do all re-exports in `lib.rs` still work? Will downstream crates (engine-server, persistence-nats) compile? |
| 4 | Error Handling | Are all error paths covered? No `unwrap()` on fallible operations in non-test code? |
| 5 | Test Coverage | Are new code paths covered by unit tests? Do existing tests still pass conceptually? |
| 6 | Hexagonal Boundaries | Does domain/ stay pure (no I/O deps)? Are adapter/ and port/ interfaces respected? |

If ANY criterion scores < 7, provide **actionable failure diagnostic** (root cause + specific fix) and return to Step 1.

### Step 3 – External Oracle
Run `scripts/oracle.sh` from this skill directory. The oracle returns only **PASS** or **FAIL + exit code**. Do NOT read oracle internals for debugging – reason from your own outputs.

### Step 4 – Evolution Decision
- **Surrogate FAIL** → Return to Step 1 with diagnostic feedback
- **Surrogate PASS, Oracle FAIL** → Escalate surrogate criteria (add stricter checks or raise thresholds), then return to Step 1
- **Oracle PASS** → Commit. Update Evolution Log below.

Max rounds: 5 oracle invocations, 15 surrogate retries.

## Common Pitfalls
- Only theoretical improvements without running the oracle
- Ignoring multi-threading edge cases in DashMap (e.g., deadlocks with nested entries)
- Rhai scripts without error handling (script panics crash the engine)
- Breaking backward-compat re-exports when moving modules
- Performance regressions in token execution hot path

## Evolution Log
<!-- Track iterations across invocations for co-evolutionary learning -->
| Date | Change | Surrogate Rounds | Oracle Result | Notes |
|---|---|---|---|---|
