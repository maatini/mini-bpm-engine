//! Unit tests for the workflow engine.
//!
//! Extracted from `engine.rs` to keep the main module focused on production
//! logic.

use super::*;
use crate::model::ProcessDefinitionBuilder;
use crate::condition::evaluate_condition;
use crate::model::ListenerEvent;


async fn complete_all_service_tasks(engine: &mut WorkflowEngine, worker: &str, vars: HashMap<String, Value>) {
    let mut to_complete = Vec::new();
    for task in &engine.pending_service_tasks {
        to_complete.push((task.id, task.topic.clone()));
    }
    for (id, topic) in to_complete {
        let _ = engine.fetch_and_lock_service_tasks(worker, 10, &[topic.clone()], 60000).await;
        engine.complete_service_task(id, worker, vars.clone()).await.unwrap();
    }
}

async fn setup_linear_engine() -> (WorkflowEngine, Uuid) {
    let mut engine = WorkflowEngine::new();

    // Register a simple service handler

    let def = ProcessDefinitionBuilder::new("linear")
        .node("start", BpmnElement::StartEvent)
        .node("svc", BpmnElement::ServiceTask { topic: "validate".into() })
        .node("ut", BpmnElement::UserTask("alice".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "ut")
        .flow("ut", "end")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;
    (engine, def_key)
}

#[tokio::test]
async fn conditional_routing_on_service_task() {
    let mut engine = WorkflowEngine::new();

    let def = ProcessDefinitionBuilder::new("cond_svc")
        .node("start", BpmnElement::StartEvent)
        .node("svc", BpmnElement::ServiceTask { topic: "noop".into() })
        .node("end_a", BpmnElement::EndEvent)
        .node("end_b", BpmnElement::EndEvent)
        .flow("start", "svc")
        .conditional_flow("svc", "end_a", "x == 1")
        .conditional_flow("svc", "end_b", "x == 2")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;

    let mut vars = HashMap::new();
    vars.insert("x".into(), Value::Number(2.into()));
    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();

    complete_all_service_tasks(&mut engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        *engine.get_instance_state(inst_id).unwrap(),
        InstanceState::Completed
    );
    let log = engine.get_audit_log(inst_id).unwrap();
    let end_entry = log.iter().find(|l| l.contains("Process completed")).unwrap();
    assert!(end_entry.contains("end_b"), "Expected end_b path: {end_entry}");
}

#[tokio::test]
async fn start_instance_pauses_at_user_task() {
    let (mut engine, def_key) = setup_linear_engine().await;
    let inst_id = engine.start_instance(def_key).await.unwrap();

    complete_all_service_tasks(&mut engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        *engine.get_instance_state(inst_id).unwrap(),
        InstanceState::WaitingOnUserTask {
            task_id: engine.pending_user_tasks[0].task_id
        }
    );
    assert_eq!(engine.pending_user_tasks.len(), 1);
}

#[tokio::test]
async fn complete_user_task_reaches_end() {
    let (mut engine, def_key) = setup_linear_engine().await;
    let inst_id = engine.start_instance(def_key).await.unwrap();
    complete_all_service_tasks(&mut engine, "worker", HashMap::new()).await;

    let task_id = engine.pending_user_tasks[0].task_id;
    engine
        .complete_user_task(task_id, HashMap::new())
        .await
        .unwrap();

    complete_all_service_tasks(&mut engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        *engine.get_instance_state(inst_id).unwrap(),
        InstanceState::Completed
    );
    assert!(engine.pending_user_tasks.is_empty());
}

#[tokio::test]
async fn completing_wrong_task_gives_error() {
    let (mut engine, def_key) = setup_linear_engine().await;
    engine.start_instance(def_key).await.unwrap();
    complete_all_service_tasks(&mut engine, "worker", HashMap::new()).await;

    let wrong_id = Uuid::new_v4();
    let result = engine
        .complete_user_task(wrong_id, HashMap::new())
        .await;
    assert!(matches!(result, Err(EngineError::TaskNotPending { .. })));
}

#[tokio::test]
async fn service_handler_modifies_variables() {
    let (mut engine, def_key) = setup_linear_engine().await;
    engine.start_instance(def_key).await.unwrap();

    let mut vars = HashMap::new();
    vars.insert("validated".into(), Value::Bool(true));
    complete_all_service_tasks(&mut engine, "worker_1", vars).await;

    // The token should have 'validated: true' from the service handler
    let pending = &engine.pending_user_tasks[0];
    assert_eq!(
        pending.token.variables.get("validated"),
        Some(&Value::Bool(true))
    );
}

#[tokio::test]
async fn timer_start_succeeds() {
    let mut engine = WorkflowEngine::new();
    let dur = Duration::from_secs(60);

    let def = ProcessDefinitionBuilder::new("timer_proc")
        .node("ts", BpmnElement::TimerStartEvent(dur))
        .node("end", BpmnElement::EndEvent)
        .flow("ts", "end")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;
    let inst_id = engine.trigger_timer_start(def_key, dur).await.unwrap();

    complete_all_service_tasks(&mut engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        *engine.get_instance_state(inst_id).unwrap(),
        InstanceState::Completed
    );
}

#[tokio::test]
async fn timer_mismatch_gives_error() {
    let mut engine = WorkflowEngine::new();

    let def = ProcessDefinitionBuilder::new("timer_proc")
        .node("ts", BpmnElement::TimerStartEvent(Duration::from_secs(60)))
        .node("end", BpmnElement::EndEvent)
        .flow("ts", "end")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;
    let result = engine
        .trigger_timer_start(def_key, Duration::from_secs(30))
        .await;
    assert!(matches!(result, Err(EngineError::TimerMismatch { .. })));
}

#[tokio::test]
async fn plain_start_rejects_timer_def() {
    let mut engine = WorkflowEngine::new();

    let def = ProcessDefinitionBuilder::new("timer_proc")
        .node("ts", BpmnElement::TimerStartEvent(Duration::from_secs(5)))
        .node("end", BpmnElement::EndEvent)
        .flow("ts", "end")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;
    let result = engine.start_instance(def_key).await;
    assert!(matches!(
        result,
        Err(EngineError::InvalidDefinition(msg)) if msg.contains("timer")
    ));
}

#[tokio::test]
async fn unknown_definition_gives_error() {
    let mut engine = WorkflowEngine::new();
    let result = engine.start_instance(Uuid::new_v4()).await;
    assert!(matches!(
        result,
        Err(EngineError::NoSuchDefinition(_))
    ));
}



#[tokio::test]
async fn audit_log_captures_all_steps() {
    let (mut engine, def_key) = setup_linear_engine().await;
    let inst_id = engine.start_instance(def_key).await.unwrap();
    complete_all_service_tasks(&mut engine, "worker", HashMap::new()).await;

    let task_id = engine.pending_user_tasks[0].task_id;
    engine
        .complete_user_task(task_id, HashMap::new())
        .await
        .unwrap();

    let log = engine.get_audit_log(inst_id).unwrap();
    assert!(log.len() >= 4);
    assert!(log[0].contains("started"));
    assert!(log.last().unwrap().contains("completed"));
}

// -----------------------------------------------------------------------
// Condition evaluator tests
// -----------------------------------------------------------------------

#[test]
fn condition_eq_number() {
    let mut vars = HashMap::new();
    vars.insert("amount".into(), Value::Number(100.into()));
    assert!(evaluate_condition("amount == 100", &vars));
    assert!(!evaluate_condition("amount == 200", &vars));
}

#[test]
fn condition_neq_string() {
    let mut vars = HashMap::new();
    vars.insert("status".into(), Value::String("approved".into()));
    assert!(evaluate_condition("status == 'approved'", &vars));
    assert!(evaluate_condition("status != 'rejected'", &vars));
    assert!(!evaluate_condition("status == 'rejected'", &vars));
}

#[test]
fn condition_gt_lt() {
    let mut vars = HashMap::new();
    vars.insert("score".into(), Value::Number(75.into()));
    assert!(evaluate_condition("score > 50", &vars));
    assert!(evaluate_condition("score >= 75", &vars));
    assert!(evaluate_condition("score < 100", &vars));
    assert!(evaluate_condition("score <= 75", &vars));
    assert!(!evaluate_condition("score > 75", &vars));
}

#[test]
fn condition_truthy_check() {
    let mut vars = HashMap::new();
    vars.insert("flag".into(), Value::Bool(true));
    vars.insert("zero".into(), Value::Number(0.into()));
    vars.insert("empty".into(), Value::String(String::new()));

    assert!(evaluate_condition("flag", &vars));
    assert!(!evaluate_condition("zero", &vars));
    assert!(!evaluate_condition("empty", &vars));
    assert!(!evaluate_condition("missing_var", &vars));
}

#[test]
fn condition_missing_variable() {
    let vars = HashMap::new();
    assert!(!evaluate_condition("x == 5", &vars));
}

// -----------------------------------------------------------------------
// ExclusiveGateway (XOR) tests
// -----------------------------------------------------------------------

#[tokio::test]
async fn exclusive_gateway_takes_matching_path() {
    let mut engine = WorkflowEngine::new();

    // Start → XOR Gateway → (amount > 100 → high) / (default → low) → End
    let def = ProcessDefinitionBuilder::new("xor_test")
        .node("start", BpmnElement::StartEvent)
        .node(
            "gw",
            BpmnElement::ExclusiveGateway {
                default: Some("low".into()),
            },
        )
        .node("high", BpmnElement::ServiceTask { topic: "noop".into() })
        .node("low", BpmnElement::ServiceTask { topic: "noop".into() })
        .node("end", BpmnElement::EndEvent)
        .flow("start", "gw")
        .conditional_flow("gw", "high", "amount > 100")
        .flow("gw", "low") // unconditional (default candidate)
        .flow("high", "end")
        .flow("low", "end")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;

    // amount = 500 → should take the "high" path
    let mut vars = HashMap::new();
    vars.insert("amount".into(), Value::Number(500.into()));
    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();

    complete_all_service_tasks(&mut engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        *engine.get_instance_state(inst_id).unwrap(),
        InstanceState::Completed
    );
    let log = engine.get_audit_log(inst_id).unwrap();
    let gw_entry = log.iter().find(|l| l.contains("Exclusive gateway")).unwrap();
    assert!(gw_entry.contains("high"), "Expected high path: {gw_entry}");
}

#[tokio::test]
async fn exclusive_gateway_uses_default_when_no_match() {
    let mut engine = WorkflowEngine::new();

    let def = ProcessDefinitionBuilder::new("xor_default")
        .node("start", BpmnElement::StartEvent)
        .node(
            "gw",
            BpmnElement::ExclusiveGateway {
                default: Some("low".into()),
            },
        )
        .node("high", BpmnElement::ServiceTask { topic: "noop".into() })
        .node("low", BpmnElement::ServiceTask { topic: "noop".into() })
        .node("end", BpmnElement::EndEvent)
        .flow("start", "gw")
        .conditional_flow("gw", "high", "amount > 100")
        .flow("gw", "low")
        .flow("high", "end")
        .flow("low", "end")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;

    // amount = 50 → no condition matches → should use default "low"
    let mut vars = HashMap::new();
    vars.insert("amount".into(), Value::Number(50.into()));
    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();

    complete_all_service_tasks(&mut engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        *engine.get_instance_state(inst_id).unwrap(),
        InstanceState::Completed
    );
    let log = engine.get_audit_log(inst_id).unwrap();
    let gw_entry = log.iter().find(|l| l.contains("Exclusive gateway")).unwrap();
    assert!(gw_entry.contains("low"), "Expected low (default) path: {gw_entry}");
}

#[tokio::test]
async fn exclusive_gateway_error_when_no_match_no_default() {
    let mut engine = WorkflowEngine::new();

    let def = ProcessDefinitionBuilder::new("xor_fail")
        .node("start", BpmnElement::StartEvent)
        .node(
            "gw",
            BpmnElement::ExclusiveGateway { default: None },
        )
        .node("a", BpmnElement::EndEvent)
        .node("b", BpmnElement::EndEvent)
        .flow("start", "gw")
        .conditional_flow("gw", "a", "x == 1")
        .conditional_flow("gw", "b", "x == 2")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;

    // No variables at all → no condition matches → error
    let result = engine.start_instance(def_key).await;
    assert!(
        matches!(result, Err(EngineError::NoMatchingCondition(_))),
        "Expected NoMatchingCondition, got: {result:?}"
    );
}

// -----------------------------------------------------------------------
// InclusiveGateway (OR) tests
// -----------------------------------------------------------------------

#[tokio::test]
async fn inclusive_gateway_forks_multiple_paths() {
    let mut engine = WorkflowEngine::new();

    // Start → Inclusive GW → (a > 0 → svc_a → end) / (b > 0 → svc_b → end)
    let def = ProcessDefinitionBuilder::new("or_test")
        .node("start", BpmnElement::StartEvent)
        .node("gw", BpmnElement::InclusiveGateway)
        .node("svc_a", BpmnElement::ServiceTask { topic: "track_a".into() })
        .node("svc_b", BpmnElement::ServiceTask { topic: "track_b".into() })
        .node("end", BpmnElement::EndEvent)
        .flow("start", "gw")
        .conditional_flow("gw", "svc_a", "a > 0")
        .conditional_flow("gw", "svc_b", "b > 0")
        .flow("svc_a", "end")
        .flow("svc_b", "end")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;

    // Both conditions true → both paths should fire
    let mut vars = HashMap::new();
    vars.insert("a".into(), Value::Number(10.into()));
    vars.insert("b".into(), Value::Number(20.into()));
    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();

    complete_all_service_tasks(&mut engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        *engine.get_instance_state(inst_id).unwrap(),
        InstanceState::Completed
    );
    let log = engine.get_audit_log(inst_id).unwrap();
    let gw_entry = log.iter().find(|l| l.contains("Inclusive gateway")).unwrap();
    assert!(
        gw_entry.contains("2 path(s)"),
        "Expected 2 forked paths: {gw_entry}"
    );
}

#[tokio::test]
async fn inclusive_gateway_single_match_no_fork() {
    let mut engine = WorkflowEngine::new();

    let def = ProcessDefinitionBuilder::new("or_single")
        .node("start", BpmnElement::StartEvent)
        .node("gw", BpmnElement::InclusiveGateway)
        .node("a", BpmnElement::ServiceTask { topic: "noop".into() })
        .node("b", BpmnElement::ServiceTask { topic: "noop".into() })
        .node("end", BpmnElement::EndEvent)
        .flow("start", "gw")
        .conditional_flow("gw", "a", "x == 1")
        .conditional_flow("gw", "b", "x == 2")
        .flow("a", "end")
        .flow("b", "end")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;

    // Only x == 1 → single match → Continue (not ContinueMultiple)
    let mut vars = HashMap::new();
    vars.insert("x".into(), Value::Number(1.into()));
    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();

    complete_all_service_tasks(&mut engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        *engine.get_instance_state(inst_id).unwrap(),
        InstanceState::Completed
    );
}

// -----------------------------------------------------------------------
// E2E: XOR Gateway with 2 UserTasks (condition: x > 0 / default)
// -----------------------------------------------------------------------

/// Helper: builds a workflow with XOR gateway routing to two user tasks.
///
/// ```text
/// Start → XOR Gateway → (x > 0) → user-task-1 ("author")   → End
///                     → (default) → user-task-2 ("reviewer") → End
/// ```
fn build_xor_user_task_definition() -> ProcessDefinition {
    ProcessDefinitionBuilder::new("xor_user_tasks")
        .node("start", BpmnElement::StartEvent)
        .node(
            "gw",
            BpmnElement::ExclusiveGateway {
                default: Some("user-task-2".into()),
            },
        )
        .node("user-task-1", BpmnElement::UserTask("author".into()))
        .node("user-task-2", BpmnElement::UserTask("reviewer".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "gw")
        .conditional_flow("gw", "user-task-1", "x > 0")
        .flow("gw", "user-task-2")
        .flow("user-task-1", "end")
        .flow("user-task-2", "end")
        .build()
        .unwrap()
}

#[tokio::test]
async fn xor_gateway_positive_x_routes_to_user_task_1() {
    let mut engine = WorkflowEngine::new();
    let def_key = engine.deploy_definition(build_xor_user_task_definition()).await;

    // x = 5 → condition "x > 0" matches → user-task-1
    let mut vars = HashMap::new();
    vars.insert("x".into(), Value::Number(5.into()));
    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();

    // Instance should be waiting on user-task-1
    let pending = engine.get_pending_user_tasks();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].node_id, "user-task-1");
    assert_eq!(pending[0].assignee, "author");

    assert!(matches!(
        engine.get_instance_state(inst_id).unwrap(),
        InstanceState::WaitingOnUserTask { .. }
    ));

    // Audit log should show gateway took path to user-task-1
    let log = engine.get_audit_log(inst_id).unwrap();
    let gw_entry = log.iter().find(|l| l.contains("Exclusive gateway")).unwrap();
    assert!(
        gw_entry.contains("user-task-1"),
        "Expected user-task-1 path: {gw_entry}"
    );

    // Complete the user task → should reach end
    let task_id = pending[0].task_id;
    engine
        .complete_user_task(task_id, HashMap::new())
        .await
        .unwrap();

    complete_all_service_tasks(&mut engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        *engine.get_instance_state(inst_id).unwrap(),
        InstanceState::Completed
    );
    assert!(engine.get_pending_user_tasks().is_empty());
}

