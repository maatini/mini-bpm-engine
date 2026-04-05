use crate::parser::parse_bpmn_xml;
use engine_core::model::{BpmnElement, ListenerEvent};

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

// ---- WP-5: Additional test coverage ----

#[test]
fn parse_parallel_gateway() {
    let xml = r#"
        <definitions id="def1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
            <process id="proc1">
                <startEvent id="s1" />
                <parallelGateway id="fork" />
                <serviceTask id="a" data-topic="task_a" />
                <serviceTask id="b" data-topic="task_b" />
                <parallelGateway id="join" />
                <endEvent id="e1" />
                <sequenceFlow id="f1" sourceRef="s1" targetRef="fork" />
                <sequenceFlow id="f2" sourceRef="fork" targetRef="a" />
                <sequenceFlow id="f3" sourceRef="fork" targetRef="b" />
                <sequenceFlow id="f4" sourceRef="a" targetRef="join" />
                <sequenceFlow id="f5" sourceRef="b" targetRef="join" />
                <sequenceFlow id="f6" sourceRef="join" targetRef="e1" />
            </process>
        </definitions>
    "#;

    let def = parse_bpmn_xml(xml).unwrap();
    assert!(matches!(def.nodes.get("fork").unwrap(), BpmnElement::ParallelGateway));
    assert!(matches!(def.nodes.get("join").unwrap(), BpmnElement::ParallelGateway));
    assert_eq!(def.next_nodes("fork").len(), 2);
}

#[test]
fn parse_inclusive_gateway() {
    let xml = r#"
        <definitions id="def1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
            <process id="proc1">
                <startEvent id="s1" />
                <inclusiveGateway id="gw1" />
                <endEvent id="e1" />
                <endEvent id="e2" />
                <sequenceFlow id="f1" sourceRef="s1" targetRef="gw1" />
                <sequenceFlow id="f2" sourceRef="gw1" targetRef="e1">
                    <conditionExpression xsi:type="tFormalExpression">x &gt; 0</conditionExpression>
                </sequenceFlow>
                <sequenceFlow id="f3" sourceRef="gw1" targetRef="e2">
                    <conditionExpression xsi:type="tFormalExpression">y &gt; 0</conditionExpression>
                </sequenceFlow>
            </process>
        </definitions>
    "#;

    let def = parse_bpmn_xml(xml).unwrap();
    assert!(matches!(def.nodes.get("gw1").unwrap(), BpmnElement::InclusiveGateway));
    assert_eq!(def.next_nodes("gw1").len(), 2);
}

#[test]
fn parse_message_start_event() {
    let xml = r#"
        <definitions id="def1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
            <message id="msg1" name="OrderReceived" />
            <process id="proc1">
                <startEvent id="s1">
                    <messageEventDefinition messageRef="msg1" />
                </startEvent>
                <endEvent id="e1" />
                <sequenceFlow id="f1" sourceRef="s1" targetRef="e1" />
            </process>
        </definitions>
    "#;

    let def = parse_bpmn_xml(xml).unwrap();
    match def.nodes.get("s1").unwrap() {
        BpmnElement::MessageStartEvent { message_name } => {
            assert_eq!(message_name, "OrderReceived");
        }
        other => panic!("Expected MessageStartEvent, got {:?}", other),
    }
}

#[test]
fn parse_message_catch_event() {
    let xml = r#"
        <definitions id="def1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
            <message id="msg1" name="PaymentConfirmed" />
            <process id="proc1">
                <startEvent id="s1" />
                <intermediateCatchEvent id="wait_payment">
                    <messageEventDefinition messageRef="msg1" />
                </intermediateCatchEvent>
                <endEvent id="e1" />
                <sequenceFlow id="f1" sourceRef="s1" targetRef="wait_payment" />
                <sequenceFlow id="f2" sourceRef="wait_payment" targetRef="e1" />
            </process>
        </definitions>
    "#;

    let def = parse_bpmn_xml(xml).unwrap();
    match def.nodes.get("wait_payment").unwrap() {
        BpmnElement::MessageCatchEvent { message_name } => {
            assert_eq!(message_name, "PaymentConfirmed");
        }
        other => panic!("Expected MessageCatchEvent, got {:?}", other),
    }
}

#[test]
fn parse_error_end_event() {
    let xml = r#"
        <definitions id="def1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
            <error id="err1" errorCode="VALIDATION_FAILED" />
            <process id="proc1">
                <startEvent id="s1" />
                <endEvent id="e1">
                    <errorEventDefinition errorRef="err1" />
                </endEvent>
                <sequenceFlow id="f1" sourceRef="s1" targetRef="e1" />
            </process>
        </definitions>
    "#;

    let def = parse_bpmn_xml(xml).unwrap();
    match def.nodes.get("e1").unwrap() {
        BpmnElement::ErrorEndEvent { error_code } => {
            assert_eq!(error_code, "VALIDATION_FAILED");
        }
        other => panic!("Expected ErrorEndEvent, got {:?}", other),
    }
}

