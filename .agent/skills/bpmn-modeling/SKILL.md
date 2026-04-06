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
| `<startEvent>` + `<timerEventDefinition>` | `TimerStartEvent(TimerDefinition)` |
| `<startEvent>` + `<messageEventDefinition>` | `MessageStartEvent { message_name }` |
| `<endEvent>` | `EndEvent` |
| `<endEvent>` + `<terminateEventDefinition>` | `TerminateEndEvent` |
| `<endEvent>` + `<errorEventDefinition>` | `ErrorEndEvent { error_code }` |
| `<serviceTask>` with `data-topic` | `ServiceTask { topic }` |
| `<userTask>` with `data-assignee` | `UserTask(assignee)` |
| `<scriptTask>` with inline `<script>` | `ScriptTask { script }` |
| `<sendTask>` with `data-message-name` | `SendTask { message_name }` |
| `<exclusiveGateway>` with optional `default` | `ExclusiveGateway { default }` |
| `<inclusiveGateway>` | `InclusiveGateway` |
| `<parallelGateway>` | `ParallelGateway` |
| `<eventBasedGateway>` | `EventBasedGateway` |
| `<intermediateCatchEvent>` + `<timerEventDefinition>` | `TimerCatchEvent(TimerDefinition)` |
| `<intermediateCatchEvent>` + `<messageEventDefinition>` | `MessageCatchEvent { message_name }` |
| `<intermediateThrowEvent>` + `<messageEventDefinition>` | `SendTask { message_name }` |
| `<boundaryEvent>` + `<timerEventDefinition>` | `BoundaryTimerEvent { attached_to, timer, cancel_activity }` |
| `<boundaryEvent>` + `<errorEventDefinition>` | `BoundaryErrorEvent { attached_to, error_code }` |
| `<task>` (generic) | `ServiceTask { topic: name }` (fallback) |

## Timer Definition Support
Parses `<timerEventDefinition>` children:
- `<timeDuration>` → `TimerDefinition::Duration` (ISO 8601, e.g. `PT30S`, `PT5M`, `P1DT2H`)
- `<timeDate>` → `TimerDefinition::AbsoluteDate` (ISO 8601 datetime)
- `<timeCycle>` → `TimerDefinition::CronCycle` or `TimerDefinition::RepeatingInterval` (R-notation)

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
- Unknown task types fall back to `ServiceTask` (with name as topic)
- Assignee defaults to `"unassigned"` if not specified
- Full ISO 8601 duration parsing via internal `parse_iso8601_duration()`
- Event sub-processes are detected and logged but handled separately