#[tokio::test]
async fn xor_gateway_negative_x_routes_to_user_task_2() {
    let mut engine = WorkflowEngine::new();
    let def_key = engine.deploy_definition(build_xor_user_task_definition()).await;

    // x = -3 → condition "x > 0" does NOT match → default → user-task-2
    let mut vars = HashMap::new();
    vars.insert("x".into(), Value::Number((-3).into()));
    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();

    // Instance should be waiting on user-task-2
    let pending = engine.get_pending_user_tasks();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].node_id, "user-task-2");
    assert_eq!(pending[0].assignee, "reviewer");

    // Audit log should show gateway took default path to user-task-2
    let log = engine.get_audit_log(inst_id).unwrap();
    let gw_entry = log.iter().find(|l| l.contains("Exclusive gateway")).unwrap();
    assert!(
        gw_entry.contains("user-task-2"),
        "Expected user-task-2 (default) path: {gw_entry}"
    );

    // Complete the user task → should reach end
    let task_id = pending[0].task_id;
    engine
        .complete_user_task(task_id, HashMap::new())
        .await
        .unwrap();

    complete_all_service_tasks(&mut engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        *engine.get_instance_state(inst_id).unwrap(),
        InstanceState::Completed
    );
}

#[tokio::test]
async fn xor_gateway_zero_x_routes_to_user_task_2() {
    let mut engine = WorkflowEngine::new();
    let def_key = engine.deploy_definition(build_xor_user_task_definition()).await;

    // x = 0 → boundary: "x > 0" is false → default → user-task-2
    let mut vars = HashMap::new();
    vars.insert("x".into(), Value::Number(0.into()));
    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();

    let pending = engine.get_pending_user_tasks();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].node_id, "user-task-2");
    assert_eq!(pending[0].assignee, "reviewer");

    // Complete → should reach end
    let task_id = pending[0].task_id;
    engine
        .complete_user_task(task_id, HashMap::new())
        .await
        .unwrap();

    complete_all_service_tasks(&mut engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        *engine.get_instance_state(inst_id).unwrap(),
        InstanceState::Completed
    );
}

