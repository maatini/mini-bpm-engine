use std::collections::HashMap;
use std::time::Duration;

use quick_xml::de::from_str;
use serde::Deserialize;

use engine_core::error::{EngineError, EngineResult};
use engine_core::model::{BpmnElement, ListenerEvent, ProcessDefinition, ProcessDefinitionBuilder};

#[derive(Debug, Deserialize)]
struct BpmnExtensionElements {
    #[serde(rename = "executionListener", default)]
    execution_listeners: Vec<BpmnExecutionListener>,
}

#[derive(Debug, Deserialize)]
struct BpmnExecutionListener {
    #[serde(rename = "@event")]
    event: String,
    
    #[serde(rename = "script")]
    script: Option<BpmnScript>,
}

#[derive(Debug, Deserialize)]
struct BpmnScript {
    #[serde(rename = "@scriptFormat")]
    #[allow(dead_code)]
    script_format: String,
    
    #[serde(rename = "$value")]
    content: String,
}

#[derive(Debug, Deserialize)]
struct BpmnDefinitions {
    #[allow(dead_code)]
    #[serde(rename = "@id", default)]
    id: Option<String>,
    
    #[serde(rename = "process", default)]
    processes: Vec<BpmnProcess>,

    #[allow(dead_code)]
    #[serde(rename = "collaboration", default)]
    collaborations: Vec<IgnoredElement>,
    
    #[allow(dead_code)]
    #[serde(rename = "BPMNDiagram", default)]
    bpmndiagrams: Vec<IgnoredElement>,

    #[serde(rename = "message", default)]
    messages: Vec<BpmnMessage>,
    
    #[serde(rename = "error", default)]
    errors: Vec<BpmnErrorDef>,
}

#[derive(Debug, Deserialize)]
struct BpmnMessage {
    #[serde(rename = "@id")]
    id: String,
    #[serde(rename = "@name")]
    name: String,
}

#[derive(Debug, Deserialize)]
struct BpmnErrorDef {
    #[serde(rename = "@id")]
    id: String,
    #[serde(rename = "@name", default)]
    name: Option<String>,
    #[serde(rename = "@errorCode", default)]
    error_code: Option<String>,
}

/// A catch-all struct for elements we want to parse but otherwise ignore.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct IgnoredElement {
    #[serde(rename = "@id", default)]
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BpmnProcess {
    #[serde(rename = "@id")]
    id: String,

    #[serde(rename = "@isExecutable", default)]
    is_executable: Option<bool>,
    
    #[serde(rename = "extensionElements")]
    extension_elements: Option<BpmnExtensionElements>,
    
    #[serde(rename = "startEvent", default)]
    start_events: Vec<BpmnStartEvent>,
    
    #[serde(rename = "endEvent", default)]
    end_events: Vec<BpmnEndEvent>,
    
    #[serde(rename = "serviceTask", default)]
    service_tasks: Vec<BpmnServiceTask>,
    
    #[serde(rename = "userTask", default)]
    user_tasks: Vec<BpmnUserTask>,
    
    #[serde(rename = "sequenceFlow", default)]
    sequence_flows: Vec<BpmnSequenceFlow>,

    // --- Ignored Visual/Data Elements ---
    #[allow(dead_code)]
    #[serde(rename = "dataObjectReference", default)]
    data_object_references: Vec<IgnoredElement>,
    #[allow(dead_code)]
    #[serde(rename = "dataStoreReference", default)]
    data_store_references: Vec<IgnoredElement>,
    #[allow(dead_code)]
    #[serde(rename = "textAnnotation", default)]
    text_annotations: Vec<IgnoredElement>,
    #[allow(dead_code)]
    #[serde(rename = "association", default)]
    associations: Vec<IgnoredElement>,

    // --- Future/Unsupported Elements ---
    #[serde(rename = "subProcess", default)]
    sub_processes: Vec<BpmnSubProcess>,
    
    #[serde(rename = "boundaryEvent", default)]
    boundary_events: Vec<BpmnBoundaryEvent>,

    /// Generic `<task>` elements (bpmn-js default when adding a task).
    #[serde(rename = "task", default)]
    generic_tasks: Vec<BpmnGenericTask>,

    /// Script, send, receive, manual, businessRule tasks — all map to ServiceTask.
    #[serde(rename = "scriptTask", default)]
    script_tasks: Vec<BpmnGenericTask>,
    #[serde(rename = "sendTask", default)]
    send_tasks: Vec<BpmnGenericTask>,
    #[serde(rename = "receiveTask", default)]
    receive_tasks: Vec<BpmnGenericTask>,
    #[serde(rename = "manualTask", default)]
    manual_tasks: Vec<BpmnGenericTask>,
    #[serde(rename = "businessRuleTask", default)]
    business_rule_tasks: Vec<BpmnGenericTask>,
    #[serde(rename = "callActivity", default)]
    call_activities: Vec<BpmnGenericTask>,

    /// Gateways — mapped to proper BpmnElement variants.
    #[serde(rename = "exclusiveGateway", default)]
    exclusive_gateways: Vec<BpmnExclusiveGateway>,
    #[serde(rename = "parallelGateway", default)]
    parallel_gateways: Vec<BpmnGateway>,
    #[serde(rename = "inclusiveGateway", default)]
    inclusive_gateways: Vec<BpmnGateway>,

    /// Intermediate events — treated as pass-through nodes.
    #[serde(rename = "intermediateThrowEvent", default)]
    intermediate_throw_events: Vec<BpmnGenericTask>,
    #[serde(rename = "intermediateCatchEvent", default)]
    intermediate_catch_events: Vec<BpmnIntermediateCatchEvent>,
}

