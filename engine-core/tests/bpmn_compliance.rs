use engine_core::engine::WorkflowEngine;
use engine_core::model::*;
use std::collections::HashMap;

fn create_engine() -> WorkflowEngine {
    WorkflowEngine::new()
}

async fn complete_task_for_node(engine: &WorkflowEngine, instance_id: &uuid::Uuid, node_id: &str, vars: HashMap<String, serde_json::Value>) {
    let tasks: Vec<_> = engine.get_pending_user_tasks()
        .into_iter()
        .filter(|t| t.instance_id == *instance_id && t.node_id == node_id)
        .collect();
    let task = tasks.first().expect(&format!("No task found for node {}", node_id));
    engine.complete_user_task(task.task_id, vars).await.unwrap();
}

/// BPMN 2.0 Specification Compliance: Exclusive Gateway (XOR)
/// Section 10.5.2 Exclusive Gateway
/// Routing must select exactly one outgoing sequence flow whose condition evaluates to 'true'.
/// If none evaluate to true, the default flow MUST be selected.
#[tokio::test]
async fn compliance_exclusive_gateway_routing() {
    let engine = create_engine();
    
    // Scenario: X > 10 routes to A, otherwise default routes to B
    let def = ProcessDefinitionBuilder::new("xor-compliance")
        .node("start", BpmnElement::StartEvent)
        .node("gw", BpmnElement::ExclusiveGateway { default: Some("taskB".into()) })
        .node("taskA", BpmnElement::UserTask("UserA".into()))
        .node("taskB", BpmnElement::UserTask("UserB".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "gw")
        .conditional_flow("gw", "taskA", "x > 10")
        .flow("gw", "taskB") // This is the default flow
        .flow("taskA", "end")
        .flow("taskB", "end")
        .build()
        .unwrap();
        
    let (def_id, _) = engine.deploy_definition(def).await;
    
    // Execute with x = 15 (Routes to TaskA)
    let inst_id_high = engine.start_instance_with_variables(def_id.clone(), HashMap::from([("x".into(), serde_json::json!(15))])).await.unwrap();
    let details_high = engine.get_instance_details(inst_id_high).await.unwrap();
    assert_eq!(details_high.tokens.values().next().unwrap().current_node, "taskA", "Compliance Violation: Expected condition x > 10 to route to taskA");

    // Execute with x = 5 (Routes to TaskB via default)
    let inst_id_low = engine.start_instance_with_variables(def_id, HashMap::from([("x".into(), serde_json::json!(5))])).await.unwrap();
    let details_low = engine.get_instance_details(inst_id_low).await.unwrap();
    assert_eq!(details_low.tokens.values().next().unwrap().current_node, "taskB", "Compliance Violation: Expected default flow to route to taskB when no conditions met");
}

/// BPMN 2.0 Specification Compliance: Parallel Gateway (AND)
/// Section 10.5.4 Parallel Gateway
/// All incoming sequence flows must produce a token before the gateway can fire (Join).
/// Upon firing, a token must be produced on ALL outgoing sequence flows (Split).
#[tokio::test]
async fn compliance_parallel_gateway_sync() {
    let engine = create_engine();
    
    let def = ProcessDefinitionBuilder::new("and-compliance")
        .node("start", BpmnElement::StartEvent)
        .node("fork", BpmnElement::ParallelGateway)
        .node("taskA", BpmnElement::UserTask("UserA".into()))
        .node("taskB", BpmnElement::UserTask("UserB".into()))
        .node("join", BpmnElement::ParallelGateway)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "fork")
        // Split
        .flow("fork", "taskA")
        .flow("fork", "taskB")
        // Join
        .flow("taskA", "join")
        .flow("taskB", "join")
        .flow("join", "end")
        .build()
        .unwrap();

    let (def_id, _) = engine.deploy_definition(def).await;
    let empty_vars = HashMap::<String, serde_json::Value>::new();
    let inst_id = engine.start_instance_with_variables(def_id, empty_vars).await.unwrap();
    
    // Check parallel split compliance - should spawn two tokens for taskA and taskB
    let details = engine.get_instance_details(inst_id).await.unwrap();
    assert_eq!(details.tokens.len(), 2, "Compliance Violation: Parallel split must produce exact number of outgoing tokens");
    
    let mut current_nodes: Vec<String> = details.tokens.values().map(|t| t.current_node.clone()).collect();
    current_nodes.sort();
    assert_eq!(current_nodes, vec!["taskA", "taskB"]);
    
    // Complete taskA - token should stop at join gateway and wait for sync
    complete_task_for_node(&engine, &inst_id, "taskA", HashMap::<String, serde_json::Value>::new()).await;
    let details2 = engine.get_instance_details(inst_id).await.unwrap();
    assert_eq!(details2.tokens.len(), 1, "Compliance Violation: Token must wait at joint gateway");
    assert_eq!(details2.tokens.values().next().unwrap().current_node, "taskB");

    // Complete taskB - join gateway fires and proceeds to end
    complete_task_for_node(&engine, &inst_id, "taskB", HashMap::<String, serde_json::Value>::new()).await;
    
    let state = engine.get_instance_details(inst_id).await.unwrap().state;
    assert!(matches!(state, engine_core::engine::types::InstanceState::Completed), "Compliance Violation: Parallel Gateway did not join and complete");
}