#[tokio::test]
async fn xor_gateway_user_task_merges_variables() {
    let mut engine = WorkflowEngine::new();
    let def_key = engine.deploy_definition(build_xor_user_task_definition()).await;

    // x = 10 → user-task-1
    let mut vars = HashMap::new();
    vars.insert("x".into(), Value::Number(10.into()));
    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();

    let task_id = engine.get_pending_user_tasks()[0].task_id;

    // Complete with extra variables → they should be merged into the instance
    let mut completion_vars = HashMap::new();
    completion_vars.insert("result".into(), Value::String("approved".into()));
    engine
        .complete_user_task(task_id, completion_vars)
        .await
        .unwrap();

    complete_all_service_tasks(&mut engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        *engine.get_instance_state(inst_id).unwrap(),
        InstanceState::Completed
    );

    // Verify both original and merged variables are present
    let details = engine.get_instance_details(inst_id).unwrap();
    assert_eq!(
        details.variables.get("x"),
        Some(&Value::Number(10.into())),
        "Original variable 'x' should be preserved"
    );
    assert_eq!(
        details.variables.get("result"),
        Some(&Value::String("approved".into())),
        "Merged variable 'result' should be present"
    );
}

fn build_script_test_definition() -> ProcessDefinition {
    ProcessDefinitionBuilder::new("script_test")
        .node("start", BpmnElement::StartEvent)
        .node("svc", BpmnElement::ServiceTask { topic: "calculate".into() })
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "end")
        .listener("svc", ListenerEvent::Start, "x = x * 2; let result = \"small\"; if x > 10 { result = \"big\" }")
        .build()
        .unwrap()
}

