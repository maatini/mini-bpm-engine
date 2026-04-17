---
name: bpmninja-parser
description: Skill for the bpmn-parser crate covering BPMN 2.0 XML parsing, model generation, validation, and error recovery. Implements EvoSkills co-evolutionary verification (arXiv 2604.01687).
version: 2.0.0
tags: [rust, bpmn-parser, xml, quick-xml, parsing, validation, evoskills]
requires: [cargo]
---

# BPMNinja Parser Skill

## When to Activate
Activate whenever you work on the `bpmn-parser` crate:
- Adding new BPMN element parsing support
- Improving XML parsing performance or error recovery
- Extending the parser model (new fields, attributes)
- Writing fuzzing or property-based tests for the parser
- Fixing spec-compliance issues in parsed output

## Scope & File Map

```
bpmn-parser/src/
├── lib.rs        # Public API re-exports
├── models.rs     # BPMN model structs (ProcessDefinition, elements, flows)
├── parser.rs     # XML → Model parsing logic (quick-xml + serde)
└── tests.rs      # Parser unit & integration tests
```

### Key Types
- `ProcessDefinition` – Top-level parsed BPMN process
- `BpmnElement` – Enum of all supported BPMN elements
- `SequenceFlow` – Flow connections between elements
- `BpmnParser::parse(xml: &str) -> Result<ProcessDefinition>` – Main entry point

### Relationship to engine-core
The parser produces `ProcessDefinition` which engine-core's `domain/definition.rs` consumes. The parser's model types MUST be compatible with engine-core's domain types. When adding new elements to the parser, engine-core's domain model usually needs a corresponding update first (see CROSS_CRATE_WORKFLOW.md).

## Domain Rules & Patterns

1. **quick-xml + serde**: All XML parsing uses `quick-xml`. Do not introduce alternative XML parsers.
2. **Graceful Error Recovery**: Unknown XML elements should be skipped with a warning, not cause parse failures. This enables forward-compatibility with newer BPMN versions.
3. **Validation**: After parsing, validate structural integrity (e.g., all sequence flow targets exist, start events present).
4. **No External I/O**: The parser is a pure function `&str → Result<ProcessDefinition>`. No file system or network access.
5. **Performance**: Parser must handle BPMN files up to 10MB without excessive memory allocation. Prefer streaming/SAX-style parsing over DOM.

## Co-Evolutionary Verification (EvoSkills, arXiv 2604.01687)

Every change MUST go through this loop before commit:

### Step 1 – Generate
Use the Graphify MCP Tools first to analyze the relevant Graph Communities (e.g. 8, 10). Only after understanding the graph boundaries, read specific parser files like `parser.rs`. Produce concrete, diff-ready Rust code changes.

### Step 2 – Surrogate Verification (Self-Critique)
Evaluate changes against these criteria (score 0–10 each, **all must be ≥ 7**):

| # | Criterion | Question |
|---|---|---|
| 1 | Parsing Safety | Does the change handle malformed XML gracefully? No panics on invalid input? |
| 2 | BPMN 2.0 Spec Compliance | Do parsed elements match the official BPMN 2.0 XML schema semantics? |
| 3 | Model Compatibility | Are parser model types still compatible with engine-core's domain types? |
| 4 | Error Recovery | Are unknown/unsupported elements skipped gracefully instead of causing failures? |
| 5 | Test Coverage | Are new parsing paths covered by tests with representative XML snippets? |
| 6 | Performance | Does the change avoid unnecessary allocations or O(n²) patterns on large inputs? |

If ANY criterion scores < 7 → return to Step 1 with actionable diagnostic.

### Step 3 – External Oracle
Run `scripts/oracle.sh`. Returns only **PASS** or **FAIL + exit code**.

### Step 4 – Evolution Decision
- **Surrogate FAIL** → Fix and retry (max 15 retries)
- **Surrogate PASS, Oracle FAIL** → Escalate surrogate criteria, retry
- **Oracle PASS** → Commit. Update Evolution Log.

Max rounds: 5 oracle, 15 surrogate.

## Common Pitfalls
- Adding parser support without updating engine-core's domain model first
- Panicking on unexpected XML attributes (use `_` catch-all in match)
- Tight coupling between parser internals and engine-core types
- Tests with inline XML that doesn't match real BPMN modeler output
- Forgetting namespace handling in XML elements

## Evolution Log
| Date | Change | Surrogate Rounds | Oracle Result | Notes |
|---|---|---|---|---|
