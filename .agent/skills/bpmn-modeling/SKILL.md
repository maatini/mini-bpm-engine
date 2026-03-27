---
name: bpmn-modeling
description: Skill for the bpmn-parser crate — parsing BPMN 2.0 XML into ProcessDefinition using quick-xml and serde.
version: 1.0
triggers: ["bpmn xml", "parser", "bpmn-parser", "quick-xml", "modeling"]
author: Maatini
tags: [rust, bpmn, xml, parser, quick-xml]
---

# BPMN PARSER SKILL

## Crate: `bpmn-parser`
Parses BPMN 2.0 XML (from bpmn-js or other modelers) into `engine-core::ProcessDefinition`.

## Dependencies
- `quick-xml = { version = "0.37", features = ["serde", "overlapped-lists"] }`
- `serde` for XML deserialization
- `engine-core` for `ProcessDefinition`, `ProcessDefinitionBuilder`, `BpmnElement`

## Public API
```rust
pub fn parse_bpmn_xml(xml: &str) -> EngineResult<ProcessDefinition>
```

## Supported BPMN Elements
| XML Element | Maps to `BpmnElement` |
|---|---|
| `<startEvent>` | `StartEvent` |
| `<startEvent>` + `<timerEventDefinition>` | `TimerStartEvent(Duration)` |
| `<endEvent>` | `EndEvent` |
| `<serviceTask>` with `data-topic` | `ServiceTask { topic }` |
| `<userTask>` with `data-assignee` | `UserTask(assignee)` |
| `<exclusiveGateway>` with optional `default` | `ExclusiveGateway { default }` |
| `<inclusiveGateway>` | `InclusiveGateway` |
| `<parallelGateway>` | `InclusiveGateway` (temporary mapping) |
| `<task>`, `<scriptTask>`, `<sendTask>`, etc. | `ServiceTask { topic: name }` (generic fallback) |
| `<intermediateThrowEvent>`, `<intermediateCatchEvent>` | `ServiceTask { topic: "event_passthrough" }` |

## Execution Listeners
Parsed from `<extensionElements>`:
```xml
<bpmn:executionListener event="start">
  <bpmn:script scriptFormat="rhai">x = x * 2;</bpmn:script>
</bpmn:executionListener>
```
Supports both process-level and node-level listeners.

## Conditional Flows
```xml
<sequenceFlow id="f1" sourceRef="gw" targetRef="end1">
  <conditionExpression xsi:type="tFormalExpression">amount > 100</conditionExpression>
</sequenceFlow>
```

## Exclusive Gateway Default Flow Resolution
The parser resolves the BPMN `default` attribute (which is a **flow ID**) to the actual **target node ID** using a flow lookup table.

## Key Design Decisions
- Uses `quick-xml` with `overlapped-lists` feature for bpmn-js interleaved output
- All unknown task types map to `ServiceTask` as fallback
- Assignee defaults to `"unassigned"` if not specified
- Timer duration parsed from `PT{n}S` format (basic implementation)