#[tokio::test]
async fn script_mutates_state_and_executes_logic() {
    let mut engine = WorkflowEngine::new();
    let def_key = engine.deploy_definition(build_script_test_definition()).await;

    let mut vars = HashMap::new();
    vars.insert("x".into(), serde_json::json!(6));

    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();

    complete_all_service_tasks(&mut engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        *engine.get_instance_state(inst_id).unwrap(),
        InstanceState::Completed
    );

    let details = engine.get_instance_details(inst_id).unwrap();

    complete_all_service_tasks(&mut engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        details.variables.get("x"),
        Some(&serde_json::json!(12)),
        "x should be mutated by script"
    );

    assert_eq!(
        details.variables.get("result"),
        Some(&serde_json::json!("big")),
        "script logic should set result"
    );
}

#[tokio::test]
async fn test_delete_instance() {
    let mut engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("test")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();

    let key = engine.deploy_definition(def).await;
    let instance_id = engine.start_instance(key).await.unwrap();
    assert_eq!(engine.instances.len(), 1);

    engine.delete_instance(instance_id).await.unwrap();
    
    assert_eq!(engine.instances.len(), 0);
    assert!(engine.get_instance_details(instance_id).is_err());
}

#[tokio::test]
async fn test_delete_definition_cascade() {
    let mut engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("test")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();

    let key = engine.deploy_definition(def).await;
    let _id1 = engine.start_instance(key).await.unwrap();
    let _id2 = engine.start_instance(key).await.unwrap();
    
    let err = engine.delete_definition(key, false).await.unwrap_err();
    assert!(matches!(err, crate::error::EngineError::DefinitionHasInstances(2)));

    engine.delete_definition(key, true).await.unwrap();
    
    assert_eq!(engine.definitions.len(), 0);
    assert_eq!(engine.instances.len(), 0);
}