#[test]
fn parse_timer_catch_event() {
    let xml = r#"
        <definitions id="def1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
            <process id="proc1">
                <startEvent id="s1" />
                <intermediateCatchEvent id="wait">
                    <timerEventDefinition>
                        <timeDuration>PT30S</timeDuration>
                    </timerEventDefinition>
                </intermediateCatchEvent>
                <endEvent id="e1" />
                <sequenceFlow id="f1" sourceRef="s1" targetRef="wait" />
                <sequenceFlow id="f2" sourceRef="wait" targetRef="e1" />
            </process>
        </definitions>
    "#;

    let def = parse_bpmn_xml(xml).unwrap();
    match def.nodes.get("wait").unwrap() {
        BpmnElement::TimerCatchEvent(d) => assert_eq!(d.as_secs(), 30),
        other => panic!("Expected TimerCatchEvent, got {:?}", other),
    }
}

#[test]
fn parse_boundary_timer_event() {
    let xml = r#"
        <definitions id="def1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
            <process id="proc1">
                <startEvent id="s1" />
                <serviceTask id="task1" data-topic="long_task" />
                <boundaryEvent id="timeout" attachedToRef="task1" cancelActivity="true">
                    <timerEventDefinition>
                        <timeDuration>PT5M</timeDuration>
                    </timerEventDefinition>
                </boundaryEvent>
                <endEvent id="e1" />
                <endEvent id="e2" />
                <sequenceFlow id="f1" sourceRef="s1" targetRef="task1" />
                <sequenceFlow id="f2" sourceRef="task1" targetRef="e1" />
                <sequenceFlow id="f3" sourceRef="timeout" targetRef="e2" />
            </process>
        </definitions>
    "#;

    let def = parse_bpmn_xml(xml).unwrap();
    match def.nodes.get("timeout").unwrap() {
        BpmnElement::BoundaryTimerEvent { attached_to, duration, cancel_activity } => {
            assert_eq!(attached_to, "task1");
            assert_eq!(duration.as_secs(), 300);
            assert!(*cancel_activity);
        }
        other => panic!("Expected BoundaryTimerEvent, got {:?}", other),
    }
}

#[test]
fn parse_boundary_error_event() {
    let xml = r#"
        <definitions id="def1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
            <error id="err1" errorCode="PAYMENT_FAILED" />
            <process id="proc1">
                <startEvent id="s1" />
                <serviceTask id="task1" data-topic="charge" />
                <boundaryEvent id="on_error" attachedToRef="task1">
                    <errorEventDefinition errorRef="err1" />
                </boundaryEvent>
                <endEvent id="e1" />
                <endEvent id="e2" />
                <sequenceFlow id="f1" sourceRef="s1" targetRef="task1" />
                <sequenceFlow id="f2" sourceRef="task1" targetRef="e1" />
                <sequenceFlow id="f3" sourceRef="on_error" targetRef="e2" />
            </process>
        </definitions>
    "#;

    let def = parse_bpmn_xml(xml).unwrap();
    match def.nodes.get("on_error").unwrap() {
        BpmnElement::BoundaryErrorEvent { attached_to, error_code } => {
            assert_eq!(attached_to, "task1");
            assert_eq!(error_code.as_deref(), Some("PAYMENT_FAILED"));
        }
        other => panic!("Expected BoundaryErrorEvent, got {:?}", other),
    }
}

#[test]
fn parse_picks_executable_process() {
    let xml = r#"
        <definitions id="def1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
            <process id="collab_proc" isExecutable="false">
                <startEvent id="cs1" />
                <endEvent id="ce1" />
                <sequenceFlow id="cf1" sourceRef="cs1" targetRef="ce1" />
            </process>
            <process id="main_proc" isExecutable="true">
                <startEvent id="s1" />
                <endEvent id="e1" />
                <sequenceFlow id="f1" sourceRef="s1" targetRef="e1" />
            </process>
        </definitions>
    "#;

    let def = parse_bpmn_xml(xml).unwrap();
    assert_eq!(def.id, "main_proc", "Should pick the executable process");
}

#[test]
fn parse_duration_rejects_invalid_input() {
    // P1D (days not supported), empty, garbage — all must fail
    let template = |dur: &str| format!(r#"
        <definitions id="def1" xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL">
            <process id="proc1">
                <startEvent id="s1">
                    <timerEventDefinition>
                        <timeDuration>{}</timeDuration>
                    </timerEventDefinition>
                </startEvent>
                <endEvent id="e1" />
                <sequenceFlow id="f1" sourceRef="s1" targetRef="e1" />
            </process>
        </definitions>"#, dur);

    // Valid inputs must still work
    assert!(parse_bpmn_xml(&template("PT5S")).is_ok());
    assert!(parse_bpmn_xml(&template("PT1H30M")).is_ok());
    assert!(parse_bpmn_xml(&template("PT0S")).is_ok());

    // Invalid inputs must now fail
    assert!(parse_bpmn_xml(&template("P1D")).is_err(), "P1D should be rejected");
    assert!(parse_bpmn_xml(&template("")).is_err(), "Empty duration should be rejected");
    assert!(parse_bpmn_xml(&template("HELLO")).is_err(), "Garbage should be rejected");
    assert!(parse_bpmn_xml(&template("PT")).is_err(), "PT without value should be rejected");
}