#[derive(Debug, Deserialize)]
struct BpmnStartEvent {
    #[serde(rename = "@id")]
    id: String,
    #[serde(rename = "extensionElements")]
    extension_elements: Option<BpmnExtensionElements>,
    #[serde(rename = "timerEventDefinition")]
    timer_event_definition: Option<BpmnTimerEventDefinition>,
    #[serde(rename = "messageEventDefinition")]
    message_event_definition: Option<BpmnMessageEventDefinition>,
}

#[derive(Debug, Deserialize)]
struct BpmnTimerEventDefinition {
    #[serde(rename = "timeDuration")]
    time_duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BpmnMessageEventDefinition {
    #[serde(rename = "@messageRef")]
    message_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BpmnErrorEventDefinition {
    #[serde(rename = "@errorRef")]
    error_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BpmnEndEvent {
    #[serde(rename = "@id")]
    id: String,
    #[serde(rename = "extensionElements")]
    extension_elements: Option<BpmnExtensionElements>,
    #[serde(rename = "errorEventDefinition")]
    error_event_definition: Option<BpmnErrorEventDefinition>,
}

#[derive(Debug, Deserialize)]
struct BpmnServiceTask {
    #[serde(rename = "@id")]
    id: String,
    #[serde(rename = "extensionElements")]
    extension_elements: Option<BpmnExtensionElements>,
    #[serde(rename = "@data-handler")]
    handler: Option<String>,
    /// Maatini type attribute: "external" means external task.

    /// Topic name for external tasks.
    #[serde(rename = "@data-topic")]
    topic: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BpmnUserTask {
    #[serde(rename = "@id")]
    id: String,
    #[serde(rename = "extensionElements")]
    extension_elements: Option<BpmnExtensionElements>,
    #[serde(rename = "@data-assignee")]
    assignee: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BpmnConditionExpression {
    #[serde(rename = "$value")]
    value: String,
}

#[derive(Debug, Deserialize)]
struct BpmnSequenceFlow {
    #[serde(rename = "@id")]
    _id: String,
    #[serde(rename = "@sourceRef")]
    source_ref: String,
    #[serde(rename = "@targetRef")]
    target_ref: String,
    #[serde(rename = "conditionExpression")]
    condition_expression: Option<BpmnConditionExpression>,
}

/// Generic catch-all for BPMN elements we parse but only need the ID from.
#[derive(Debug, Deserialize)]
struct BpmnGenericTask {
    #[serde(rename = "@id")]
    id: String,
    #[serde(rename = "extensionElements")]
    extension_elements: Option<BpmnExtensionElements>,
    /// Optional name attribute (bpmn-js sometimes sets this).
    #[serde(rename = "@name", default)]
    name: Option<String>,
}

/// Exclusive gateway with optional `default` attribute referencing a sequence flow ID.
#[derive(Debug, Deserialize)]
struct BpmnExclusiveGateway {
    #[serde(rename = "@id")]
    id: String,
    #[serde(rename = "extensionElements")]
    extension_elements: Option<BpmnExtensionElements>,
    /// Per BPMN spec: the ID of the default outgoing sequence flow.
    #[serde(rename = "@default", default)]
    default: Option<String>,
}

/// Generic gateway struct for inclusive and parallel gateways.
#[derive(Debug, Deserialize)]
struct BpmnGateway {
    #[serde(rename = "@id")]
    id: String,
    #[serde(rename = "extensionElements")]
    extension_elements: Option<BpmnExtensionElements>,
}

#[derive(Debug, Deserialize)]
struct BpmnIntermediateCatchEvent {
    #[serde(rename = "@id")]
    id: String,
    #[serde(rename = "extensionElements")]
    extension_elements: Option<BpmnExtensionElements>,
    #[serde(rename = "timerEventDefinition")]
    timer_event_definition: Option<BpmnTimerEventDefinition>,
    #[serde(rename = "messageEventDefinition")]
    message_event_definition: Option<BpmnMessageEventDefinition>,
}

#[derive(Debug, Deserialize)]
struct BpmnBoundaryEvent {
    #[serde(rename = "@id")]
    id: String,
    #[serde(rename = "@attachedToRef")]
    attached_to_ref: String,
    #[serde(rename = "@cancelActivity", default)]
    cancel_activity: Option<bool>,
    #[serde(rename = "timerEventDefinition")]
    timer_event_definition: Option<BpmnTimerEventDefinition>,
    #[serde(rename = "errorEventDefinition")]
    error_event_definition: Option<BpmnErrorEventDefinition>,
}

#[derive(Debug, Deserialize)]
struct BpmnSubProcess {
    #[allow(dead_code)]
    #[serde(rename = "@id")]
    id: String,
}

/// Helper to attach parsed listeners to the builder
fn add_listeners(
    mut builder: ProcessDefinitionBuilder,
    node_id: &str,
    ext_elements: Option<BpmnExtensionElements>,
) -> ProcessDefinitionBuilder {
    if let Some(exts) = ext_elements {
        for l in exts.execution_listeners {
            let evt = match l.event.as_str() {
                "start" => ListenerEvent::Start,
                "end" => ListenerEvent::End,
                _ => continue,
            };
            if let Some(s) = l.script {
                builder = builder.listener(node_id, evt, s.content.trim());
            }
        }
    }
    builder
}

/// Helper to parse basic ISO 8601 durations like PT1H30M, PT5M.
fn parse_iso8601_duration(s: &str) -> Duration {
    let s = s.trim();
    if !s.starts_with("PT") {
        return Duration::from_secs(0);
    }
    let s = &s[2..];
    
    let mut total_secs = 0;
    let mut current_num = String::new();
    
    for c in s.chars() {
        if c.is_ascii_digit() {
            current_num.push(c);
        } else {
            let val = current_num.parse::<u64>().unwrap_or(0);
            match c {
                'H' => total_secs += val * 3600,
                'M' => total_secs += val * 60,
                'S' => total_secs += val,
                _ => {}
            }
            current_num.clear();
        }
    }
    
    Duration::from_secs(total_secs)
}

/// Parses a subset of BPMN 2.0 XML and builds a `ProcessDefinition`.
///
/// Note: Since `quick-xml` expects exact structure, the parsed XML must match
/// the structs above (elements rather than attributes where specified, etc.).
pub fn parse_bpmn_xml(xml: &str) -> EngineResult<ProcessDefinition> {
    let mut defs: BpmnDefinitions = from_str(xml).map_err(|e| {
        EngineError::InvalidDefinition(format!(
            "BPMN XML parsing failed (hint: ensure the XML contains a valid <process> element and supported entities): {}",
            e
        ))
    })?;

    if defs.processes.is_empty() {
        return Err(EngineError::InvalidDefinition("No <process> element found in BPMN XML".to_string()));
    }

    // Find the executable process, or fallback to the first one available
    let executable_idx = defs.processes.iter().position(|p| p.is_executable == Some(true)).unwrap_or(0);
    let process = defs.processes.remove(executable_idx);

    let process_id = process.id.clone();
    let mut builder = ProcessDefinitionBuilder::new(process_id.clone());

    if !process.sub_processes.is_empty() {
        return Err(EngineError::InvalidDefinition("Embedded subprocesses are not yet supported. Please use flat processes.".to_string()));
    }

    // Lookup maps for messages and errors
    let message_lookup: HashMap<String, String> = defs
        .messages
        .iter()
        .map(|m| (m.id.clone(), m.name.clone()))
        .collect();

    let error_lookup: HashMap<String, String> = defs
        .errors
        .iter()
        .map(|e| {
            let code = e.error_code.clone().or_else(|| e.name.clone()).unwrap_or_else(|| e.id.clone());
            (e.id.clone(), code)
        })
        .collect();

    // Process-level listeners
    builder = add_listeners(builder, &process_id, process.extension_elements);

    // 1. Process Start Events
    for start in process.start_events {
        let node_id = start.id.clone();
        if let Some(timer) = start.timer_event_definition {
            let dur = if let Some(time) = timer.time_duration {
                parse_iso8601_duration(&time)
            } else {
                Duration::from_secs(0)
            };
            builder = builder.node(start.id, BpmnElement::TimerStartEvent(dur));
        } else if let Some(msg) = start.message_event_definition {
            let message_name = msg.message_ref
                .and_then(|ref_id| message_lookup.get(&ref_id).cloned())
                .unwrap_or_else(|| "generic_message".into());
            builder = builder.node(start.id, BpmnElement::MessageStartEvent { message_name });
        } else {
            builder = builder.node(start.id, BpmnElement::StartEvent);
        }
        builder = add_listeners(builder, &node_id, start.extension_elements);
    }

    // 2. Process End Events
    for end in process.end_events {
        let node_id = end.id.clone();
        if let Some(err) = end.error_event_definition {
            let error_code = err.error_ref
                .and_then(|ref_id| error_lookup.get(&ref_id).cloned())
                .unwrap_or_else(|| "generic_error".into());
            builder = builder.node(end.id, BpmnElement::ErrorEndEvent { error_code });
        } else {
            builder = builder.node(end.id, BpmnElement::EndEvent);
        }
        builder = add_listeners(builder, &node_id, end.extension_elements);
    }

    // 3. Process Service Tasks (All now use external fetching API via topics)
    for task in process.service_tasks {
        let node_id = task.id.clone();
        // Fallback: use topic, then handler (backward compat), then node_id
        let topic = task.topic
            .or(task.handler)
            .unwrap_or_else(|| task.id.clone());
        builder = builder.node(task.id, BpmnElement::ServiceTask { topic });
        builder = add_listeners(builder, &node_id, task.extension_elements);
    }

    // 4. Process User Tasks
    for task in process.user_tasks {
        let node_id = task.id.clone();
        let assignee = task.assignee.unwrap_or_else(|| "unassigned".into());
        builder = builder.node(task.id, BpmnElement::UserTask(assignee));
        builder = add_listeners(builder, &node_id, task.extension_elements);
    }

    // 5. Generic tasks (bpmn-js default task element)
    //    Also covers scriptTask, sendTask, receiveTask, manualTask,
    //    businessRuleTask, and callActivity — all map to ServiceTask.
    let all_generic_tasks = process.generic_tasks.into_iter()
        .chain(process.script_tasks)
        .chain(process.send_tasks)
        .chain(process.receive_tasks)
        .chain(process.manual_tasks)
        .chain(process.business_rule_tasks)
        .chain(process.call_activities);

    for task in all_generic_tasks {
        let node_id = task.id.clone();
        let topic = task.name.unwrap_or_else(|| task.id.clone());
        builder = builder.node(task.id, BpmnElement::ServiceTask { topic });
        builder = add_listeners(builder, &node_id, task.extension_elements);
    }

    // 6. Build a flow lookup (flow-ID → target-ref) for resolving the
    //    `default` attribute on exclusive gateways.
    let flow_lookup: HashMap<String, String> = process
        .sequence_flows
        .iter()
        .map(|f| (f._id.clone(), f.target_ref.clone()))
        .collect();

    // 6a. Exclusive gateways — resolve `default` flow ID → target node ID
    for gw in process.exclusive_gateways {
        let node_id = gw.id.clone();
        let default_target = gw.default.and_then(|flow_id| flow_lookup.get(&flow_id).cloned());
        builder = builder.node(
            gw.id,
            BpmnElement::ExclusiveGateway { default: default_target },
        );
        builder = add_listeners(builder, &node_id, gw.extension_elements);
    }

    // 6b. Inclusive gateways
    for gw in process.inclusive_gateways {
        let node_id = gw.id.clone();
        builder = builder.node(gw.id, BpmnElement::InclusiveGateway);
        builder = add_listeners(builder, &node_id, gw.extension_elements);
    }

    // 6c. Parallel gateways
    for gw in process.parallel_gateways {
        let node_id = gw.id.clone();
        builder = builder.node(gw.id, BpmnElement::ParallelGateway);
        builder = add_listeners(builder, &node_id, gw.extension_elements);
    }

    // 7. Intermediate catch events 
    for catch_evt in process.intermediate_catch_events {
        let node_id = catch_evt.id.clone();
        if let Some(timer) = catch_evt.timer_event_definition {
            let dur = if let Some(time) = timer.time_duration {
                parse_iso8601_duration(&time)
            } else {
                Duration::from_secs(0)
            };
            builder = builder.node(catch_evt.id, BpmnElement::TimerCatchEvent(dur));
        } else if let Some(msg) = catch_evt.message_event_definition {
            let message_name = msg.message_ref
                .and_then(|ref_id| message_lookup.get(&ref_id).cloned())
                .unwrap_or_else(|| "generic_message".into());
            builder = builder.node(catch_evt.id, BpmnElement::MessageCatchEvent { message_name });
        } else {
            // generic pass through
            builder = builder.node(catch_evt.id, BpmnElement::ServiceTask { topic: "event_passthrough".into() });
        }
        builder = add_listeners(builder, &node_id, catch_evt.extension_elements);
    }

    // 8. Intermediate throw events
    for evt in process.intermediate_throw_events {
        let node_id = evt.id.clone();
        builder = builder.node(evt.id, BpmnElement::ServiceTask { topic: "event_passthrough".into() });
        builder = add_listeners(builder, &node_id, evt.extension_elements);
    }

    // 9. Boundary Events
    for bd in process.boundary_events {
        let node_id = bd.id.clone();
        let attached_to = bd.attached_to_ref.clone();
        // cancelActivity is true by default
        let cancel_activity = bd.cancel_activity.unwrap_or(true);

        if let Some(timer) = bd.timer_event_definition {
            let dur = if let Some(time) = timer.time_duration {
                parse_iso8601_duration(&time)
            } else {
                Duration::from_secs(0)
            };
            builder = builder.node(bd.id, BpmnElement::BoundaryTimerEvent { attached_to, duration: dur, cancel_activity });
        } else if let Some(err) = bd.error_event_definition {
            let error_code = err.error_ref.and_then(|ref_id| error_lookup.get(&ref_id).cloned());
            builder = builder.node(bd.id, BpmnElement::BoundaryErrorEvent { attached_to, error_code });
        } else {
            // Unhandled boundary event, map to noop
            builder = builder.node(bd.id, BpmnElement::ServiceTask { topic: "noop".into() });
        }
        builder = add_listeners(builder, &node_id, None);
    }

    // 8. Process Sequence Flows
    for flow in process.sequence_flows {
        if let Some(cond) = flow.condition_expression {
            builder = builder.conditional_flow(flow.source_ref, flow.target_ref, cond.value.trim());
        } else {
            builder = builder.flow(flow.source_ref, flow.target_ref);
        }
    }

    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_bpmn() {
        let xml = r#"
            <bpmn:definitions id="def1" xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL">
                <process id="proc1">
                    <startEvent id="start1" />
                    <serviceTask id="svc1" data-handler="my_handler" />
                    <userTask id="ut1" data-assignee="alice" />
                    <endEvent id="end1" />
                    <sequenceFlow id="f1" sourceRef="start1" targetRef="svc1" />
                    <sequenceFlow id="f2" sourceRef="svc1" targetRef="ut1" />
                    <sequenceFlow id="f3" sourceRef="ut1" targetRef="end1" />
                </process>
            </bpmn:definitions>
        "#;

        let def = parse_bpmn_xml(xml).unwrap();
        assert_eq!(def.id, "proc1");
        assert!(def.nodes.contains_key("start1"));
        assert!(def.nodes.contains_key("svc1"));
        assert!(def.nodes.contains_key("ut1"));
        assert!(def.nodes.contains_key("end1"));
        
        assert_eq!(def.next_node("start1"), Some("svc1"));
        assert_eq!(def.next_node("svc1"), Some("ut1"));
        assert_eq!(def.next_node("ut1"), Some("end1"));
        
        match def.nodes.get("svc1").unwrap() {
            BpmnElement::ServiceTask { topic } => assert_eq!(topic, "my_handler"),
            _ => panic!("Expected ServiceTask"),
        }
        
        match def.nodes.get("ut1").unwrap() {
            BpmnElement::UserTask(a) => assert_eq!(a, "alice"),
            _ => panic!("Expected UserTask"),
        }
    }

    #[test]
    fn parse_conditional_flows() {
        let xml = r#"
            <definitions id="def1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
                <process id="proc1">
                    <startEvent id="start1" />
                    <exclusiveGateway id="gw1" />
                    <endEvent id="end1" />
                    <endEvent id="end2" />
                    
                    <sequenceFlow id="f1" sourceRef="start1" targetRef="gw1" />
                    
                    <sequenceFlow id="f2" sourceRef="gw1" targetRef="end1">
                        <conditionExpression xsi:type="tFormalExpression">amount &gt; 100</conditionExpression>
                    </sequenceFlow>
                    
                    <sequenceFlow id="f3" sourceRef="gw1" targetRef="end2" />
                </process>
            </definitions>
        "#;

        let def = parse_bpmn_xml(xml).unwrap();

        // Gateway must be parsed as ExclusiveGateway, NOT ServiceTask
        match def.nodes.get("gw1").unwrap() {
            BpmnElement::ExclusiveGateway { default } => assert_eq!(*default, None),
            other => panic!("Expected ExclusiveGateway, got {:?}", other),
        }

        let flows = def.next_nodes("gw1");
        assert_eq!(flows.len(), 2);
        
        let flow1 = flows.iter().find(|f| f.target == "end1").unwrap();
        assert_eq!(flow1.condition, Some("amount > 100".to_string()));
        
        let flow2 = flows.iter().find(|f| f.target == "end2").unwrap();
        assert_eq!(flow2.condition, None);
    }

    #[test]
    fn parse_exclusive_gateway_with_default() {
        let xml = r#"
            <definitions id="def1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
                <process id="proc1">
                    <startEvent id="start1" />
                    <exclusiveGateway id="gw1" default="f3" />
                    <userTask id="ut1" data-assignee="alice" />
                    <userTask id="ut2" data-assignee="bob" />
                    <endEvent id="end1" />
                    
                    <sequenceFlow id="f1" sourceRef="start1" targetRef="gw1" />
                    <sequenceFlow id="f2" sourceRef="gw1" targetRef="ut1">
                        <conditionExpression xsi:type="tFormalExpression">x &gt; 0</conditionExpression>
                    </sequenceFlow>
                    <sequenceFlow id="f3" sourceRef="gw1" targetRef="ut2" />
                    <sequenceFlow id="f4" sourceRef="ut1" targetRef="end1" />
                    <sequenceFlow id="f5" sourceRef="ut2" targetRef="end1" />
                </process>
            </definitions>
        "#;

        let def = parse_bpmn_xml(xml).unwrap();

        // Default attribute must resolve flow "f3" → target "ut2"
        match def.nodes.get("gw1").unwrap() {
            BpmnElement::ExclusiveGateway { default } => {
                assert_eq!(default.as_deref(), Some("ut2"));
            }
            other => panic!("Expected ExclusiveGateway, got {:?}", other),
        }

        // User tasks must be parsed correctly
        match def.nodes.get("ut1").unwrap() {
            BpmnElement::UserTask(a) => assert_eq!(a, "alice"),
            other => panic!("Expected UserTask, got {:?}", other),
        }
        match def.nodes.get("ut2").unwrap() {
            BpmnElement::UserTask(a) => assert_eq!(a, "bob"),
            other => panic!("Expected UserTask, got {:?}", other),
        }
    }

    #[test]
    fn parse_timer_start() {
        let xml = r#"
            <definitions id="def1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
                <process id="proc1">
                    <startEvent id="start1">
                        <timerEventDefinition>
                            <timeDuration>PT60S</timeDuration>
                        </timerEventDefinition>
                    </startEvent>
                    <endEvent id="end1" />
                    <sequenceFlow id="f1" sourceRef="start1" targetRef="end1" />
                </process>
            </definitions>
        "#;

        let def = parse_bpmn_xml(xml).unwrap();
        match def.nodes.get("start1").unwrap() {
            BpmnElement::TimerStartEvent(d) => assert_eq!(d.as_secs(), 60),
            _ => panic!("Expected TimerStartEvent"),
        }
    }

    /// Regression test: bpmn-js generates interleaved elements, e.g.
    /// `startEvent`, `sequenceFlow`, `serviceTask`, `sequenceFlow`, `endEvent`.
    /// quick-xml 0.31 rejected this as "duplicate field `sequenceFlow`".
    /// Fixed by upgrading to quick-xml 0.37 with `overlapped-lists` feature.
    #[test]
    fn parse_interleaved_bpmn_js_output() {
        let xml = r#"
            <bpmn2:definitions id="Definitions_1" xmlns:bpmn2="http://www.omg.org/spec/BPMN/20100524/MODEL">
                <bpmn2:process id="Process_1" isExecutable="true">
                    <bpmn2:startEvent id="StartEvent_1" />
                    <bpmn2:sequenceFlow id="Flow_1" sourceRef="StartEvent_1" targetRef="ServiceTask_1" />
                    <bpmn2:serviceTask id="ServiceTask_1" data-handler="validate" />
                    <bpmn2:sequenceFlow id="Flow_2" sourceRef="ServiceTask_1" targetRef="UserTask_1" />
                    <bpmn2:userTask id="UserTask_1" data-assignee="admin" />
                    <bpmn2:sequenceFlow id="Flow_3" sourceRef="UserTask_1" targetRef="EndEvent_1" />
                    <bpmn2:endEvent id="EndEvent_1" />
                </bpmn2:process>
            </bpmn2:definitions>
        "#;

        let def = parse_bpmn_xml(xml).expect("should parse interleaved BPMN XML");
        assert_eq!(def.id, "Process_1");
        assert_eq!(def.nodes.len(), 4);
        assert_eq!(def.flows.len(), 3);
        assert_eq!(def.next_node("StartEvent_1"), Some("ServiceTask_1"));
        assert_eq!(def.next_node("ServiceTask_1"), Some("UserTask_1"));
        assert_eq!(def.next_node("UserTask_1"), Some("EndEvent_1"));
    }

    #[test]
    fn test_parse_execution_listeners_and_scripts() {
        let xml = r#"
<?xml version="1.0" encoding="UTF-8"?>
<bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" id="Definitions_1">
  <bpmn:process id="Process_1" isExecutable="true">
    <bpmn:extensionElements>
      <bpmn:executionListener event="start">
        <bpmn:script scriptFormat="rhai">
          print("Process Started");
        </bpmn:script>
      </bpmn:executionListener>
    </bpmn:extensionElements>
    
    <bpmn:startEvent id="Start_1" />
    <bpmn:sequenceFlow id="Flow_1" sourceRef="Start_1" targetRef="Task_1" />
    
    <bpmn:serviceTask id="Task_1">
      <bpmn:extensionElements>
        <bpmn:executionListener event="end">
          <bpmn:script scriptFormat="rhai">
            print("Task Ended");
          </bpmn:script>
        </bpmn:executionListener>
      </bpmn:extensionElements>
    </bpmn:serviceTask>
    
    <bpmn:sequenceFlow id="Flow_2" sourceRef="Task_1" targetRef="End_1" />
    <bpmn:endEvent id="End_1" />
  </bpmn:process>
</bpmn:definitions>
"#;
        let p = parse_bpmn_xml(xml).expect("Should parse");
        
        let mut process_listeners = p.listeners.get("Process_1").cloned().unwrap_or_default();
        process_listeners.sort_by_key(|l| match l.event {
            ListenerEvent::Start => 1,
            ListenerEvent::End => 2,
        });
        
        assert_eq!(process_listeners.len(), 1);
        assert!(matches!(process_listeners[0].event, ListenerEvent::Start));
        assert_eq!(process_listeners[0].script, "print(\"Process Started\");");

        let task_listeners = p.listeners.get("Task_1").cloned().unwrap_or_default();
        assert_eq!(task_listeners.len(), 1);
        assert!(matches!(task_listeners[0].event, ListenerEvent::End));
        assert_eq!(task_listeners[0].script, "print(\"Task Ended\");");
    }

    #[test]
    fn parse_parallel_gateway() {
        let xml = r#"
            <definitions id="def1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
                <process id="proc1">
                    <startEvent id="start1" />
                    <parallelGateway id="gw1" />
                    <endEvent id="end1" />
                    <endEvent id="end2" />
                    <sequenceFlow id="f1" sourceRef="start1" targetRef="gw1" />
                    <sequenceFlow id="f2" sourceRef="gw1" targetRef="end1" />
                    <sequenceFlow id="f3" sourceRef="gw1" targetRef="end2" />
                </process>
            </definitions>
        "#;

        let def = parse_bpmn_xml(xml).unwrap();
        match def.nodes.get("gw1").unwrap() {
            BpmnElement::ParallelGateway => {}
            other => panic!("Expected ParallelGateway, got {:?}", other),
        }
        
        let flows = def.next_nodes("gw1");
        assert_eq!(flows.len(), 2);
    }

    #[test]
    fn test_parse_iso8601_duration() {
        assert_eq!(parse_iso8601_duration("PT15S"), Duration::from_secs(15));
        assert_eq!(parse_iso8601_duration("PT5M"), Duration::from_secs(300));
        assert_eq!(parse_iso8601_duration("PT1H30M"), Duration::from_secs(5400));
        assert_eq!(parse_iso8601_duration("PT2H15M30S"), Duration::from_secs(8130));
        assert_eq!(parse_iso8601_duration("  PT10M  "), Duration::from_secs(600));
        assert_eq!(parse_iso8601_duration("invalid"), Duration::from_secs(0));
    }

    #[test]
    fn test_invalid_xml_error_message() {
        let xml = "<bpmn:definitions><invalid_element /></bpmn:definitions>";
        let res = parse_bpmn_xml(xml);
        assert!(res.is_err());
        let err = res.unwrap_err();
        match err {
            EngineError::InvalidDefinition(msg) => {
                assert!(msg.contains("No <process> element found"));
            }
            _ => panic!("Expected InvalidDefinition error"),
        }
    }

    #[test]
    fn test_malformed_xml_error_message() {
        let xml = "<bpmn:definitions><process id=\"p1\">Unclosed tag";
        let res = parse_bpmn_xml(xml);
        assert!(res.is_err());
        let err = res.unwrap_err();
        match err {
            EngineError::InvalidDefinition(msg) => {
                assert!(msg.contains("BPMN XML parsing failed"));
            }
            _ => panic!("Expected InvalidDefinition error"),
        }
    }

    #[test]
    fn reject_unsupported_subprocess() {
        let xml = r#"
            <bpmn:definitions id="def1" xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL">
                <bpmn:process id="Process_1" isExecutable="true">
                    <bpmn:subProcess id="SubProcess_1">
                        <bpmn:startEvent id="Start_1" />
                    </bpmn:subProcess>
                </bpmn:process>
            </bpmn:definitions>
        "#;
        let res = parse_bpmn_xml(xml);
        assert!(matches!(res, Err(EngineError::InvalidDefinition(msg)) if msg.contains("Embedded subprocesses are not yet supported")));
    }

    #[test]
    fn parse_complex_bpmn_with_diagram_and_collaboration() {
        let xml = r#"
            <bpmn:definitions id="def1" xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI">
                <bpmn:collaboration id="Collaboration_1">
                    <bpmn:participant id="Participant_1" processRef="Process_1" />
                </bpmn:collaboration>
                <bpmn:process id="Process_1" isExecutable="true">
                    <bpmn:startEvent id="Start_1" />
                    <bpmn:endEvent id="End_1" />
                    <bpmn:sequenceFlow id="Flow_1" sourceRef="Start_1" targetRef="End_1" />
                    <bpmn:dataObjectReference id="DataObj_1" />
                    <bpmn:textAnnotation id="Text_1" />
                </bpmn:process>
                <bpmndi:BPMNDiagram id="BPMNDiagram_1">
                    <bpmndi:BPMNPlane id="BPMNPlane_1" bpmnElement="Collaboration_1" />
                </bpmndi:BPMNDiagram>
            </bpmn:definitions>
        "#;
        let def = parse_bpmn_xml(xml).expect("Should successfully skip collaboration and diagrams");
        assert_eq!(def.id, "Process_1");
        assert!(def.nodes.contains_key("Start_1"));
        assert!(def.nodes.contains_key("End_1"));
    }

    #[test]
    fn parse_massive_xml_robustness_test() {
        // Generate a 10,000 node XML programmatically to test OOM and stack overflow protection
        let mut xml = String::with_capacity(1_000_000);
        xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
            <bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" id="Def_massive">
              <bpmn:process id="Proc_massive" isExecutable="true">
                <bpmn:startEvent id="start" />
        "#);

        for i in 0..10_000 {
            xml.push_str(&format!(r#"<bpmn:serviceTask id="svc_{i}" />"#));
            let source = if i == 0 { "start".to_string() } else { format!("svc_{}", i - 1) };
            xml.push_str(&format!(r#"<bpmn:sequenceFlow id="f_{i}" sourceRef="{source}" targetRef="svc_{i}" />"#));
        }

        xml.push_str(r#"
                <bpmn:endEvent id="end" />
                <bpmn:sequenceFlow id="f_last" sourceRef="svc_9999" targetRef="end" />
              </bpmn:process>
            </bpmn:definitions>
        "#);

        let def = parse_bpmn_xml(&xml).expect("Should parse mass XML without OOM or Stack Overflow");
        assert_eq!(def.id, "Proc_massive");
        assert_eq!(def.nodes.len(), 10_002); // start + 10k svc + end
        assert_eq!(def.flows.len(), 10_001); // 10k flows + 1 last
    }
}