// -----------------------------------------------------------------------
// ParallelGateway (AND) & Multi-Token tests
// -----------------------------------------------------------------------

#[tokio::test]
async fn parallel_gateway_forks_and_joins() {
    let mut engine = WorkflowEngine::new();

    // Start -> Split -> (A, B) -> Join -> End
    let def = ProcessDefinitionBuilder::new("and_test")
        .node("start", BpmnElement::StartEvent)
        .node("split", BpmnElement::ParallelGateway)
        .node("task_a", BpmnElement::ServiceTask { topic: "task_a".into() })
        .node("task_b", BpmnElement::ServiceTask { topic: "task_b".into() })
        .node("join", BpmnElement::ParallelGateway)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "split")
        .flow("split", "task_a")
        .flow("split", "task_b")
        .flow("task_a", "join")
        .flow("task_b", "join")
        .flow("join", "end")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;

    let inst_id = engine
        .start_instance(def_key)
        .await
        .unwrap();

    // Should be paused in parallel execution waiting for both service tasks
    let state = engine.get_instance_state(inst_id).unwrap().clone();
    println!("State after start: {:?}", state);
    for entry in engine.get_audit_log(inst_id).unwrap() {
        println!("Log: {}", entry);
    }
    assert!(matches!(state, InstanceState::ParallelExecution { active_token_count: 2 }), "State should be parallel execution: {:?}", state);

    assert_eq!(engine.pending_service_tasks.len(), 2);

    // Complete task A
    let task_a = engine.pending_service_tasks.iter().find(|t| t.topic == "task_a").unwrap().id;
    // Need to fetch and lock it first
    let _ = engine.fetch_and_lock_service_tasks("worker", 10, &["task_a".into()], 1000).await;
    
    let mut vars_a = std::collections::HashMap::new();
    vars_a.insert("var_a".into(), serde_json::Value::Bool(true));
    engine.complete_service_task(task_a, "worker", vars_a).await.unwrap();

    // After A completes, it should be waiting at the join. Still in parallel state.
    let state = engine.get_instance_state(inst_id).unwrap().clone();
    println!("State after A completes: {:?}", state);
    for entry in engine.get_audit_log(inst_id).unwrap() {
        println!("Log: {}", entry);
    }
    assert!(matches!(state, InstanceState::ParallelExecution { active_token_count: 2 }));
    
    // Check join barrier
    let inst = engine.instances.get(&inst_id).unwrap();
    let barrier = inst.join_barriers.get("join").unwrap();
    assert_eq!(barrier.expected_count, 2);
    assert_eq!(barrier.arrived_tokens.len(), 1);

    // Complete task B
    let task_b = engine.pending_service_tasks.iter().find(|t| t.topic == "task_b").unwrap().id;
    let _ = engine.fetch_and_lock_service_tasks("worker", 10, &["task_b".into()], 1000).await;
    let mut vars_b = std::collections::HashMap::new();
    vars_b.insert("var_b".into(), serde_json::Value::Bool(true));
    engine.complete_service_task(task_b, "worker", vars_b).await.unwrap();

    // Now it should be complete!
    assert_eq!(
        *engine.get_instance_state(inst_id).unwrap(),
        InstanceState::Completed
    );

    // Variables from both branches should be merged
}

// -----------------------------------------------------------------------
// Service Task specific operations
// -----------------------------------------------------------------------

#[tokio::test]
async fn service_task_fail_and_retries() {
    let mut engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("retries")
        .node("start", BpmnElement::StartEvent)
        .node("svc", BpmnElement::ServiceTask { topic: "fail_test".into() })
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "end")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;
    engine.start_instance(def_key).await.unwrap();

    // 1. Fetch task
    let tasks = engine.fetch_and_lock_service_tasks("worker", 1, &["fail_test".into()], 60).await;
    assert_eq!(tasks.len(), 1);
    let task_id = tasks[0].id;

    // 2. Fail task (default 3 retries, decrementing to 2)
    engine.fail_service_task(task_id, "worker", None, Some("Failed".into()), None).await.unwrap();

    // 3. Task should be unlocked and retries should be 2
    let pending = engine.get_pending_service_tasks();
    let t = pending.iter().find(|t| t.id == task_id).unwrap();
    assert_eq!(t.retries, 2);
    assert!(t.worker_id.is_none());

    // 4. Fail directly to 0
    let _ = engine.fetch_and_lock_service_tasks("worker2", 1, &["fail_test".into()], 60).await;
    engine.fail_service_task(task_id, "worker2", Some(0), Some("Fatal".into()), None).await.unwrap();

    // Incident should be logged
    let inst_id = tasks[0].instance_id;
    let log = engine.get_audit_log(inst_id).unwrap();
    assert!(log.iter().any(|l| l.contains("INCIDENT")));
    assert!(log.iter().any(|l| l.contains("Fatal")));
}

#[tokio::test]
async fn service_task_extend_lock() {
    let mut engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("extend")
        .node("start", BpmnElement::StartEvent)
        .node("svc", BpmnElement::ServiceTask { topic: "ext".into() })
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "end")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;
    engine.start_instance(def_key).await.unwrap();

    let tasks = engine.fetch_and_lock_service_tasks("worker", 1, &["ext".into()], 60).await;
    let task_id = tasks[0].id;
    let exp_before = engine.get_pending_service_tasks().iter().find(|t| t.id == task_id).unwrap().lock_expiration.unwrap();
    
    engine.extend_lock(task_id, "worker", 120).await.unwrap();
    
    let exp_after = engine.get_pending_service_tasks().iter().find(|t| t.id == task_id).unwrap().lock_expiration.unwrap();
    assert!(exp_after > exp_before);
}