/// BPMN 2.0 Specification Compliance: Complex Gateway
/// Section 10.5.6 Complex Gateway
/// Join condition evaluates tokens. For example, 2 out of 3 required.
#[tokio::test]
async fn compliance_complex_gateway_activation() {
    let engine = create_engine();
    
    let def = ProcessDefinitionBuilder::new("complex-compliance")
        .node("start", BpmnElement::StartEvent)
        .node("fork", BpmnElement::ParallelGateway)
        .node("taskA", BpmnElement::UserTask("admin".into()))
        .node("taskB", BpmnElement::UserTask("admin".into()))
        .node("taskC", BpmnElement::UserTask("admin".into()))
        .node("complex_join", BpmnElement::ComplexGateway {
            default: None,
            join_condition: Some("tokens_arrived >= 2".into()), // Needs 2 of 3
        })
        .node("end", BpmnElement::EndEvent)
        .flow("start", "fork")
        .flow("fork", "taskA")
        .flow("fork", "taskB")
        .flow("fork", "taskC")
        .flow("taskA", "complex_join")
        .flow("taskB", "complex_join")
        .flow("taskC", "complex_join")
        .flow("complex_join", "end")
        .build()
        .unwrap();

    let (def_id, _) = engine.deploy_definition(def).await;
    let empty_vars = HashMap::<String, serde_json::Value>::new();
    let inst_id = engine.start_instance_with_variables(def_id, empty_vars).await.unwrap();
    
    // 1st token arrives -> tokens_arrived = 1 (condition not met)
    complete_task_for_node(&engine, &inst_id, "taskA", HashMap::from([("tokens_arrived".into(), serde_json::json!(1))])).await;
    let details1 = engine.get_instance_details(inst_id).await.unwrap();
    assert_eq!(details1.tokens.len(), 2, "Still waiting for second token");

    // 2nd token arrives -> tokens_arrived = 2 (condition MET -> fires!)
    complete_task_for_node(&engine, &inst_id, "taskB", HashMap::from([("tokens_arrived".into(), serde_json::json!(2))])).await;
    
    let details2 = engine.get_instance_details(inst_id).await.unwrap();
    // The instance actually completes because our token goes to EndEvent.
    // Let's just check the instance is completed or the remaining tokens.
    // If it completed, tokens is empty. If it didn't complete, tokens.len() == 1.
    // Complex Gateway early termination -> the workflow might complete because token hits end event!
    // But t3 is orphan. In our engine, hitting an end event completes the current token. If another token is alive (t3) it is still running!
    assert!(details2.tokens.values().any(|t| t.current_node == "taskC"), "Remaining orphan token should still be at taskC");
    assert!(details2.audit_log.iter().any(|l| l.contains("Complex gateway join condition met early")), "Complex gateway must have fired early");
}
