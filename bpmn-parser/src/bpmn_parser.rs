use std::time::Duration;

use quick_xml::de::from_str;
use serde::Deserialize;

use engine_core::error::{EngineError, EngineResult};
use engine_core::model::{BpmnElement, ProcessDefinition, ProcessDefinitionBuilder};

#[derive(Debug, Deserialize)]
struct BpmnDefinitions {
    #[allow(dead_code)]
    #[serde(rename = "@id")]
    id: String,
    process: BpmnProcess,
}

#[derive(Debug, Deserialize)]
struct BpmnProcess {
    #[serde(rename = "@id")]
    id: String,
    
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
}

#[derive(Debug, Deserialize)]
struct BpmnStartEvent {
    #[serde(rename = "@id")]
    id: String,
    #[serde(rename = "timerEventDefinition")]
    timer_event_definition: Option<BpmnTimerEventDefinition>,
}

#[derive(Debug, Deserialize)]
struct BpmnTimerEventDefinition {
    #[serde(rename = "timeDuration")]
    time_duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BpmnEndEvent {
    #[serde(rename = "@id")]
    id: String,
}

#[derive(Debug, Deserialize)]
struct BpmnServiceTask {
    #[serde(rename = "@id")]
    id: String,
    #[serde(rename = "@data-handler")]
    handler: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BpmnUserTask {
    #[serde(rename = "@id")]
    id: String,
    #[serde(rename = "@data-assignee")]
    assignee: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BpmnSequenceFlow {
    #[serde(rename = "@id")]
    _id: String,
    #[serde(rename = "@sourceRef")]
    source_ref: String,
    #[serde(rename = "@targetRef")]
    target_ref: String,
}

/// Parses a subset of BPMN 2.0 XML and builds a `ProcessDefinition`.
///
/// Note: Since `quick-xml` expects exact structure, the parsed XML must match
/// the structs above (elements rather than attributes where specified, etc.).
pub fn parse_bpmn_xml(xml: &str) -> EngineResult<ProcessDefinition> {
    let defs: BpmnDefinitions = from_str(xml).map_err(|e| {
        EngineError::InvalidDefinition(format!("Failed to parse BPMN XML: {}", e))
    })?;

    let process_id = defs.process.id.clone();
    let mut builder = ProcessDefinitionBuilder::new(process_id);

    // 1. Process Start Events
    for start in defs.process.start_events {
        if let Some(timer) = start.timer_event_definition {
            // Parse duration from PTnHnS
            // Very basic implementation: just looking for "PT{secs}S"
            let dur = if let Some(time) = timer.time_duration {
                let text = time.replace("PT", "").replace("S", "");
                let secs: u64 = text.parse().unwrap_or(0);
                Duration::from_secs(secs)
            } else {
                Duration::from_secs(0)
            };
            builder = builder.node(start.id, BpmnElement::TimerStartEvent(dur));
        } else {
            builder = builder.node(start.id, BpmnElement::StartEvent);
        }
    }

    // 2. Process End Events
    for end in defs.process.end_events {
        builder = builder.node(end.id, BpmnElement::EndEvent);
    }

    // 3. Process Service Tasks
    for task in defs.process.service_tasks {
        let handler = task.handler.unwrap_or_else(|| "default_handler".into());
        builder = builder.node(task.id, BpmnElement::ServiceTask(handler));
    }

    // 4. Process User Tasks
    for task in defs.process.user_tasks {
        let assignee = task.assignee.unwrap_or_else(|| "unassigned".into());
        builder = builder.node(task.id, BpmnElement::UserTask(assignee));
    }

    // 5. Process Sequence Flows
    for flow in defs.process.sequence_flows {
        builder = builder.flow(flow.source_ref, flow.target_ref);
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
        
        assert_eq!(def.flows.get("start1"), Some(&"svc1".to_string()));
        assert_eq!(def.flows.get("svc1"), Some(&"ut1".to_string()));
        assert_eq!(def.flows.get("ut1"), Some(&"end1".to_string()));
        
        match def.nodes.get("svc1").unwrap() {
            BpmnElement::ServiceTask(h) => assert_eq!(h, "my_handler"),
            _ => panic!("Expected ServiceTask"),
        }
        
        match def.nodes.get("ut1").unwrap() {
            BpmnElement::UserTask(a) => assert_eq!(a, "alice"),
            _ => panic!("Expected UserTask"),
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
}