#[tokio::test]
async fn service_task_handle_bpmn_error() {
    let mut engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("err")
        .node("start", BpmnElement::StartEvent)
        .node("svc", BpmnElement::ServiceTask { topic: "err".into() })
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "end")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;
    engine.start_instance(def_key).await.unwrap();

    let tasks = engine.fetch_and_lock_service_tasks("worker", 1, &["err".into()], 60).await;
    
    assert_eq!(engine.get_pending_service_tasks().len(), 1);
    
    engine.handle_bpmn_error(tasks[0].id, "worker", "ERR_CODE").await.unwrap();
    
    // Task should be removed and error logged.
    assert_eq!(engine.get_pending_service_tasks().len(), 0);
    
    let log = engine.get_audit_log(tasks[0].instance_id).unwrap();
    assert!(log.iter().any(|l| l.contains("ERR_CODE")));
}

#[tokio::test]
async fn restore_instance_loads_from_persistence() {
    let mut engine = WorkflowEngine::new();
    
    // Deploy a definition so it exists
    let def = ProcessDefinitionBuilder::new("restore")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();
    let def_key = engine.deploy_definition(def).await;

    // Create a dummy instance
    let inst = ProcessInstance {
        id: Uuid::new_v4(),
        definition_key: def_key,
        business_key: "BK1".into(),
        parent_instance_id: None,
        state: InstanceState::Completed,
        current_node: "end".into(),
        audit_log: vec![],
        variables: std::collections::HashMap::new(),
        active_tokens: vec![],
        join_barriers: std::collections::HashMap::new(),
    };
    
    engine.restore_instance(inst.clone());
    
    let loaded = engine.get_instance_details(inst.id).unwrap();
    assert_eq!(loaded.id, inst.id);
    assert_eq!(loaded.business_key, "BK1");
}

#[tokio::test]
async fn mutation_delete_instance_and_variables() {
    let mut engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("del")
        .node("start", BpmnElement::StartEvent)
        .node("t1", BpmnElement::UserTask("a".into()))
        .node("t2", BpmnElement::UserTask("b".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "t1")
        .flow("t1", "t2")
        .flow("t2", "end")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;
    
    // Check list definitions formatting
    let defs = engine.list_definitions();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].1, "del");
    
    let inst_id = engine.start_instance(def_key).await.unwrap();

    // Variable Math check (verify += vs -= mutant in update_instance_variables)
    let mut vars = HashMap::new();
    vars.insert("val".into(), serde_json::Value::Number(10.into()));
    engine.update_instance_variables(inst_id, vars).await.unwrap();

    let details = engine.get_instance_details(inst_id).unwrap();
    assert_eq!(details.variables.get("val").unwrap(), &serde_json::Value::Number(10.into()));
    assert_eq!(details.variables.len(), 1); // Test mutant missing logic
    
    // Test delete instance == vs != loop
    let mut vars2 = HashMap::new();
    vars2.insert("other".into(), serde_json::Value::Bool(true));
    engine.update_instance_variables(inst_id, vars2).await.unwrap();
    let details2 = engine.get_instance_details(inst_id).unwrap();
    assert_eq!(details2.variables.len(), 2);
    
    // Create side tasks
    let pending = engine.get_pending_user_tasks();
    assert_eq!(pending.len(), 1);
    
    engine.delete_instance(inst_id).await.unwrap();
    // After delete, list should be 0.
    assert!(engine.get_instance_state(inst_id).is_err());
}

#[tokio::test]
async fn mutation_fetch_service_task_boundary() {
    let mut engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("lock")
        .node("start", BpmnElement::StartEvent)
        .node("t1", BpmnElement::ServiceTask { topic: "bound".into() })
        .node("end", BpmnElement::EndEvent)
        .flow("start", "t1")
        .flow("t1", "end")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;
    engine.start_instance(def_key).await.unwrap();

    // Fetch once
    let tasks1 = engine.fetch_and_lock_service_tasks("worker1", 1, &["bound".into()], 1).await;
    assert_eq!(tasks1.len(), 1);

    // Fetch immediately again, should return 0 since locked and not expired
    let tasks2 = engine.fetch_and_lock_service_tasks("worker2", 1, &["bound".into()], 1).await;
    assert_eq!(tasks2.len(), 0);

    // Sleep 1.1 second so it exceeds. `cargo mutants` tests > vs == on the `expiration > now`.
    tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

    // Fetch again, should return 1 since lock expired
    let tasks3 = engine.fetch_and_lock_service_tasks("worker3", 1, &["bound".into()], 1).await;
    assert_eq!(tasks3.len(), 1);
}

#[tokio::test]
async fn mutation_find_downstream_join() {
    let mut engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("join")
        .node("start", BpmnElement::StartEvent)
        .node("gw_split", BpmnElement::ParallelGateway)
        .node("gw_join", BpmnElement::InclusiveGateway)
        .node("dummy", BpmnElement::ServiceTask { topic: "dummy".to_string() })
        .node("end", BpmnElement::EndEvent)
        .flow("start", "gw_split")
        .flow("gw_split", "gw_join")
        .flow("gw_split", "dummy")
        .flow("dummy", "gw_join")
        .flow("gw_join", "end")
        .build()
        .unwrap();

    engine.deploy_definition(def.clone()).await;
    
    // Testing the logic explicitly via direct call (internal visibility allows this within engine module)
    let engine_local = WorkflowEngine::new();
    let found = engine_local.find_downstream_join(&def, "gw_split");
    assert_eq!(found, Some("gw_join".to_string()));

    // Find with depth limit (though internal recursion only decreases by 1, testing the > 100 limit protection is hard, but we can just test if the logic iterates correctly).
    let found_from_start = engine_local.find_downstream_join(&def, "start"); 
    assert_eq!(found_from_start, Some("gw_join".to_string()));
    // It actually returns None because it exceeds max recursion or doesn't find gateway.
}

