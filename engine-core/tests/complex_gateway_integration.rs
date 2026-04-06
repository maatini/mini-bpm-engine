use engine_core::engine::WorkflowEngine;
use engine_core::model::{BpmnElement, ProcessDefinitionBuilder};
use engine_core::engine::types::InstanceState;

#[tokio::test]
async fn test_complex_gateway_split_and_default() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("complex_split")
        .node("start", BpmnElement::StartEvent)
        .node(
            "cgw",
            BpmnElement::ComplexGateway {
                join_condition: None,
                default: Some("t3".into()), // fallback
            },
        )
        .node("t1", BpmnElement::UserTask("worker".into()))
        .node("t2", BpmnElement::UserTask("worker".into()))
        .node("t3", BpmnElement::UserTask("worker".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "cgw")
        .conditional_flow("cgw", "t1", "a == true")
        .conditional_flow("cgw", "t2", "b == true")
        .flow("cgw", "t3") // default
        .flow("t1", "end")
        .flow("t2", "end")
        .flow("t3", "end")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(def).await;

    // Test 1: Forking (both conditions match)
    let mut vars1 = std::collections::HashMap::new();
    vars1.insert("a".into(), serde_json::json!(true));
    vars1.insert("b".into(), serde_json::json!(true));
    let inst_id1 = engine.start_instance_with_variables(key, vars1).await.unwrap();

    let tasks1 = engine
        .get_pending_user_tasks()
        .into_iter()
        .filter(|t| t.instance_id == inst_id1)
        .collect::<Vec<_>>();
    assert_eq!(tasks1.len(), 2, "Complex Gateway should fork to t1 and t2");
    let active_nodes: Vec<_> = tasks1.iter().map(|t| t.node_id.as_str()).collect();
    assert!(active_nodes.contains(&"t1") && active_nodes.contains(&"t2"));

    // Test 2: Default Flow
    let inst_id2 = engine.start_instance(key).await.unwrap(); // no variables

    let tasks2 = engine
        .get_pending_user_tasks()
        .into_iter()
        .filter(|t| t.instance_id == inst_id2)
        .collect::<Vec<_>>();
    assert_eq!(tasks2.len(), 1, "Complex Gateway should fallback to default t3");
    assert_eq!(tasks2[0].node_id, "t3");
}

#[tokio::test]
async fn test_complex_gateway_join_condition() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("complex_join")
        .node("start", BpmnElement::StartEvent)
        .node("split", BpmnElement::ParallelGateway)
        .node("t1", BpmnElement::UserTask("worker".into()))
        .node("t2", BpmnElement::UserTask("worker".into()))
        .node("t3", BpmnElement::UserTask("worker".into()))
        .node(
            "cgw_join",
            BpmnElement::ComplexGateway {
                join_condition: Some("ready == true".into()), // Wait for ready flag
                default: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "split")
        .flow("split", "t1")
        .flow("split", "t2")
        .flow("split", "t3")
        .flow("t1", "cgw_join")
        .flow("t2", "cgw_join")
        .flow("t3", "cgw_join")
        .flow("cgw_join", "end")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    let tasks = engine
        .get_pending_user_tasks()
        .into_iter()
        .filter(|t| t.instance_id == inst_id)
        .collect::<Vec<_>>();
    assert_eq!(tasks.len(), 3);

    let t1 = tasks.iter().find(|t| t.node_id == "t1").unwrap();
    let t2 = tasks.iter().find(|t| t.node_id == "t2").unwrap();

    // Complete t1 with ready=false
    let mut vars1 = std::collections::HashMap::new();
    vars1.insert("ready".into(), serde_json::json!(false));
    engine.complete_user_task(t1.task_id, vars1).await.unwrap();

    // The condition ready == true is not met yet (arrived = 1)
    let inst1 = engine.get_instance_details(inst_id).await.unwrap();
    assert!(matches!(inst1.state, InstanceState::ParallelExecution { .. }));

    // Complete t2 with ready=true
    let mut vars2 = std::collections::HashMap::new();
    vars2.insert("ready".into(), serde_json::json!(true));
    engine.complete_user_task(t2.task_id, vars2).await.unwrap();

    // condition is met! It should join and reach end event, completing the instance,
    // EVEN THOUGH t3 is still running / never arrived.
    let inst2 = engine.get_instance_details(inst_id).await.unwrap();
    
    // In our engine, if a branch reaches EndEvent it finishes the branch. But we also
    // remove the join_barrier on early trigger. Wait! If the gateway continues, does it kill the remaining sibling tokens?
    // BPMN says it depends on the workflow structure, but our `all_tokens_completed` logic in `NextAction::Complete`
    // will see that `t3`'s token is still active, so the instance might still be running unless there is a TerminateEndEvent.
    // So the instance state should be parallel or running, BUT the log should contain the complex gateway trigger.
    let log = inst2.audit_log;
    assert!(log.iter().any(|l| l.contains("Complex gateway join condition met early")));
}