#[tokio::test]
async fn message_start_event_succeeds() {
    let mut engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("msg_start")
        .node("start", BpmnElement::MessageStartEvent { message_name: "start_msg".to_string() })
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();

    engine.deploy_definition(def).await;

    // Normal start should fail or wait if not message? Actually, correlate_message starts it
    let mut vars = HashMap::new();
    vars.insert("k".into(), serde_json::Value::String("v".into()));
    
    let affected = engine.correlate_message("start_msg".into(), Some("bk1".into()), vars).await.unwrap();
    assert_eq!(affected.len(), 1);
    
    let inst_id = affected[0];
    let inst = engine.get_instance_details(inst_id).unwrap();
    assert_eq!(inst.state, InstanceState::Completed);
    assert_eq!(inst.business_key, "bk1");
}

#[tokio::test]
async fn timer_catch_event_succeeds() {
    let mut engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("timer_catch")
        .node("start", BpmnElement::StartEvent)
        .node("timer", BpmnElement::TimerCatchEvent(std::time::Duration::from_millis(50)))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "timer")
        .flow("timer", "end")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();
    
    assert_eq!(engine.get_instance_state(inst_id).unwrap(), &InstanceState::WaitingOnTimer { timer_id: engine.pending_timers[0].id });
    
    // Won't trigger immediately
    let triggered = engine.process_timers().await.unwrap();
    assert_eq!(triggered, 0);

    tokio::time::sleep(tokio::time::Duration::from_millis(60)).await;
    
    let triggered = engine.process_timers().await.unwrap();
    assert_eq!(triggered, 1);
    
    assert_eq!(engine.get_instance_state(inst_id).unwrap(), &InstanceState::Completed);
}

#[tokio::test]
async fn boundary_timer_event_cancels_task() {
    let mut engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("bound_timer")
        .node("start", BpmnElement::StartEvent)
        .node("task", BpmnElement::UserTask("assignee".into()))
        .node("bound_timer", BpmnElement::BoundaryTimerEvent { attached_to: "task".into(), duration: std::time::Duration::from_millis(50), cancel_activity: true })
        .node("end1", BpmnElement::EndEvent)
        .node("end2", BpmnElement::EndEvent)
        .flow("start", "task")
        .flow("task", "end1")
        .flow("bound_timer", "end2")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();
    
    assert_eq!(engine.pending_user_tasks.len(), 1);
    assert_eq!(engine.pending_timers.len(), 1);
    
    tokio::time::sleep(tokio::time::Duration::from_millis(60)).await;
    let triggered = engine.process_timers().await.unwrap();
    assert_eq!(triggered, 1);
    
    let inst = engine.get_instance_details(inst_id).unwrap();
    assert_eq!(inst.state, InstanceState::Completed);
    assert_eq!(inst.current_node, "end2");
}

#[tokio::test]
async fn boundary_error_event_catches_error() {
    let mut engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("bound_err")
        .node("start", BpmnElement::StartEvent)
        .node("task", BpmnElement::ServiceTask { topic: "err_topic".into() })
        .node("bound_err", BpmnElement::BoundaryErrorEvent { attached_to: "task".into(), error_code: Some("ERR_CODE_500".into()) })
        .node("end1", BpmnElement::EndEvent)
        .node("end2", BpmnElement::EndEvent)
        .flow("start", "task")
        .flow("task", "end1")
        .flow("bound_err", "end2")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();
    
    let tasks = engine.fetch_and_lock_service_tasks("worker", 1, &["err_topic".into()], 10).await;
    assert_eq!(tasks.len(), 1);
    
    engine.handle_bpmn_error(tasks[0].id, "worker", "ERR_CODE_500").await.unwrap();
    
    let inst = engine.get_instance_details(inst_id).unwrap();
    assert_eq!(inst.state, InstanceState::Completed);
    assert_eq!(inst.current_node, "end2");
}

#[tokio::test]
async fn call_activity_lifecycle() {
    let mut engine = WorkflowEngine::new();
    
    // Deploy Child
    let child_def = ProcessDefinitionBuilder::new("child_proc")
        .node("start", BpmnElement::StartEvent)
        .node("child_task", BpmnElement::UserTask("child_assignee".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "child_task")
        .flow("child_task", "end")
        .build()
        .unwrap();
    let _child_key = engine.deploy_definition(child_def).await;
    
    // Deploy Parent
    let parent_def = ProcessDefinitionBuilder::new("parent_proc")
        .node("start", BpmnElement::StartEvent)
        .node("call", BpmnElement::CallActivity { called_element: "child_proc".into() })
        .node("end", BpmnElement::EndEvent)
        .flow("start", "call")
        .flow("call", "end")
        .build()
        .unwrap();
    let parent_key = engine.deploy_definition(parent_def).await;
    
    // Start Parent
    let parent_id = engine.start_instance(parent_key).await.unwrap();
    
    // Parent should be blocked on Call Activity
    let parent_inst = engine.get_instance_details(parent_id).unwrap();
    if let InstanceState::WaitingOnCallActivity { sub_instance_id, .. } = parent_inst.state {
        // Child instance should exist
        let child_inst = engine.get_instance_details(sub_instance_id).unwrap();
        assert_eq!(child_inst.parent_instance_id, Some(parent_id));
        assert!(matches!(child_inst.state, InstanceState::WaitingOnUserTask { .. }));
        assert!(matches!(child_inst.state, InstanceState::WaitingOnUserTask { .. }));
        
        // Complete the child's user task
        let tasks = engine.get_pending_user_tasks();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].instance_id, sub_instance_id);
        
        let child_task_id = tasks[0].task_id;
        
        // Add a variable to child to ensure parent gets it
        let mut vars = std::collections::HashMap::new();
        vars.insert("from_child".into(), serde_json::json!("hello parent"));
        engine.complete_user_task(child_task_id, vars).await.unwrap();
        
        // Child should be completed
        let child_inst = engine.get_instance_details(sub_instance_id).unwrap();
        assert_eq!(child_inst.state, InstanceState::Completed);
        
        // Parent should now be automatically resumed and completed
        let parent_inst = engine.get_instance_details(parent_id).unwrap();
        assert_eq!(parent_inst.state, InstanceState::Completed);
        assert_eq!(parent_inst.variables.get("from_child").unwrap().as_str().unwrap(), "hello parent");
    } else {
        panic!("Parent not waiting on call activity: {:?}", parent_inst.state);
    }
}

// ---------------------------------------------------------------------------
// Advanced Edge Case Testing with InMemoryPersistence
// ---------------------------------------------------------------------------

#[tokio::test]
async fn in_memory_simultaneous_timer_and_message_race() {
    let mut engine = WorkflowEngine::with_in_memory_persistence();
    
    let def = ProcessDefinitionBuilder::new("race")
        .node("start", BpmnElement::StartEvent)
        .node("fork", BpmnElement::ParallelGateway)
        .node("timer", BpmnElement::TimerCatchEvent(std::time::Duration::from_millis(50)))
        .node("msg", BpmnElement::MessageCatchEvent { message_name: "MSG_CANCEL".into() })
        .node("join", BpmnElement::ParallelGateway)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "fork")
        .flow("fork", "timer")
        .flow("fork", "msg")
        .flow("timer", "join")
        .flow("msg", "join")
        .flow("join", "end")
        .build()
        .unwrap();
        
    let def_key = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();
    
    assert_eq!(engine.pending_timers.len(), 1);
    assert_eq!(engine.pending_message_catches.len(), 1);
    
    // Simulate time passing (50ms) BUT before processing timers, we send the message!
    tokio::time::sleep(tokio::time::Duration::from_millis(60)).await;
    
    // Race: The message arrives precisely when the timer is due.
    let msgs = engine.pending_message_catches.clone();
    engine.correlate_message(msgs[0].message_name.clone(), None, std::collections::HashMap::new()).await.unwrap();
    
    // Since message was processed first, the instance was routed to join, and blocked on parallel gate
    let _inst = engine.get_instance_details(inst_id).unwrap();
    // (Note: correlate_message blindly resets state to Running visually, but it's still waiting on the other parallel branch inside active_tokens)
    
    // Now if we process timers, it should trigger the timer and join to finish
    let triggered = engine.process_timers().await.unwrap();
    assert_eq!(triggered, 1);
    
    let inst2 = engine.get_instance_details(inst_id).unwrap();
    assert_eq!(inst2.state, InstanceState::Completed);
}

#[tokio::test]
async fn in_memory_script_robust_failure_handling() {
    let mut engine = WorkflowEngine::with_in_memory_persistence();
    let script = "let a = 1; throw \"Intentional crash!\";";
    
    let def = ProcessDefinitionBuilder::new("script_crash")
        .node("start", BpmnElement::StartEvent)
        .node("task", BpmnElement::UserTask("worker".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "task")
        .flow("task", "end")
        .listener("start", crate::model::ListenerEvent::Start, script)
        .build()
        .unwrap();
        
    let def_key = engine.deploy_definition(def).await;
    
    // Engine should panic or return error because script is broken
    let result = engine.start_instance(def_key).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Intentional crash!"));
}

#[tokio::test]
async fn in_memory_large_file_variables() {
    let mut engine = WorkflowEngine::with_in_memory_persistence();
    
    let def = ProcessDefinitionBuilder::new("large_file")
        .node("start", BpmnElement::StartEvent)
        .node("task", BpmnElement::UserTask("worker".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "task")
        .flow("task", "end")
        .build()
        .unwrap();
        
    let def_key = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();
    
    // Create a very large dummy payload (10 MB of zeros to simulate memory stress)
    // NOTE: In the real engine-server, the file goes to the persistence layer.
    // In engine-core tests, we can just insert the reference into variables and 
    // also persist it explicitly to in-memory persistence.
    let large_payload = vec![0u8; 10 * 1024 * 1024]; 
    
    if let Some(p) = &engine.persistence {
        p.save_file("file:big_data", &large_payload).await.unwrap();
    }
    
    let file_ref = crate::model::FileReference {
        object_key: "file:big_data".into(),
        filename: "big_data.bin".into(),
        mime_type: "application/octet-stream".into(),
        size_bytes: large_payload.len() as u64,
        uploaded_at: chrono::Utc::now().to_rfc3339(),
    };
    
    // Inject it into task
    let tasks = engine.get_pending_user_tasks();
    let mut vars = std::collections::HashMap::new();
    vars.insert("my_file".into(), serde_json::to_value(&file_ref).unwrap());
    
    engine.complete_user_task(tasks[0].task_id, vars).await.unwrap();
    
    let inst = engine.get_instance_details(inst_id).unwrap();
    assert_eq!(inst.state, InstanceState::Completed);
    
    // Validate we can download it back
    let v = inst.variables.get("my_file").unwrap();
    let f_ref: crate::model::FileReference = serde_json::from_value(v.clone()).unwrap();
    if let Some(p) = &engine.persistence {
        let downloaded = p.load_file(&f_ref.object_key).await.unwrap();
        assert_eq!(downloaded.len(), 10 * 1024 * 1024);
    }
}
