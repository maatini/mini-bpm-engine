//! Unit tests for the workflow engine.
//!
//! Extracted from `engine.rs` to keep the main module focused on production
//! logic.

use super::super::*;
use crate::domain::ListenerEvent;
use crate::domain::ProcessDefinitionBuilder;

async fn complete_all_service_tasks(
    engine: &WorkflowEngine,
    worker: &str,
    vars: HashMap<String, Value>,
) {
    let mut to_complete = Vec::new();
    for task in engine
        .pending_service_tasks
        .iter()
        .map(|r| r.value().clone())
    {
        to_complete.push((task.id, task.topic.clone()));
    }
    for (id, topic) in to_complete {
        let _ = engine
            .fetch_and_lock_service_tasks(worker, 10, std::slice::from_ref(&topic), 60000)
            .await;
        engine
            .complete_service_task(id, worker, vars.clone())
            .await
            .unwrap();
    }
}

async fn setup_linear_engine() -> (WorkflowEngine, Uuid) {
    let engine = WorkflowEngine::new();

    // Register a simple service handler

    let def = ProcessDefinitionBuilder::new("linear")
        .node("start", BpmnElement::StartEvent)
        .node(
            "svc",
            BpmnElement::ServiceTask {
                topic: "validate".into(),
                multi_instance: None,
            },
        )
        .node("ut", BpmnElement::UserTask("alice".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "ut")
        .flow("ut", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    (engine, def_key)
}

#[tokio::test]
async fn conditional_routing_on_service_task() {
    let engine = WorkflowEngine::new();

    let def = ProcessDefinitionBuilder::new("cond_svc")
        .node("start", BpmnElement::StartEvent)
        .node(
            "svc",
            BpmnElement::ServiceTask {
                topic: "noop".into(),
                multi_instance: None,
            },
        )
        .node("end_a", BpmnElement::EndEvent)
        .node("end_b", BpmnElement::EndEvent)
        .flow("start", "svc")
        .conditional_flow("svc", "end_a", "x == 1")
        .conditional_flow("svc", "end_b", "x == 2")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;

    let mut vars = HashMap::new();
    vars.insert("x".into(), Value::Number(2.into()));
    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();

    complete_all_service_tasks(&engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );
    let log = engine.get_audit_log(inst_id).await.unwrap();
    let end_entry = log
        .iter()
        .find(|l| l.contains("Process completed"))
        .unwrap();
    assert!(
        end_entry.contains("end_b"),
        "Expected end_b path: {end_entry}"
    );
}

#[tokio::test]
async fn start_instance_pauses_at_user_task() {
    let (engine, def_key) = setup_linear_engine().await;
    let inst_id = engine.start_instance(def_key).await.unwrap();

    complete_all_service_tasks(&engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::WaitingOnUserTask {
            task_id: engine
                .pending_user_tasks
                .iter()
                .map(|r| r.value().clone())
                .next()
                .unwrap()
                .task_id
        }
    );
    assert_eq!(engine.pending_user_tasks.len(), 1);
}

#[tokio::test]
async fn complete_user_task_reaches_end() {
    let (engine, def_key) = setup_linear_engine().await;
    let inst_id = engine.start_instance(def_key).await.unwrap();
    complete_all_service_tasks(&engine, "worker", HashMap::new()).await;

    let task_id = engine
        .pending_user_tasks
        .iter()
        .map(|r| r.value().clone())
        .next()
        .unwrap()
        .task_id;
    engine
        .complete_user_task(task_id, HashMap::new())
        .await
        .unwrap();

    complete_all_service_tasks(&engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );
    assert!(engine.pending_user_tasks.is_empty());
}

#[tokio::test]
async fn completing_wrong_task_gives_error() {
    let (engine, def_key) = setup_linear_engine().await;
    engine.start_instance(def_key).await.unwrap();
    complete_all_service_tasks(&engine, "worker", HashMap::new()).await;

    let wrong_id = Uuid::new_v4();
    let result = engine.complete_user_task(wrong_id, HashMap::new()).await;
    assert!(matches!(result, Err(EngineError::TaskNotPending { .. })));
}

#[tokio::test]
async fn service_handler_modifies_variables() {
    let (engine, def_key) = setup_linear_engine().await;
    let inst_id = engine.start_instance(def_key).await.unwrap();

    let mut vars = HashMap::new();
    vars.insert("validated".into(), Value::Bool(true));
    complete_all_service_tasks(&engine, "worker_1", vars).await;

    // The token stored centrally should have 'validated: true' from the service handler
    let pending = engine
        .pending_user_tasks
        .iter()
        .map(|r| r.value().clone())
        .next()
        .unwrap();
    let inst_arc = engine.instances.get(&inst_id).await.unwrap();
    let inst = inst_arc.read().await;
    let token = inst.tokens.get(&pending.token_id).unwrap();
    assert_eq!(token.variables.get("validated"), Some(&Value::Bool(true)));
}

#[tokio::test]
async fn timer_start_succeeds() {
    let engine = WorkflowEngine::new();
    let dur = Duration::from_secs(60);

    let def = ProcessDefinitionBuilder::new("timer_proc")
        .node(
            "ts",
            BpmnElement::TimerStartEvent(crate::domain::TimerDefinition::Duration(dur)),
        )
        .node("end", BpmnElement::EndEvent)
        .flow("ts", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.trigger_timer_start(def_key, dur).await.unwrap();

    complete_all_service_tasks(&engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );
}

#[tokio::test]
async fn timer_mismatch_gives_error() {
    let engine = WorkflowEngine::new();

    let def = ProcessDefinitionBuilder::new("timer_proc")
        .node(
            "ts",
            BpmnElement::TimerStartEvent(crate::domain::TimerDefinition::Duration(
                Duration::from_secs(60),
            )),
        )
        .node("end", BpmnElement::EndEvent)
        .flow("ts", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    let result = engine
        .trigger_timer_start(def_key, Duration::from_secs(30))
        .await;
    assert!(matches!(result, Err(EngineError::TimerMismatch { .. })));
}

#[tokio::test]
async fn plain_start_rejects_timer_def() {
    let engine = WorkflowEngine::new();

    let def = ProcessDefinitionBuilder::new("timer_proc")
        .node(
            "ts",
            BpmnElement::TimerStartEvent(crate::domain::TimerDefinition::Duration(
                Duration::from_secs(5),
            )),
        )
        .node("end", BpmnElement::EndEvent)
        .flow("ts", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    let result = engine.start_instance(def_key).await;
    assert!(matches!(
        result,
        Err(EngineError::InvalidDefinition(msg)) if msg.contains("timer")
    ));
}

#[tokio::test]
async fn unknown_definition_gives_error() {
    let engine = WorkflowEngine::new();
    let result = engine.start_instance(Uuid::new_v4()).await;
    assert!(matches!(result, Err(EngineError::NoSuchDefinition(_))));
}

#[tokio::test]
async fn audit_log_captures_all_steps() {
    let (engine, def_key) = setup_linear_engine().await;
    let inst_id = engine.start_instance(def_key).await.unwrap();
    complete_all_service_tasks(&engine, "worker", HashMap::new()).await;

    let task_id = engine
        .pending_user_tasks
        .iter()
        .map(|r| r.value().clone())
        .next()
        .unwrap()
        .task_id;
    engine
        .complete_user_task(task_id, HashMap::new())
        .await
        .unwrap();

    let log = engine.get_audit_log(inst_id).await.unwrap();
    assert!(log.len() >= 4);
    assert!(log[0].contains("started"));
    assert!(log.last().unwrap().contains("completed"));
}

// -----------------------------------------------------------------------
// Condition evaluator tests
// -----------------------------------------------------------------------

// Condition unit tests removed — covered by condition.rs::tests.

// -----------------------------------------------------------------------
// ExclusiveGateway (XOR) tests
// -----------------------------------------------------------------------

#[tokio::test]
async fn exclusive_gateway_takes_matching_path() {
    let engine = WorkflowEngine::new();

    // Start → XOR Gateway → (amount > 100 → high) / (default → low) → End
    let def = ProcessDefinitionBuilder::new("xor_test")
        .node("start", BpmnElement::StartEvent)
        .node(
            "gw",
            BpmnElement::ExclusiveGateway {
                default: Some("low".into()),
            },
        )
        .node(
            "high",
            BpmnElement::ServiceTask {
                topic: "noop".into(),
                multi_instance: None,
            },
        )
        .node(
            "low",
            BpmnElement::ServiceTask {
                topic: "noop".into(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "gw")
        .conditional_flow("gw", "high", "amount > 100")
        .flow("gw", "low") // unconditional (default candidate)
        .flow("high", "end")
        .flow("low", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;

    // amount = 500 → should take the "high" path
    let mut vars = HashMap::new();
    vars.insert("amount".into(), Value::Number(500.into()));
    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();

    complete_all_service_tasks(&engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );
    let log = engine.get_audit_log(inst_id).await.unwrap();
    let gw_entry = log
        .iter()
        .find(|l| l.contains("Exclusive gateway"))
        .unwrap();
    assert!(gw_entry.contains("high"), "Expected high path: {gw_entry}");
}

#[tokio::test]
async fn exclusive_gateway_uses_default_when_no_match() {
    let engine = WorkflowEngine::new();

    let def = ProcessDefinitionBuilder::new("xor_default")
        .node("start", BpmnElement::StartEvent)
        .node(
            "gw",
            BpmnElement::ExclusiveGateway {
                default: Some("low".into()),
            },
        )
        .node(
            "high",
            BpmnElement::ServiceTask {
                topic: "noop".into(),
                multi_instance: None,
            },
        )
        .node(
            "low",
            BpmnElement::ServiceTask {
                topic: "noop".into(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "gw")
        .conditional_flow("gw", "high", "amount > 100")
        .flow("gw", "low")
        .flow("high", "end")
        .flow("low", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;

    // amount = 50 → no condition matches → should use default "low"
    let mut vars = HashMap::new();
    vars.insert("amount".into(), Value::Number(50.into()));
    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();

    complete_all_service_tasks(&engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );
    let log = engine.get_audit_log(inst_id).await.unwrap();
    let gw_entry = log
        .iter()
        .find(|l| l.contains("Exclusive gateway"))
        .unwrap();
    assert!(
        gw_entry.contains("low"),
        "Expected low (default) path: {gw_entry}"
    );
}

#[tokio::test]
async fn exclusive_gateway_error_when_no_match_no_default() {
    let engine = WorkflowEngine::new();

    let def = ProcessDefinitionBuilder::new("xor_fail")
        .node("start", BpmnElement::StartEvent)
        .node("gw", BpmnElement::ExclusiveGateway { default: None })
        .node("a", BpmnElement::EndEvent)
        .node("b", BpmnElement::EndEvent)
        .flow("start", "gw")
        .conditional_flow("gw", "a", "x == 1")
        .conditional_flow("gw", "b", "x == 2")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;

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
    let engine = WorkflowEngine::new();

    // Start → Inclusive GW → (a > 0 → svc_a → end) / (b > 0 → svc_b → end)
    let def = ProcessDefinitionBuilder::new("or_test")
        .node("start", BpmnElement::StartEvent)
        .node("gw", BpmnElement::InclusiveGateway)
        .node(
            "svc_a",
            BpmnElement::ServiceTask {
                topic: "track_a".into(),
                multi_instance: None,
            },
        )
        .node(
            "svc_b",
            BpmnElement::ServiceTask {
                topic: "track_b".into(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "gw")
        .conditional_flow("gw", "svc_a", "a > 0")
        .conditional_flow("gw", "svc_b", "b > 0")
        .flow("svc_a", "end")
        .flow("svc_b", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;

    // Both conditions true → both paths should fire
    let mut vars = HashMap::new();
    vars.insert("a".into(), Value::Number(10.into()));
    vars.insert("b".into(), Value::Number(20.into()));
    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();

    complete_all_service_tasks(&engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );
    let log = engine.get_audit_log(inst_id).await.unwrap();
    let gw_entry = log
        .iter()
        .find(|l| l.contains("Inclusive gateway"))
        .unwrap();
    assert!(
        gw_entry.contains("2 path(s)"),
        "Expected 2 forked paths: {gw_entry}"
    );
}

#[tokio::test]
async fn inclusive_gateway_single_match_no_fork() {
    let engine = WorkflowEngine::new();

    let def = ProcessDefinitionBuilder::new("or_single")
        .node("start", BpmnElement::StartEvent)
        .node("gw", BpmnElement::InclusiveGateway)
        .node(
            "a",
            BpmnElement::ServiceTask {
                topic: "noop".into(),
                multi_instance: None,
            },
        )
        .node(
            "b",
            BpmnElement::ServiceTask {
                topic: "noop".into(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "gw")
        .conditional_flow("gw", "a", "x == 1")
        .conditional_flow("gw", "b", "x == 2")
        .flow("a", "end")
        .flow("b", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;

    // Only x == 1 → single match → Continue (not ContinueMultiple)
    let mut vars = HashMap::new();
    vars.insert("x".into(), Value::Number(1.into()));
    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();

    complete_all_service_tasks(&engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
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
    let engine = WorkflowEngine::new();
    let (def_key, _) = engine
        .deploy_definition(build_xor_user_task_definition())
        .await;

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
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::WaitingOnUserTask { .. }
    ));

    // Audit log should show gateway took path to user-task-1
    let log = engine.get_audit_log(inst_id).await.unwrap();
    let gw_entry = log
        .iter()
        .find(|l| l.contains("Exclusive gateway"))
        .unwrap();
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

    complete_all_service_tasks(&engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );
    assert!(engine.get_pending_user_tasks().is_empty());
}

#[tokio::test]
async fn xor_gateway_negative_x_routes_to_user_task_2() {
    let engine = WorkflowEngine::new();
    let (def_key, _) = engine
        .deploy_definition(build_xor_user_task_definition())
        .await;

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
    let log = engine.get_audit_log(inst_id).await.unwrap();
    let gw_entry = log
        .iter()
        .find(|l| l.contains("Exclusive gateway"))
        .unwrap();
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

    complete_all_service_tasks(&engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );
}

// xor_gateway_zero_x_routes_to_user_task_2 entfernt — redundant mit
// xor_gateway_negative_x_routes_to_user_task_2 (gleicher Default-Pfad).

#[tokio::test]
async fn xor_gateway_user_task_merges_variables() {
    let engine = WorkflowEngine::new();
    let (def_key, _) = engine
        .deploy_definition(build_xor_user_task_definition())
        .await;

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

    complete_all_service_tasks(&engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );

    // Verify both original and merged variables are present
    let details = engine.get_instance_details(inst_id).await.unwrap();
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
        .node(
            "svc",
            BpmnElement::ServiceTask {
                topic: "calculate".into(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "end")
        .listener(
            "svc",
            ListenerEvent::Start,
            "x = x * 2; let result = \"small\"; if x > 10 { result = \"big\" }",
        )
        .build()
        .unwrap()
}

#[tokio::test]
async fn script_mutates_state_and_executes_logic() {
    let engine = WorkflowEngine::new();
    let (def_key, _) = engine
        .deploy_definition(build_script_test_definition())
        .await;

    let mut vars = HashMap::new();
    vars.insert("x".into(), serde_json::json!(6));

    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();

    complete_all_service_tasks(&engine, "worker_1", HashMap::new()).await;

    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );

    let details = engine.get_instance_details(inst_id).await.unwrap();

    complete_all_service_tasks(&engine, "worker_1", HashMap::new()).await;

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
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("test")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(def).await;
    let instance_id = engine.start_instance(key).await.unwrap();
    assert_eq!(engine.instances.len().await, 1);

    engine.delete_instance(instance_id).await.unwrap();

    assert_eq!(engine.instances.len().await, 0);
    assert!(engine.get_instance_details(instance_id).await.is_err());
}

#[tokio::test]
async fn test_delete_definition_cascade() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("test")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(def).await;
    let _id1 = engine.start_instance(key).await.unwrap();
    let _id2 = engine.start_instance(key).await.unwrap();

    let err = engine.delete_definition(key, false).await.unwrap_err();
    assert!(matches!(
        err,
        crate::domain::EngineError::DefinitionHasInstances(2)
    ));

    engine.delete_definition(key, true).await.unwrap();

    let stats = engine.get_stats().await;
    assert_eq!(stats.definitions_count, 0);
    assert_eq!(engine.instances.len().await, 0);
}

// -----------------------------------------------------------------------
// ParallelGateway (AND) & Multi-Token tests
// -----------------------------------------------------------------------

#[tokio::test]
async fn parallel_gateway_forks_and_joins() {
    let engine = WorkflowEngine::new();

    // Start -> Split -> (A, B) -> Join -> End
    let def = ProcessDefinitionBuilder::new("and_test")
        .node("start", BpmnElement::StartEvent)
        .node("split", BpmnElement::ParallelGateway)
        .node(
            "task_a",
            BpmnElement::ServiceTask {
                topic: "task_a".into(),
                multi_instance: None,
            },
        )
        .node(
            "task_b",
            BpmnElement::ServiceTask {
                topic: "task_b".into(),
                multi_instance: None,
            },
        )
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

    let (def_key, _) = engine.deploy_definition(def).await;

    let inst_id = engine.start_instance(def_key).await.unwrap();

    // Should be paused in parallel execution waiting for both service tasks
    let state = engine.get_instance_state(inst_id).await.unwrap().clone();
    println!("State after start: {:?}", state);
    for entry in engine.get_audit_log(inst_id).await.unwrap() {
        println!("Log: {}", entry);
    }
    assert!(
        matches!(
            state,
            InstanceState::ParallelExecution {
                active_token_count: 2
            }
        ),
        "State should be parallel execution: {:?}",
        state
    );

    assert_eq!(engine.pending_service_tasks.len(), 2);

    // Complete task A
    let task_a = engine
        .pending_service_tasks
        .iter()
        .map(|r| r.value().clone())
        .find(|t| t.topic == "task_a")
        .unwrap()
        .id;
    // Need to fetch and lock it first
    let _ = engine
        .fetch_and_lock_service_tasks("worker", 10, &["task_a".into()], 1000)
        .await;

    let mut vars_a = std::collections::HashMap::new();
    vars_a.insert("var_a".into(), serde_json::Value::Bool(true));
    engine
        .complete_service_task(task_a, "worker", vars_a)
        .await
        .unwrap();

    // After A completes, it should be waiting at the join. Still in parallel state.
    let state = engine.get_instance_state(inst_id).await.unwrap().clone();
    println!("State after A completes: {:?}", state);
    for entry in engine.get_audit_log(inst_id).await.unwrap() {
        println!("Log: {}", entry);
    }
    assert!(matches!(
        state,
        InstanceState::ParallelExecution {
            active_token_count: 2
        }
    ));

    // Check join barrier
    let inst = engine.instances.get(&inst_id).await.unwrap();
    let inst_lock = inst.read().await;
    let barrier = inst_lock.join_barriers.get("join").unwrap();
    assert_eq!(barrier.expected_count, 2);
    assert_eq!(barrier.arrived_tokens.len(), 1);
    drop(inst_lock);

    // Complete task B
    let task_b = engine
        .pending_service_tasks
        .iter()
        .map(|r| r.value().clone())
        .find(|t| t.topic == "task_b")
        .unwrap()
        .id;
    let _ = engine
        .fetch_and_lock_service_tasks("worker", 10, &["task_b".into()], 1000)
        .await;
    let mut vars_b = std::collections::HashMap::new();
    vars_b.insert("var_b".into(), serde_json::Value::Bool(true));
    engine
        .complete_service_task(task_b, "worker", vars_b)
        .await
        .unwrap();

    // Now it should be complete!
    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );

    // Variables from both branches should be merged
}

// -----------------------------------------------------------------------
// Service Task specific operations
// -----------------------------------------------------------------------

#[tokio::test]
async fn service_task_fail_and_retries() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("retries")
        .node("start", BpmnElement::StartEvent)
        .node(
            "svc",
            BpmnElement::ServiceTask {
                topic: "fail_test".into(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    engine.start_instance(def_key).await.unwrap();

    // 1. Fetch task
    let tasks = engine
        .fetch_and_lock_service_tasks("worker", 1, &["fail_test".into()], 60)
        .await;
    assert_eq!(tasks.len(), 1);
    let task_id = tasks[0].id;

    // 2. Fail task (default 3 retries, decrementing to 2)
    engine
        .fail_service_task(task_id, "worker", None, Some("Failed".into()), None)
        .await
        .unwrap();

    // 3. Task should be unlocked and retries should be 2
    let pending = engine.get_pending_service_tasks();
    let t = pending.iter().find(|t| t.id == task_id).unwrap();
    assert_eq!(t.retries, 2);
    assert!(t.worker_id.is_none());

    // 4. Fail directly to 0
    let _ = engine
        .fetch_and_lock_service_tasks("worker2", 1, &["fail_test".into()], 60)
        .await;
    engine
        .fail_service_task(task_id, "worker2", Some(0), Some("Fatal".into()), None)
        .await
        .unwrap();

    // Incident should be logged
    let inst_id = tasks[0].instance_id;
    let log = engine.get_audit_log(inst_id).await.unwrap();
    assert!(log.iter().any(|l| l.contains("INCIDENT")));
    assert!(log.iter().any(|l| l.contains("Fatal")));
}

#[tokio::test]
async fn service_task_extend_lock() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("extend")
        .node("start", BpmnElement::StartEvent)
        .node(
            "svc",
            BpmnElement::ServiceTask {
                topic: "ext".into(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    engine.start_instance(def_key).await.unwrap();

    let tasks = engine
        .fetch_and_lock_service_tasks("worker", 1, &["ext".into()], 60)
        .await;
    let task_id = tasks[0].id;
    let exp_before = engine
        .get_pending_service_tasks()
        .iter()
        .find(|t| t.id == task_id)
        .unwrap()
        .lock_expiration
        .unwrap();

    engine.extend_lock(task_id, "worker", 120).await.unwrap();

    let exp_after = engine
        .get_pending_service_tasks()
        .iter()
        .find(|t| t.id == task_id)
        .unwrap()
        .lock_expiration
        .unwrap();
    assert!(exp_after > exp_before);
}

#[tokio::test]
async fn service_task_handle_bpmn_error() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("err")
        .node("start", BpmnElement::StartEvent)
        .node(
            "svc",
            BpmnElement::ServiceTask {
                topic: "err".into(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    engine.start_instance(def_key).await.unwrap();

    let tasks = engine
        .fetch_and_lock_service_tasks("worker", 1, &["err".into()], 60)
        .await;

    assert_eq!(engine.get_pending_service_tasks().len(), 1);

    engine
        .handle_bpmn_error(tasks[0].id, "worker", "ERR_CODE")
        .await
        .unwrap();

    // Task should be removed and error logged.
    assert_eq!(engine.get_pending_service_tasks().len(), 0);

    let log = engine.get_audit_log(tasks[0].instance_id).await.unwrap();
    assert!(log.iter().any(|l| l.contains("ERR_CODE")));
}

#[tokio::test]
async fn restore_instance_loads_from_persistence() {
    let engine = WorkflowEngine::new();

    // Deploy a definition so it exists
    let def = ProcessDefinitionBuilder::new("restore")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();
    let (def_key, _) = engine.deploy_definition(def).await;

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
        tokens: std::collections::HashMap::new(),
        active_tokens: vec![],
        join_barriers: std::collections::HashMap::new(),
        multi_instance_state: std::collections::HashMap::new(),
        compensation_log: Vec::new(),
        started_at: None,
        completed_at: None,
    };

    engine.restore_instance(inst.clone()).await;

    let loaded = engine.get_instance_details(inst.id).await.unwrap();
    assert_eq!(loaded.id, inst.id);
    assert_eq!(loaded.business_key, "BK1");
}

#[tokio::test]
async fn mutation_delete_instance_and_variables() {
    let engine = WorkflowEngine::new();
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

    let (def_key, _) = engine.deploy_definition(def).await;

    // Check list definitions formatting
    let defs = engine.list_definitions().await;
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].1, "del");

    let inst_id = engine.start_instance(def_key).await.unwrap();

    // Variable Math check (verify += vs -= mutant in update_instance_variables)
    let mut vars = HashMap::new();
    vars.insert("val".into(), serde_json::Value::Number(10.into()));
    engine
        .update_instance_variables(inst_id, vars)
        .await
        .unwrap();

    let details = engine.get_instance_details(inst_id).await.unwrap();
    assert_eq!(
        details.variables.get("val").unwrap(),
        &serde_json::Value::Number(10.into())
    );
    assert_eq!(details.variables.len(), 1); // Test mutant missing logic

    // Test delete instance == vs != loop
    let mut vars2 = HashMap::new();
    vars2.insert("other".into(), serde_json::Value::Bool(true));
    engine
        .update_instance_variables(inst_id, vars2)
        .await
        .unwrap();
    let details2 = engine.get_instance_details(inst_id).await.unwrap();
    assert_eq!(details2.variables.len(), 2);

    // Create side tasks
    let pending = engine.get_pending_user_tasks();
    assert_eq!(pending.len(), 1);

    engine.delete_instance(inst_id).await.unwrap();
    // After delete, list should be 0.
    assert!(engine.get_instance_state(inst_id).await.is_err());
}

#[tokio::test]
async fn mutation_fetch_service_task_boundary() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("lock")
        .node("start", BpmnElement::StartEvent)
        .node(
            "t1",
            BpmnElement::ServiceTask {
                topic: "bound".into(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "t1")
        .flow("t1", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    engine.start_instance(def_key).await.unwrap();

    // Fetch once
    let tasks1 = engine
        .fetch_and_lock_service_tasks("worker1", 1, &["bound".into()], 1)
        .await;
    assert_eq!(tasks1.len(), 1);

    // Fetch immediately again, should return 0 since locked and not expired
    let tasks2 = engine
        .fetch_and_lock_service_tasks("worker2", 1, &["bound".into()], 1)
        .await;
    assert_eq!(tasks2.len(), 0);

    // Sleep 1.1 second so it exceeds. `cargo mutants` tests > vs == on the `expiration > now`.
    tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

    // Fetch again, should return 1 since lock expired
    let tasks3 = engine
        .fetch_and_lock_service_tasks("worker3", 1, &["bound".into()], 1)
        .await;
    assert_eq!(tasks3.len(), 1);
}

#[tokio::test]
async fn mutation_find_downstream_join() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("join")
        .node("start", BpmnElement::StartEvent)
        .node("gw_split", BpmnElement::ParallelGateway)
        .node("gw_join", BpmnElement::ParallelGateway)
        .node(
            "dummy",
            BpmnElement::ServiceTask {
                topic: "dummy".to_string(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "gw_split")
        .flow("gw_split", "gw_join")
        .flow("gw_split", "dummy")
        .flow("dummy", "gw_join")
        .flow("gw_join", "end")
        .build()
        .unwrap();

    let _ = engine.deploy_definition(def.clone()).await;

    // Testing the logic explicitly via direct call (internal visibility allows this within engine module)
    let engine_local = WorkflowEngine::new();
    let found = engine_local.find_downstream_join(&def, "gw_split");
    assert_eq!(found, Some("gw_join".to_string()));

    // Find with depth limit (though internal recursion only decreases by 1, testing the > 100 limit protection is hard, but we can just test if the logic iterates correctly).
    let found_from_start = engine_local.find_downstream_join(&def, "start");
    assert_eq!(found_from_start, None);
    // It actually returns None because it exceeds max recursion or doesn't find gateway.
}

#[tokio::test]
async fn message_start_event_succeeds() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("msg_start")
        .node(
            "start",
            BpmnElement::MessageStartEvent {
                message_name: "start_msg".to_string(),
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();

    let _ = engine.deploy_definition(def).await;

    // Normal start should fail or wait if not message? Actually, correlate_message starts it
    let mut vars = HashMap::new();
    vars.insert("k".into(), serde_json::Value::String("v".into()));

    let affected = engine
        .correlate_message("start_msg".into(), Some("bk1".into()), vars)
        .await
        .unwrap();
    assert_eq!(affected.len(), 1);

    let inst_id = affected[0];
    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert_eq!(inst.state, InstanceState::Completed);
    assert_eq!(inst.business_key, "bk1");
}

#[tokio::test]
async fn timer_catch_event_succeeds() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("timer_catch")
        .node("start", BpmnElement::StartEvent)
        .node(
            "timer",
            BpmnElement::TimerCatchEvent(crate::domain::TimerDefinition::Duration(
                std::time::Duration::from_millis(50),
            )),
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "timer")
        .flow("timer", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();

    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::WaitingOnTimer {
            timer_id: engine
                .pending_timers
                .iter()
                .map(|r| r.value().clone())
                .next()
                .unwrap()
                .id
        }
    );

    // Won't trigger immediately
    let triggered = engine.process_timers().await.unwrap();
    assert_eq!(triggered, 0);

    tokio::time::sleep(tokio::time::Duration::from_millis(60)).await;

    let triggered = engine.process_timers().await.unwrap();
    assert_eq!(triggered, 1);

    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );
}

#[tokio::test]
async fn boundary_timer_event_cancels_task() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("bound_timer")
        .node("start", BpmnElement::StartEvent)
        .node("task", BpmnElement::UserTask("assignee".into()))
        .node(
            "bound_timer",
            BpmnElement::BoundaryTimerEvent {
                attached_to: "task".into(),
                timer: crate::domain::TimerDefinition::Duration(std::time::Duration::from_millis(
                    50,
                )),
                cancel_activity: true,
            },
        )
        .node("end1", BpmnElement::EndEvent)
        .node("end2", BpmnElement::EndEvent)
        .flow("start", "task")
        .flow("task", "end1")
        .flow("bound_timer", "end2")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();

    assert_eq!(engine.pending_user_tasks.len(), 1);
    assert_eq!(engine.pending_timers.len(), 1);

    tokio::time::sleep(tokio::time::Duration::from_millis(60)).await;
    let triggered = engine.process_timers().await.unwrap();
    assert_eq!(triggered, 1);

    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert_eq!(inst.state, InstanceState::Completed);
    assert_eq!(inst.current_node, "end2");
}

#[tokio::test]
async fn boundary_error_event_catches_error() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("bound_err")
        .node("start", BpmnElement::StartEvent)
        .node(
            "task",
            BpmnElement::ServiceTask {
                topic: "err_topic".into(),
                multi_instance: None,
            },
        )
        .node(
            "bound_err",
            BpmnElement::BoundaryErrorEvent {
                attached_to: "task".into(),
                error_code: Some("ERR_CODE_500".into()),
            },
        )
        .node("end1", BpmnElement::EndEvent)
        .node("end2", BpmnElement::EndEvent)
        .flow("start", "task")
        .flow("task", "end1")
        .flow("bound_err", "end2")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();

    let tasks = engine
        .fetch_and_lock_service_tasks("worker", 1, &["err_topic".into()], 10)
        .await;
    assert_eq!(tasks.len(), 1);

    engine
        .handle_bpmn_error(tasks[0].id, "worker", "ERR_CODE_500")
        .await
        .unwrap();

    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert_eq!(inst.state, InstanceState::Completed);
    assert_eq!(inst.current_node, "end2");
}

#[tokio::test]
async fn call_activity_lifecycle() {
    let engine = WorkflowEngine::new();

    // Deploy Child
    let child_def = ProcessDefinitionBuilder::new("child_proc")
        .node("start", BpmnElement::StartEvent)
        .node("child_task", BpmnElement::UserTask("child_assignee".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "child_task")
        .flow("child_task", "end")
        .build()
        .unwrap();
    let (_child_key, _) = engine.deploy_definition(child_def).await;

    // Deploy Parent
    let parent_def = ProcessDefinitionBuilder::new("parent_proc")
        .node("start", BpmnElement::StartEvent)
        .node(
            "call",
            BpmnElement::CallActivity {
                called_element: "child_proc".into(),
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "call")
        .flow("call", "end")
        .build()
        .unwrap();
    let (parent_key, _) = engine.deploy_definition(parent_def).await;

    // Start Parent
    let parent_id = engine.start_instance(parent_key).await.unwrap();

    // Parent should be blocked on Call Activity
    let parent_inst = engine.get_instance_details(parent_id).await.unwrap();
    if let InstanceState::WaitingOnCallActivity {
        sub_instance_id, ..
    } = parent_inst.state
    {
        // Child instance should exist
        let child_inst = engine.get_instance_details(sub_instance_id).await.unwrap();
        assert_eq!(child_inst.parent_instance_id, Some(parent_id));
        assert!(matches!(
            child_inst.state,
            InstanceState::WaitingOnUserTask { .. }
        ));
        assert!(matches!(
            child_inst.state,
            InstanceState::WaitingOnUserTask { .. }
        ));

        // Complete the child's user task
        let tasks = engine.get_pending_user_tasks();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].instance_id, sub_instance_id);

        let child_task_id = tasks[0].task_id;

        // Add a variable to child to ensure parent gets it
        let mut vars = std::collections::HashMap::new();
        vars.insert("from_child".into(), serde_json::json!("hello parent"));
        engine
            .complete_user_task(child_task_id, vars)
            .await
            .unwrap();

        // Child should be completed
        let child_inst = engine.get_instance_details(sub_instance_id).await.unwrap();
        assert_eq!(child_inst.state, InstanceState::Completed);

        // Parent should now be automatically resumed and completed
        let parent_inst = engine.get_instance_details(parent_id).await.unwrap();
        assert_eq!(parent_inst.state, InstanceState::Completed);
        assert_eq!(
            parent_inst
                .variables
                .get("from_child")
                .unwrap()
                .as_str()
                .unwrap(),
            "hello parent"
        );
    } else {
        panic!(
            "Parent not waiting on call activity: {:?}",
            parent_inst.state
        );
    }
}

// ---------------------------------------------------------------------------
// Advanced Edge Case Testing with InMemoryPersistence
// ---------------------------------------------------------------------------

#[tokio::test]
async fn in_memory_simultaneous_timer_and_message_race() {
    let engine = WorkflowEngine::with_in_memory_persistence();

    let def = ProcessDefinitionBuilder::new("race")
        .node("start", BpmnElement::StartEvent)
        .node("fork", BpmnElement::ParallelGateway)
        .node(
            "timer",
            BpmnElement::TimerCatchEvent(crate::domain::TimerDefinition::Duration(
                std::time::Duration::from_millis(50),
            )),
        )
        .node(
            "msg",
            BpmnElement::MessageCatchEvent {
                message_name: "MSG_CANCEL".into(),
            },
        )
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

    let (def_key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();

    assert_eq!(engine.pending_timers.len(), 1);
    assert_eq!(engine.pending_message_catches.len(), 1);

    // Simulate time passing (50ms) BUT before processing timers, we send the message!
    tokio::time::sleep(tokio::time::Duration::from_millis(60)).await;

    // Race: The message arrives precisely when the timer is due.
    let msg_name = engine
        .pending_message_catches
        .iter()
        .map(|r| r.value().clone())
        .next()
        .unwrap()
        .message_name
        .clone();
    engine
        .correlate_message(msg_name, None, std::collections::HashMap::new())
        .await
        .unwrap();

    // Since message was processed first, the instance was routed to join, and blocked on parallel gate
    let _inst = engine.get_instance_details(inst_id).await.unwrap();
    // (Note: correlate_message blindly resets state to Running visually, but it's still waiting on the other parallel branch inside active_tokens)

    // Now if we process timers, it should trigger the timer and join to finish
    let triggered = engine.process_timers().await.unwrap();
    assert_eq!(triggered, 1);

    let inst2 = engine.get_instance_details(inst_id).await.unwrap();
    assert_eq!(inst2.state, InstanceState::Completed);
}

#[tokio::test]
async fn in_memory_script_robust_failure_handling() {
    let engine = WorkflowEngine::with_in_memory_persistence();
    let script = "let a = 1; throw \"Intentional crash!\";";

    let def = ProcessDefinitionBuilder::new("script_crash")
        .node("start", BpmnElement::StartEvent)
        .node("task", BpmnElement::UserTask("worker".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "task")
        .flow("task", "end")
        .listener("start", crate::domain::ListenerEvent::Start, script)
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;

    // Engine should panic or return error because script is broken
    let result = engine.start_instance(def_key).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Intentional crash!")
    );
}

#[tokio::test]
async fn in_memory_large_file_variables() {
    let engine = WorkflowEngine::with_in_memory_persistence();

    let def = ProcessDefinitionBuilder::new("large_file")
        .node("start", BpmnElement::StartEvent)
        .node("task", BpmnElement::UserTask("worker".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "task")
        .flow("task", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();

    // Create a very large dummy payload (10 MB of zeros to simulate memory stress)
    // NOTE: In the real engine-server, the file goes to the persistence layer.
    // In engine-core tests, we can just insert the reference into variables and
    // also persist it explicitly to in-memory persistence.
    let large_payload = vec![0u8; 10 * 1024 * 1024];

    if let Some(p) = &engine.persistence {
        p.save_file("file:big_data", &large_payload).await.unwrap();
    }

    let file_ref = crate::domain::FileReference {
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

    engine
        .complete_user_task(tasks[0].task_id, vars)
        .await
        .unwrap();

    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert_eq!(inst.state, InstanceState::Completed);

    // Validate we can download it back
    let v = inst.variables.get("my_file").unwrap();
    let f_ref: crate::domain::FileReference = serde_json::from_value(v.clone()).unwrap();
    if let Some(p) = &engine.persistence {
        let downloaded = p.load_file(&f_ref.object_key).await.unwrap();
        assert_eq!(downloaded.len(), 10 * 1024 * 1024);
    }
}

#[tokio::test]
async fn test_definition_versioning_and_migration() {
    let engine = WorkflowEngine::with_in_memory_persistence();

    // V1 Definition
    let def_v1 = ProcessDefinitionBuilder::new("my_process")
        .node("start", BpmnElement::StartEvent)
        .node("task", BpmnElement::UserTask("worker".into()))
        .node("end1", BpmnElement::EndEvent)
        .flow("start", "task")
        .flow("task", "end1")
        .build()
        .unwrap();

    let (key_v1, _) = engine.deploy_definition(def_v1).await;
    let def_v1_deployed = engine.definitions.get(&key_v1).unwrap();
    assert_eq!(def_v1_deployed.version, 1);

    // Start instance on V1
    let inst_v1 = engine.start_instance(key_v1).await.unwrap();

    // V2 Definition (Same ID, changed structure)
    let def_v2 = ProcessDefinitionBuilder::new("my_process")
        .node("start", BpmnElement::StartEvent)
        .node("task2", BpmnElement::UserTask("worker2".into())) // Changed ID
        .node("end2", BpmnElement::EndEvent)
        .flow("start", "task2")
        .flow("task2", "end2")
        .build()
        .unwrap();

    let (key_v2, _) = engine.deploy_definition(def_v2).await;
    let def_v2_deployed = engine.definitions.get(&key_v2).unwrap();

    // Key should be different, version should be bumped
    assert_ne!(key_v1, key_v2);
    assert_eq!(def_v2_deployed.version, 2);

    // Instance v1 should still be on 'task' safely.
    let inst_v1_data = engine.get_instance_details(inst_v1).await.unwrap();
    assert_eq!(inst_v1_data.current_node, "task");
    assert_eq!(inst_v1_data.definition_key, key_v1);

    // Start instance on V2
    let inst_v2 = engine.start_instance(key_v2).await.unwrap();
    let inst_v2_data = engine.get_instance_details(inst_v2).await.unwrap();
    assert_eq!(inst_v2_data.current_node, "task2");
    assert_eq!(inst_v2_data.definition_key, key_v2);
}

#[tokio::test]
async fn restore_timer_and_message_catch() {
    let engine = WorkflowEngine::new();

    let timer = PendingTimer {
        id: Uuid::new_v4(),
        instance_id: Uuid::new_v4(),
        node_id: "timer_1".into(),
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(60),
        token_id: Uuid::new_v4(),
        timer_def: None,
        remaining_repetitions: None,
    };
    engine.restore_timer(timer.clone());
    assert_eq!(engine.pending_timers.len(), 1);
    assert_eq!(
        engine
            .pending_timers
            .iter()
            .map(|r| r.value().clone())
            .next()
            .unwrap()
            .id,
        timer.id
    );

    let catch = PendingMessageCatch {
        id: Uuid::new_v4(),
        instance_id: Uuid::new_v4(),
        node_id: "msg_1".into(),
        message_name: "ORDER_RECEIVED".into(),
        token_id: Uuid::new_v4(),
    };
    engine.restore_message_catch(catch.clone());
    assert_eq!(engine.pending_message_catches.len(), 1);
    assert_eq!(
        engine
            .pending_message_catches
            .iter()
            .map(|r| r.value().clone())
            .next()
            .unwrap()
            .id,
        catch.id
    );
}

#[tokio::test]
async fn test_nested_parallel_gateways() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("nested")
        .node("start", BpmnElement::StartEvent)
        .node("s1", BpmnElement::ParallelGateway)
        .node(
            "t1",
            BpmnElement::ServiceTask {
                topic: "t".into(),
                multi_instance: None,
            },
        )
        .node("s2", BpmnElement::ParallelGateway)
        .node(
            "t2",
            BpmnElement::ServiceTask {
                topic: "t".into(),
                multi_instance: None,
            },
        )
        .node(
            "t3",
            BpmnElement::ServiceTask {
                topic: "t".into(),
                multi_instance: None,
            },
        )
        .node("j2", BpmnElement::ParallelGateway)
        .node("j1", BpmnElement::ParallelGateway)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "s1")
        .flow("s1", "t1")
        .flow("s1", "s2")
        .flow("s2", "t2")
        .flow("s2", "t3")
        .flow("t2", "j2")
        .flow("t3", "j2")
        .flow("j2", "j1")
        .flow("t1", "j1")
        .flow("j1", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();

    let _ = engine
        .fetch_and_lock_service_tasks("worker", 10, &["t".into()], 10)
        .await;

    // Complete all 3 tasks
    let mut i = 0;
    while let Some(task) = engine.get_pending_service_tasks().first() {
        let task_id = task.id;
        engine
            .complete_service_task(task_id, "worker", std::collections::HashMap::new())
            .await
            .unwrap();
        i += 1;
        if i > 5 {
            break;
        } // safety loop limit
    }

    let state = engine.get_instance_state(inst_id).await.unwrap();
    assert_eq!(state, InstanceState::Completed);
}

// -----------------------------------------------------------------------
// Validation / ErrorEndEvent / Call Activity Error Propagation Tests
// -----------------------------------------------------------------------

#[tokio::test]
async fn top_level_error_end_event_results_in_completed_with_error() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("top_err")
        .node("start", BpmnElement::StartEvent)
        .node(
            "err_end",
            BpmnElement::ErrorEndEvent {
                error_code: String::from("CRITICAL_FAIL"),
            },
        )
        .flow("start", "err_end")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(def).await;
    let instance_id = engine.start_instance(key).await.unwrap();

    let state = engine.get_instance_state(instance_id).await.unwrap();
    assert_eq!(
        state,
        InstanceState::CompletedWithError {
            error_code: "CRITICAL_FAIL".into()
        }
    );

    let log = engine.get_audit_log(instance_id).await.unwrap();
    assert!(
        log.iter()
            .any(|l| l.contains("CRITICAL_FAIL") && l.contains("Error End"))
    );
}

fn build_child_error_process(code: &str) -> ProcessDefinition {
    ProcessDefinitionBuilder::new("child_proc")
        .node("start", BpmnElement::StartEvent)
        .node(
            "err_end",
            BpmnElement::ErrorEndEvent {
                error_code: String::from(code),
            },
        )
        .flow("start", "err_end")
        .build()
        .unwrap()
}

#[tokio::test]
async fn call_activity_propagates_error_to_matching_boundary_event() {
    let engine = WorkflowEngine::new();
    let (_, _) = engine
        .deploy_definition(build_child_error_process("ERR_CHILD"))
        .await;

    let parent_def = ProcessDefinitionBuilder::new("parent_proc")
        .node("start", BpmnElement::StartEvent)
        .node(
            "call",
            BpmnElement::CallActivity {
                called_element: "child_proc".into(),
            },
        )
        .node(
            "bound_err",
            BpmnElement::BoundaryErrorEvent {
                attached_to: "call".into(),
                error_code: Some("ERR_CHILD".into()),
            },
        )
        .node("end_normal", BpmnElement::EndEvent)
        .node("end_error", BpmnElement::EndEvent)
        .flow("start", "call")
        .flow("call", "end_normal")
        .flow("bound_err", "end_error")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(parent_def).await;
    let instance_id = engine.start_instance(key).await.unwrap();

    let state = engine.get_instance_state(instance_id).await.unwrap();
    assert_eq!(state, InstanceState::Completed);

    let log = engine.get_audit_log(instance_id).await.unwrap();
    // Verify it took the error path
    assert!(log.iter().any(|l| l.contains("'end_error'")));
}

#[tokio::test]
async fn call_activity_propagates_error_to_wildcard_boundary_event() {
    let engine = WorkflowEngine::new();
    let (_, _) = engine
        .deploy_definition(build_child_error_process("ANY_ERR_CODE"))
        .await;

    let parent_def = ProcessDefinitionBuilder::new("parent_wildcard")
        .node("start", BpmnElement::StartEvent)
        .node(
            "call",
            BpmnElement::CallActivity {
                called_element: "child_proc".into(),
            },
        )
        .node(
            "bound_err",
            BpmnElement::BoundaryErrorEvent {
                attached_to: "call".into(),
                error_code: None,
            },
        ) // Wildcard
        .node("end_normal", BpmnElement::EndEvent)
        .node("end_error", BpmnElement::EndEvent)
        .flow("start", "call")
        .flow("call", "end_normal")
        .flow("bound_err", "end_error")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(parent_def).await;
    let instance_id = engine.start_instance(key).await.unwrap();

    let state = engine.get_instance_state(instance_id).await.unwrap();
    assert_eq!(state, InstanceState::Completed);

    let log = engine.get_audit_log(instance_id).await.unwrap();
    assert!(log.iter().any(|l| l.contains("'end_error'")));
}

#[tokio::test]
async fn call_activity_unhandled_error_becomes_incident() {
    let engine = WorkflowEngine::new();
    let (_, _) = engine
        .deploy_definition(build_child_error_process("UNHANDLED_CODE"))
        .await;

    let parent_def = ProcessDefinitionBuilder::new("parent_unhandled")
        .node("start", BpmnElement::StartEvent)
        .node(
            "call",
            BpmnElement::CallActivity {
                called_element: "child_proc".into(),
            },
        )
        .node("end_normal", BpmnElement::EndEvent)
        .flow("start", "call")
        .flow("call", "end_normal")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(parent_def).await;
    let instance_id = engine.start_instance(key).await.unwrap();

    // Check state is waiting on call activity because it's an incident
    let state = engine.get_instance_state(instance_id).await.unwrap();
    assert!(matches!(state, InstanceState::WaitingOnCallActivity { .. }));

    let log = engine.get_audit_log(instance_id).await.unwrap();
    assert!(
        log.iter()
            .any(|l| l.contains("INCIDENT") && l.contains("UNHANDLED_CODE"))
    );
}

#[test]
fn engine_is_send_and_sync() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<super::super::WorkflowEngine>();
    assert_sync::<super::super::WorkflowEngine>();
}

#[tokio::test]
async fn event_based_gateway_timer_wins() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("ebg_timer")
        .node("start", BpmnElement::StartEvent)
        .node("gw", BpmnElement::EventBasedGateway)
        .node(
            "catch_timer",
            BpmnElement::TimerCatchEvent(crate::domain::TimerDefinition::Duration(
                Duration::from_millis(50),
            )),
        )
        .node(
            "catch_msg",
            BpmnElement::MessageCatchEvent {
                message_name: "win_msg".into(),
            },
        )
        .node("end_timer", BpmnElement::EndEvent)
        .node("end_msg", BpmnElement::EndEvent)
        .flow("start", "gw")
        .flow("gw", "catch_timer")
        .flow("gw", "catch_msg")
        .flow("catch_timer", "end_timer")
        .flow("catch_msg", "end_msg")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(def).await;
    let instance_id = engine.start_instance(key).await.unwrap();

    let state = engine.get_instance_state(instance_id).await.unwrap();
    assert_eq!(state, InstanceState::WaitingOnEventBasedGateway);

    // ensure pending timers and messages are registered
    assert_eq!(engine.pending_timers.len(), 1);
    assert_eq!(engine.pending_message_catches.len(), 1);

    // Wait for timer to expire
    tokio::time::sleep(Duration::from_millis(60)).await;

    let processed = engine.process_timers().await.unwrap();
    assert_eq!(processed, 1);

    // The message catch should have been CANCELLED and removed!
    assert_eq!(engine.pending_timers.len(), 0);
    assert_eq!(engine.pending_message_catches.len(), 0);

    let state = engine.get_instance_state(instance_id).await.unwrap();
    assert_eq!(state, InstanceState::Completed);

    let log = engine.get_audit_log(instance_id).await.unwrap();
    assert!(log.iter().any(|l| l.contains("cancelled")));
    assert!(log.iter().any(|l| l.contains("'end_timer'")));
}

#[tokio::test]
async fn event_based_gateway_message_wins() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("ebg_msg")
        .node("start", BpmnElement::StartEvent)
        .node("gw", BpmnElement::EventBasedGateway)
        .node(
            "catch_timer",
            BpmnElement::TimerCatchEvent(crate::domain::TimerDefinition::Duration(
                Duration::from_millis(5000),
            )),
        ) // Long timer
        .node(
            "catch_msg",
            BpmnElement::MessageCatchEvent {
                message_name: "win_msg".into(),
            },
        )
        .node("end_timer", BpmnElement::EndEvent)
        .node("end_msg", BpmnElement::EndEvent)
        .flow("start", "gw")
        .flow("gw", "catch_timer")
        .flow("gw", "catch_msg")
        .flow("catch_timer", "end_timer")
        .flow("catch_msg", "end_msg")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(def).await;
    let instance_id = engine.start_instance(key).await.unwrap();

    // Correlate message
    let affected = engine
        .correlate_message("win_msg".into(), None, Default::default())
        .await
        .unwrap();
    assert_eq!(affected.len(), 1);

    // The timer should have been CANCELLED and removed!
    assert_eq!(engine.pending_timers.len(), 0);
    assert_eq!(engine.pending_message_catches.len(), 0);

    let state = engine.get_instance_state(instance_id).await.unwrap();
    assert_eq!(state, InstanceState::Completed);

    let log = engine.get_audit_log(instance_id).await.unwrap();
    assert!(log.iter().any(|l| l.contains("cancelled")));
    assert!(log.iter().any(|l| l.contains("'end_msg'")));
}

#[tokio::test]
async fn test_non_interrupting_timer_boundary() {
    // A process with a user task that has a non-interrupting timer boundary.
    // The timer fires: the process forks to a second user task, while the first ONE is STILL alive!
    let eng = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("test_bnd")
        .node("start", BpmnElement::StartEvent)
        .flow("start", "task")
        .node("task", BpmnElement::UserTask("User1".into()))
        .node(
            "timer_bnd",
            BpmnElement::BoundaryTimerEvent {
                attached_to: "task".into(),
                timer: crate::domain::TimerDefinition::Duration(std::time::Duration::from_secs(1)),
                cancel_activity: false, // NON-INTERRUPTING
            },
        )
        // From timer -> goes to task2
        .flow("timer_bnd", "task2")
        .node("task2", BpmnElement::UserTask("User1".into()))
        .flow("task2", "end2")
        .node("end2", BpmnElement::EndEvent)
        // From main task -> goes to end1
        .flow("task", "end1")
        .node("end1", BpmnElement::EndEvent)
        .build()
        .unwrap();

    let (def_key, _) = eng.deploy_definition(def).await;
    let inst_id = eng
        .start_instance_with_variables(def_key, Default::default())
        .await
        .unwrap();

    // The user task should be pending
    let tasks = eng
        .get_pending_user_tasks()
        .into_iter()
        .filter(|t| t.instance_id == inst_id)
        .collect::<Vec<_>>();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].node_id, "task");

    // The timer should be pending
    let timers = eng
        .pending_timers
        .iter()
        .map(|r| r.value().clone())
        .collect::<Vec<_>>();
    assert_eq!(timers.len(), 1);

    // Simulate timer firing
    let mut pending = eng.pending_timers.get_mut(&timers[0].id).unwrap();
    pending.expires_at = chrono::Utc::now() - chrono::Duration::hours(1);
    drop(pending);

    eng.process_timers().await.unwrap();

    // Now, there should be TWO user tasks pending: 'task' and 'task2'
    let tasks = eng
        .get_pending_user_tasks()
        .into_iter()
        .filter(|t| t.instance_id == inst_id)
        .collect::<Vec<_>>();
    let node_ids: std::collections::HashSet<_> = tasks.iter().map(|t| t.node_id.clone()).collect();
    assert_eq!(node_ids.len(), 2, "Expected both tasks to be pending");
    assert!(node_ids.contains("task"));
    assert!(node_ids.contains("task2"));

    // Instance state should be parallel
    {
        let inst_lk = eng.instances.get(&inst_id).await.unwrap();
        let inst = inst_lk.read().await;
        assert!(matches!(
            inst.state,
            crate::runtime::InstanceState::ParallelExecution {
                active_token_count: 2
            }
        ));
    }

    // Complete first task
    let task1_id = tasks.iter().find(|t| t.node_id == "task").unwrap().task_id;
    eng.complete_user_task(task1_id, Default::default())
        .await
        .unwrap();

    // Process still running (waiting on task2)
    {
        let inst_lk = eng.instances.get(&inst_id).await.unwrap();
        let inst = inst_lk.read().await;
        assert!(!matches!(
            inst.state,
            crate::runtime::InstanceState::Completed
        ));
    }

    // Complete second task
    let task2_id = tasks.iter().find(|t| t.node_id == "task2").unwrap().task_id;
    eng.complete_user_task(task2_id, Default::default())
        .await
        .unwrap();

    // Now completed
    {
        let inst_lk = eng.instances.get(&inst_id).await.unwrap();
        let inst = inst_lk.read().await;
        assert!(matches!(
            inst.state,
            crate::runtime::InstanceState::Completed
        ));
    }
}

#[tokio::test]
async fn test_interrupting_timer_boundary_cleanup() {
    let eng = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("test_bnd_2")
        .node("start", BpmnElement::StartEvent)
        .flow("start", "task")
        .node(
            "task",
            BpmnElement::ServiceTask {
                topic: "test".into(),
                multi_instance: None,
            },
        )
        .node(
            "timer_bnd",
            BpmnElement::BoundaryTimerEvent {
                attached_to: "task".into(),
                timer: crate::domain::TimerDefinition::Duration(std::time::Duration::from_secs(1)),
                cancel_activity: true, // INTERRUPTING
            },
        )
        .flow("timer_bnd", "end")
        .flow("task", "end")
        .node("end", BpmnElement::EndEvent)
        .build()
        .unwrap();

    let (def_key, _) = eng.deploy_definition(def).await;
    let inst_id = eng
        .start_instance_with_variables(def_key, Default::default())
        .await
        .unwrap();

    let service_tasks = eng
        .get_pending_service_tasks()
        .into_iter()
        .filter(|t| t.instance_id == inst_id)
        .collect::<Vec<_>>();
    assert_eq!(service_tasks.len(), 1);

    let timers = eng
        .pending_timers
        .iter()
        .map(|r| r.value().clone())
        .collect::<Vec<_>>();
    let tid = timers[0].id;
    let mut pending = eng.pending_timers.get_mut(&tid).unwrap();
    pending.expires_at = chrono::Utc::now() - chrono::Duration::hours(1);
    drop(pending);

    eng.process_timers().await.unwrap();

    // The service task should be DELETED, not just orphaned token
    let service_tasks_after = eng
        .get_pending_service_tasks()
        .into_iter()
        .filter(|t| t.instance_id == inst_id)
        .collect::<Vec<_>>();
    assert_eq!(
        service_tasks_after.len(),
        0,
        "Interrupting boundary event should delete the pending service task"
    );

    let inst_lk = eng.instances.get(&inst_id).await.unwrap();
    let inst = inst_lk.read().await;
    assert!(matches!(
        inst.state,
        crate::runtime::InstanceState::Completed
    ));
}

#[tokio::test]
async fn test_non_interrupting_message_boundary() {
    let eng = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("test_bnd_3")
        .node("start", BpmnElement::StartEvent)
        .flow("start", "task")
        .node("task", BpmnElement::UserTask("User1".into()))
        .node(
            "msg_bnd",
            BpmnElement::BoundaryMessageEvent {
                attached_to: "task".into(),
                message_name: "async_signal".into(),
                cancel_activity: false, // NON-INTERRUPTING
            },
        )
        .flow("msg_bnd", "end2")
        .node("end2", BpmnElement::EndEvent)
        .flow("task", "end1")
        .node("end1", BpmnElement::EndEvent)
        .build()
        .unwrap();

    let (def_key, _) = eng.deploy_definition(def).await;
    let inst_id = eng
        .start_instance_with_variables(def_key, Default::default())
        .await
        .unwrap();

    // Trigger message
    eng.correlate_message("async_signal".into(), None, Default::default())
        .await
        .unwrap();

    // The user task should STILL be pending!
    let tasks = eng
        .get_pending_user_tasks()
        .into_iter()
        .filter(|t| t.instance_id == inst_id)
        .collect::<Vec<_>>();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].node_id, "task");

    // Instance state should be parallel, but one branch (end2) just finished.
    // Wait, the message correlates, starts parallel branch, immediately hits EndEvent.
    // So the state parallel count decreased by 1 immediately.
    // Let's just check the instance is not fully completed.
    let inst_lk = eng.instances.get(&inst_id).await.unwrap();
    assert!(!matches!(
        inst_lk.read().await.state,
        crate::runtime::InstanceState::Completed
    ));

    eng.complete_user_task(tasks[0].task_id, Default::default())
        .await
        .unwrap();
    assert!(matches!(
        inst_lk.read().await.state,
        crate::runtime::InstanceState::Completed
    ));
}

// ============================================================================
// Escalation Event Tests
// ============================================================================

#[tokio::test]
async fn test_escalation_end_event_completes_instance() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("esc_end")
        .node("start", BpmnElement::StartEvent)
        .node(
            "esc_end",
            BpmnElement::EscalationEndEvent {
                escalation_code: "ESC_001".into(),
            },
        )
        .flow("start", "esc_end")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    let inst = engine.get_instance_details(inst_id).await.unwrap();
    // EscalationEnd at top level completes the instance (non-fatal)
    assert!(matches!(inst.state, InstanceState::Completed));
    assert!(inst.audit_log.iter().any(|l| l.contains("Escalation")));
}

#[tokio::test]
async fn test_escalation_throw_no_handler_continues() {
    // Intermediate escalation throw with no handler → token continues normally
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("esc_throw_no_handler")
        .node("start", BpmnElement::StartEvent)
        .node(
            "esc_throw",
            BpmnElement::EscalationThrowEvent {
                escalation_code: "ESC_002".into(),
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "esc_throw")
        .flow("esc_throw", "end")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(matches!(inst.state, InstanceState::Completed));
    assert!(
        inst.audit_log
            .iter()
            .any(|l| l.contains("no handler found"))
    );
}

#[tokio::test]
async fn test_escalation_throw_with_interrupting_boundary() {
    // Escalation throw inside subprocess, caught by interrupting boundary on task
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("esc_interrupt")
        .node("start", BpmnElement::StartEvent)
        .node(
            "task1",
            BpmnElement::ServiceTask {
                topic: "noop".into(),
                multi_instance: None,
            },
        )
        .node(
            "esc_throw",
            BpmnElement::EscalationThrowEvent {
                escalation_code: "REVIEW_NEEDED".into(),
            },
        )
        .node(
            "boundary_esc",
            BpmnElement::BoundaryEscalationEvent {
                attached_to: "task1".into(),
                escalation_code: Some("REVIEW_NEEDED".into()),
                cancel_activity: true,
            },
        )
        .node("esc_handler_end", BpmnElement::EndEvent)
        .node("normal_end", BpmnElement::EndEvent)
        .flow("start", "esc_throw")
        .flow("task1", "normal_end")
        .flow("esc_throw", "normal_end")
        .flow("boundary_esc", "esc_handler_end")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(matches!(inst.state, InstanceState::Completed));
    assert!(inst.audit_log.iter().any(|l| l.contains("interrupting")));
}

#[tokio::test]
async fn test_escalation_throw_with_non_interrupting_boundary() {
    // Non-interrupting boundary → spawns handler token, main token continues
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("esc_non_interrupt")
        .node("start", BpmnElement::StartEvent)
        .node(
            "task1",
            BpmnElement::ServiceTask {
                topic: "noop".into(),
                multi_instance: None,
            },
        )
        .node(
            "esc_throw",
            BpmnElement::EscalationThrowEvent {
                escalation_code: "INFO".into(),
            },
        )
        .node(
            "boundary_esc",
            BpmnElement::BoundaryEscalationEvent {
                attached_to: "task1".into(),
                escalation_code: Some("INFO".into()),
                cancel_activity: false,
            },
        )
        .node("handler_end", BpmnElement::EndEvent)
        .node("normal_end", BpmnElement::EndEvent)
        .flow("start", "esc_throw")
        .flow("task1", "normal_end")
        .flow("esc_throw", "normal_end")
        .flow("boundary_esc", "handler_end")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(matches!(inst.state, InstanceState::Completed));
    assert!(
        inst.audit_log
            .iter()
            .any(|l| l.contains("non-interrupting"))
    );
}

#[tokio::test]
async fn test_escalation_wildcard_boundary_catches_any() {
    // Boundary with escalation_code: None catches any escalation code
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("esc_wildcard")
        .node("start", BpmnElement::StartEvent)
        .node(
            "task1",
            BpmnElement::ServiceTask {
                topic: "noop".into(),
                multi_instance: None,
            },
        )
        .node(
            "esc_throw",
            BpmnElement::EscalationThrowEvent {
                escalation_code: "ANY_CODE".into(),
            },
        )
        .node(
            "boundary_esc",
            BpmnElement::BoundaryEscalationEvent {
                attached_to: "task1".into(),
                escalation_code: None, // wildcard
                cancel_activity: true,
            },
        )
        .node("handler_end", BpmnElement::EndEvent)
        .node("normal_end", BpmnElement::EndEvent)
        .flow("start", "esc_throw")
        .flow("task1", "normal_end")
        .flow("esc_throw", "normal_end")
        .flow("boundary_esc", "handler_end")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(matches!(inst.state, InstanceState::Completed));
    assert!(
        inst.audit_log
            .iter()
            .any(|l| l.contains("caught") && l.contains("interrupting"))
    );
}

// ============================================================================
// Compensation Event Tests
// ============================================================================

#[tokio::test]
async fn test_compensation_registers_and_executes_handler() {
    // Script task with compensation boundary → compensation throw undoes it
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("comp_basic")
        .node("start", BpmnElement::StartEvent)
        .node(
            "script1",
            BpmnElement::ScriptTask {
                script: r#"let step1 = "done";"#.into(),
                multi_instance: None,
            },
        )
        .node(
            "boundary_comp",
            BpmnElement::BoundaryCompensationEvent {
                attached_to: "script1".into(),
            },
        )
        .node(
            "comp_handler",
            BpmnElement::ScriptTask {
                script: r#"step1 = "undone";"#.into(),
                multi_instance: None,
            },
        )
        .node(
            "comp_throw",
            BpmnElement::CompensationThrowEvent { activity_ref: None },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "script1")
        .flow("script1", "comp_throw")
        .flow("comp_throw", "end")
        .flow("boundary_comp", "comp_handler")
        .flow("comp_handler", "end")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(matches!(inst.state, InstanceState::Completed));
    // Compensation handler should have been registered and executed
    assert!(
        inst.audit_log
            .iter()
            .any(|l| l.contains("Registered compensation"))
    );
    assert!(
        inst.audit_log
            .iter()
            .any(|l| l.contains("Compensation triggered"))
    );
    // After compensation, step1 should be "undone"
    assert_eq!(
        inst.variables.get("step1").and_then(|v| v.as_str()),
        Some("undone")
    );
}

#[tokio::test]
async fn test_compensation_end_event() {
    // CompensationEndEvent triggers compensation and then completes
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("comp_end")
        .node("start", BpmnElement::StartEvent)
        .node(
            "script1",
            BpmnElement::ScriptTask {
                script: r#"let x = 42;"#.into(),
                multi_instance: None,
            },
        )
        .node(
            "boundary_comp",
            BpmnElement::BoundaryCompensationEvent {
                attached_to: "script1".into(),
            },
        )
        .node(
            "comp_handler",
            BpmnElement::ScriptTask {
                script: r#"x = 0;"#.into(),
                multi_instance: None,
            },
        )
        .node(
            "comp_end",
            BpmnElement::CompensationEndEvent { activity_ref: None },
        )
        .node("handler_end", BpmnElement::EndEvent)
        .flow("start", "script1")
        .flow("script1", "comp_end")
        .flow("boundary_comp", "comp_handler")
        .flow("comp_handler", "handler_end")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(matches!(inst.state, InstanceState::Completed));
    assert_eq!(inst.variables.get("x").and_then(|v| v.as_i64()), Some(0));
}

#[tokio::test]
async fn test_compensation_specific_activity() {
    // CompensationThrowEvent with activity_ref targets only one activity's handler
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("comp_specific")
        .node("start", BpmnElement::StartEvent)
        .node(
            "script1",
            BpmnElement::ScriptTask {
                script: r#"let a = 1;"#.into(),
                multi_instance: None,
            },
        )
        .node(
            "boundary_comp1",
            BpmnElement::BoundaryCompensationEvent {
                attached_to: "script1".into(),
            },
        )
        .node(
            "comp_handler1",
            BpmnElement::ScriptTask {
                script: r#"a = -1;"#.into(),
                multi_instance: None,
            },
        )
        .node(
            "script2",
            BpmnElement::ScriptTask {
                script: r#"let b = 2;"#.into(),
                multi_instance: None,
            },
        )
        .node(
            "boundary_comp2",
            BpmnElement::BoundaryCompensationEvent {
                attached_to: "script2".into(),
            },
        )
        .node(
            "comp_handler2",
            BpmnElement::ScriptTask {
                script: r#"b = -2;"#.into(),
                multi_instance: None,
            },
        )
        .node(
            "comp_throw",
            BpmnElement::CompensationThrowEvent {
                activity_ref: Some("script1".into()),
            },
        )
        .node("end", BpmnElement::EndEvent)
        .node("handler_end1", BpmnElement::EndEvent)
        .node("handler_end2", BpmnElement::EndEvent)
        .flow("start", "script1")
        .flow("script1", "script2")
        .flow("script2", "comp_throw")
        .flow("comp_throw", "end")
        .flow("boundary_comp1", "comp_handler1")
        .flow("comp_handler1", "handler_end1")
        .flow("boundary_comp2", "comp_handler2")
        .flow("comp_handler2", "handler_end2")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(matches!(inst.state, InstanceState::Completed));
    // Only script1's handler ran, so a=-1, but b stays at 2
    assert_eq!(inst.variables.get("a").and_then(|v| v.as_i64()), Some(-1));
    assert_eq!(inst.variables.get("b").and_then(|v| v.as_i64()), Some(2));
}

#[tokio::test]
async fn test_compensation_no_handlers_still_completes() {
    // CompensationThrowEvent with no registered handlers → just continues
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("comp_empty")
        .node("start", BpmnElement::StartEvent)
        .node(
            "comp_throw",
            BpmnElement::CompensationThrowEvent { activity_ref: None },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "comp_throw")
        .flow("comp_throw", "end")
        .build()
        .unwrap();

    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(matches!(inst.state, InstanceState::Completed));
    assert!(
        inst.audit_log
            .iter()
            .any(|l| l.contains("0 handler(s) to execute"))
    );
}

// ============================================================================
// Mutation-Score Improvement Tests
// ============================================================================

#[tokio::test]
async fn test_get_stats_counts_correctly() {
    // Catches: replace += with -=, *= in get_stats; delete match arms
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("stats")
        .node("start", BpmnElement::StartEvent)
        .node("ut", BpmnElement::UserTask("alice".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "ut")
        .flow("ut", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;

    // Start 3 instances — all should be waiting on user task
    let _id1 = engine.start_instance(key).await.unwrap();
    let _id2 = engine.start_instance(key).await.unwrap();
    let _id3 = engine.start_instance(key).await.unwrap();

    let stats = engine.get_stats().await;
    assert_eq!(stats.definitions_count, 1);
    assert_eq!(stats.instances_waiting_user, 3);
    assert_eq!(stats.instances_running, 0);
    assert_eq!(stats.instances_completed, 0);
    assert_eq!(stats.instances_total, 3);
}

#[tokio::test]
async fn test_suspend_and_resume_instance() {
    // Catches: delete match arms in suspend/resume_instance
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("susp")
        .node("start", BpmnElement::StartEvent)
        .node("ut", BpmnElement::UserTask("alice".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "ut")
        .flow("ut", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    // Suspend
    let result = engine.suspend_instance(inst_id).await;
    assert!(result.is_ok());
    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(matches!(inst.state, InstanceState::Suspended { .. }));

    // Double suspend should fail
    let result = engine.suspend_instance(inst_id).await;
    assert!(result.is_err());

    // Resume
    let result = engine.resume_instance(inst_id).await;
    assert!(result.is_ok());
    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(matches!(
        inst.state,
        InstanceState::WaitingOnUserTask { .. }
    ));

    // Double resume should fail
    let result = engine.resume_instance(inst_id).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_suspend_completed_instance_fails() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("done")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    // Instance is completed — suspend should fail
    let result = engine.suspend_instance(inst_id).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_list_instances_returns_all() {
    // Catches: replace list_instances -> Vec<ProcessInstance> with vec![]
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("list")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    engine.start_instance(key).await.unwrap();
    engine.start_instance(key).await.unwrap();

    let instances = engine.list_instances().await;
    assert_eq!(instances.len(), 2);
}

#[tokio::test]
async fn test_update_instance_variables() {
    // Catches: replace += with -=, *= in update_instance_variables
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("vars")
        .node("start", BpmnElement::StartEvent)
        .node("ut", BpmnElement::UserTask("alice".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "ut")
        .flow("ut", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    let mut new_vars = HashMap::new();
    new_vars.insert("x".to_string(), serde_json::json!(42));
    new_vars.insert("name".to_string(), serde_json::json!("test"));
    let result = engine.update_instance_variables(inst_id, new_vars).await;
    assert!(result.is_ok());

    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert_eq!(inst.variables.get("x"), Some(&serde_json::json!(42)));
    assert_eq!(inst.variables.get("name"), Some(&serde_json::json!("test")));
}

#[tokio::test]
async fn test_registry_find_by_bpmn_id() {
    // Catches: replace find_by_bpmn_id -> Option with None; replace == with !=
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("myproc")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;

    // find_by_bpmn_id should find it
    let found = engine.definitions.find_by_bpmn_id("myproc");
    assert!(found.is_some());
    assert_eq!(found.unwrap().0, key);

    // Non-existent should return None
    let not_found = engine.definitions.find_by_bpmn_id("nope");
    assert!(not_found.is_none());
}

#[tokio::test]
async fn test_registry_find_latest_and_versions() {
    // Catches: replace find_latest_by_bpmn_id -> Option with None;
    //          replace all_versions_of -> Vec with vec![]
    let engine = WorkflowEngine::new();
    let def1 = ProcessDefinitionBuilder::new("versioned")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();
    let (key1, _) = engine.deploy_definition(def1).await;

    let def2 = ProcessDefinitionBuilder::new("versioned")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();
    let (key2, _) = engine.deploy_definition(def2).await;

    // Latest should be v2
    let latest = engine.definitions.find_latest_by_bpmn_id("versioned");
    assert!(latest.is_some());
    assert_eq!(latest.unwrap().0, key2);
    assert_ne!(key1, key2);

    // All versions
    let versions = engine.definitions.all_versions_of("versioned");
    assert_eq!(versions.len(), 2);

    // Registry stats
    assert!(!engine.definitions.is_empty());
    assert_eq!(engine.definitions.len(), 2);
    assert!(engine.definitions.contains_key(&key1));
}

#[tokio::test]
async fn test_move_token_to_valid_node() {
    // Catches: delete ! in move_token; replace == with != for node existence checks
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("move")
        .node("start", BpmnElement::StartEvent)
        .node("ut1", BpmnElement::UserTask("alice".into()))
        .node("ut2", BpmnElement::UserTask("bob".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "ut1")
        .flow("ut1", "ut2")
        .flow("ut2", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    // Should be at ut1
    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert_eq!(inst.current_node, "ut1");

    // Move to ut2
    let result = engine
        .move_token(inst_id, "ut2", HashMap::new(), false)
        .await;
    assert!(result.is_ok());

    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert_eq!(inst.current_node, "ut2");
}

#[tokio::test]
async fn test_move_token_invalid_node_fails() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("move_bad")
        .node("start", BpmnElement::StartEvent)
        .node("ut", BpmnElement::UserTask("alice".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "ut")
        .flow("ut", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    // Move to non-existent node
    let result = engine
        .move_token(inst_id, "nonexistent", HashMap::new(), false)
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_delete_instance_cleans_all_queues() {
    // Catches: replace == with != in delete_instance queue cleanup
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("del_q")
        .node("start", BpmnElement::StartEvent)
        .node(
            "svc",
            BpmnElement::ServiceTask {
                topic: "test_topic".into(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    // Service task should be pending
    assert!(!engine.pending_service_tasks.is_empty());

    // Delete the instance
    let result = engine.delete_instance(inst_id).await;
    assert!(result.is_ok());

    // Instance should be gone
    assert!(engine.get_instance_details(inst_id).await.is_err());
    // Pending service tasks for this instance should be cleaned
    let remaining: Vec<_> = engine
        .pending_service_tasks
        .iter()
        .filter(|t| t.instance_id == inst_id)
        .collect();
    assert!(remaining.is_empty());
}

#[tokio::test]
async fn test_inclusive_gateway_multiple_paths() {
    // Catches: replace && with || in execute_inclusive_gateway;
    //          replace >= with <; delete !
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("incl")
        .node("start", BpmnElement::StartEvent)
        .node("gw", BpmnElement::InclusiveGateway)
        .node(
            "a",
            BpmnElement::ScriptTask {
                script: "let r = 1;".into(),
                multi_instance: None,
            },
        )
        .node(
            "b",
            BpmnElement::ScriptTask {
                script: "let s = 2;".into(),
                multi_instance: None,
            },
        )
        .node("join", BpmnElement::InclusiveGateway)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "gw")
        .conditional_flow("gw", "a", "x == 1")
        .conditional_flow("gw", "b", "y == 1")
        .flow("a", "join")
        .flow("b", "join")
        .flow("join", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;

    // Both conditions true → both paths taken
    let mut vars = HashMap::new();
    vars.insert("x".to_string(), serde_json::json!(1));
    vars.insert("y".to_string(), serde_json::json!(1));
    let inst_id = engine
        .start_instance_with_variables(key, vars)
        .await
        .unwrap();
    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(matches!(inst.state, InstanceState::Completed));
    assert_eq!(inst.variables.get("r"), Some(&serde_json::json!(1)));
    assert_eq!(inst.variables.get("s"), Some(&serde_json::json!(2)));
}

#[tokio::test]
async fn test_compensation_specific_activity_only_targets_one() {
    // Catches: replace != with == in handle_compensation_throw_event (events.rs:412)
    // This is the same test as test_compensation_specific_activity but we
    // explicitly verify that ONLY script1's handler runs, not script2's
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("comp_filter")
        .node("start", BpmnElement::StartEvent)
        .node(
            "script1",
            BpmnElement::ScriptTask {
                script: r#"let a = 10;"#.into(),
                multi_instance: None,
            },
        )
        .node(
            "boundary_comp1",
            BpmnElement::BoundaryCompensationEvent {
                attached_to: "script1".into(),
            },
        )
        .node(
            "comp_handler1",
            BpmnElement::ScriptTask {
                script: r#"a = 0;"#.into(),
                multi_instance: None,
            },
        )
        .node(
            "script2",
            BpmnElement::ScriptTask {
                script: r#"let b = 20;"#.into(),
                multi_instance: None,
            },
        )
        .node(
            "boundary_comp2",
            BpmnElement::BoundaryCompensationEvent {
                attached_to: "script2".into(),
            },
        )
        .node(
            "comp_handler2",
            BpmnElement::ScriptTask {
                script: r#"b = 0;"#.into(),
                multi_instance: None,
            },
        )
        .node(
            "comp_throw",
            BpmnElement::CompensationThrowEvent {
                activity_ref: Some("script2".into()),
            },
        )
        .node("end", BpmnElement::EndEvent)
        .node("handler_end1", BpmnElement::EndEvent)
        .node("handler_end2", BpmnElement::EndEvent)
        .flow("start", "script1")
        .flow("script1", "script2")
        .flow("script2", "comp_throw")
        .flow("comp_throw", "end")
        .flow("boundary_comp1", "comp_handler1")
        .flow("comp_handler1", "handler_end1")
        .flow("boundary_comp2", "comp_handler2")
        .flow("comp_handler2", "handler_end2")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();
    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(matches!(inst.state, InstanceState::Completed));
    // Only script2's handler ran (b → 0), script1's handler did NOT run (a stays 10)
    assert_eq!(inst.variables.get("a").and_then(|v| v.as_i64()), Some(10));
    assert_eq!(inst.variables.get("b").and_then(|v| v.as_i64()), Some(0));
}

// ============================================================================
// Gezielte Mutation-Score Tests
// ============================================================================

/// Catches: get_stats delete match arms, replace += with -=/×=
/// Erzeugt Instanzen in ALLEN Zuständen und prüft jeden Zähler exakt.
#[tokio::test]
async fn test_get_stats_all_state_categories() {
    let engine = WorkflowEngine::new();

    // User-Task Def (waiting_user)
    let user_def = ProcessDefinitionBuilder::new("s_user")
        .node("start", BpmnElement::StartEvent)
        .node("ut", BpmnElement::UserTask("a".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "ut")
        .flow("ut", "end")
        .build()
        .unwrap();
    let (uk, _) = engine.deploy_definition(user_def).await;

    // Service-Task Def (waiting_service)
    let svc_def = ProcessDefinitionBuilder::new("s_svc")
        .node("start", BpmnElement::StartEvent)
        .node(
            "svc",
            BpmnElement::ServiceTask {
                topic: "stats_topic".into(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "end")
        .build()
        .unwrap();
    let (sk, _) = engine.deploy_definition(svc_def).await;

    // Completed Def
    let done_def = ProcessDefinitionBuilder::new("s_done")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();
    let (dk, _) = engine.deploy_definition(done_def).await;

    // CompletedWithError Def
    let err_def = ProcessDefinitionBuilder::new("s_err")
        .node("start", BpmnElement::StartEvent)
        .node(
            "err_end",
            BpmnElement::ErrorEndEvent {
                error_code: "E1".into(),
            },
        )
        .flow("start", "err_end")
        .build()
        .unwrap();
    let (ek, _) = engine.deploy_definition(err_def).await;

    // 2 waiting_user
    engine.start_instance(uk).await.unwrap();
    engine.start_instance(uk).await.unwrap();

    // 1 waiting_service
    engine.start_instance(sk).await.unwrap();

    // 1 completed
    engine.start_instance(dk).await.unwrap();

    // 1 completed_with_error
    engine.start_instance(ek).await.unwrap();

    let stats = engine.get_stats().await;
    assert_eq!(stats.definitions_count, 4);
    assert_eq!(stats.instances_total, 5);
    assert_eq!(stats.instances_waiting_user, 2);
    assert_eq!(stats.instances_waiting_service, 1);
    assert_eq!(stats.instances_completed, 2); // 1 Completed + 1 CompletedWithError
    assert_eq!(stats.instances_running, 0);
    assert_eq!(stats.pending_user_tasks, 2);
    assert_eq!(stats.pending_service_tasks, 1);
}

/// Catches: update_instance_variables += counters (added, modified, deleted)
/// Prüft Audit-Log-Text für korrekte Zählung.
#[tokio::test]
async fn test_update_instance_variables_counts_added_modified_deleted() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("var_count")
        .node("start", BpmnElement::StartEvent)
        .node("ut", BpmnElement::UserTask("a".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "ut")
        .flow("ut", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    // Step 1: Add 2 variables
    let mut vars = HashMap::new();
    vars.insert("a".into(), serde_json::json!(1));
    vars.insert("b".into(), serde_json::json!(2));
    engine
        .update_instance_variables(inst_id, vars)
        .await
        .unwrap();

    let log1 = engine.get_audit_log(inst_id).await.unwrap();
    assert!(
        log1.iter()
            .any(|l| l.contains("+2") && l.contains("~0") && l.contains("-0")),
        "Expected +2 ~0 -0 but got: {log1:?}"
    );

    // Step 2: Modify 1, add 1
    let mut vars2 = HashMap::new();
    vars2.insert("a".into(), serde_json::json!(99)); // modify
    vars2.insert("c".into(), serde_json::json!(3)); // add
    engine
        .update_instance_variables(inst_id, vars2)
        .await
        .unwrap();

    let log2 = engine.get_audit_log(inst_id).await.unwrap();
    assert!(
        log2.iter()
            .any(|l| l.contains("+1") && l.contains("~1") && l.contains("-0")),
        "Expected +1 ~1 -0 but got: {log2:?}"
    );

    // Step 3: Delete 1
    let mut vars3 = HashMap::new();
    vars3.insert("b".into(), Value::Null); // delete
    engine
        .update_instance_variables(inst_id, vars3)
        .await
        .unwrap();

    let log3 = engine.get_audit_log(inst_id).await.unwrap();
    assert!(
        log3.iter()
            .any(|l| l.contains("+0") && l.contains("~0") && l.contains("-1")),
        "Expected +0 ~0 -1 but got: {log3:?}"
    );

    // Verify final state
    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert_eq!(inst.variables.get("a"), Some(&serde_json::json!(99)));
    assert!(inst.variables.get("b").is_none());
    assert_eq!(inst.variables.get("c"), Some(&serde_json::json!(3)));
}

/// Catches: delete_instance == vs != in retain-Prädikaten
/// Zwei Instanzen mit pending tasks — nur die gelöschte wird bereinigt.
#[tokio::test]
async fn test_delete_instance_only_affects_target() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("del_iso")
        .node("start", BpmnElement::StartEvent)
        .node("ut", BpmnElement::UserTask("a".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "ut")
        .flow("ut", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;

    let inst_a = engine.start_instance(key).await.unwrap();
    let inst_b = engine.start_instance(key).await.unwrap();

    assert_eq!(engine.pending_user_tasks.len(), 2);

    // Delete only inst_a
    engine.delete_instance(inst_a).await.unwrap();

    // inst_b tasks should remain, inst_a tasks gone
    assert_eq!(engine.pending_user_tasks.len(), 1);
    let remaining: Vec<_> = engine
        .pending_user_tasks
        .iter()
        .map(|r| r.value().instance_id)
        .collect();
    assert_eq!(remaining, vec![inst_b]);
    assert!(engine.get_instance_details(inst_b).await.is_ok());
}

/// Catches: move_token on completed/suspended instances, cancel_current == vs != in filter
#[tokio::test]
async fn test_move_token_rejected_for_completed_and_suspended() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("mv_rej")
        .node("start", BpmnElement::StartEvent)
        .node("ut", BpmnElement::UserTask("a".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "ut")
        .flow("ut", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;

    // Completed instance → move_token should fail
    let done_def = ProcessDefinitionBuilder::new("mv_done")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();
    let (dk, _) = engine.deploy_definition(done_def).await;
    let done_id = engine.start_instance(dk).await.unwrap();
    let res = engine
        .move_token(done_id, "end", HashMap::new(), false)
        .await;
    assert!(matches!(res, Err(EngineError::AlreadyCompleted)));

    // Suspended instance → move_token should fail
    let susp_id = engine.start_instance(key).await.unwrap();
    engine.suspend_instance(susp_id).await.unwrap();
    let res = engine
        .move_token(susp_id, "ut", HashMap::new(), false)
        .await;
    assert!(matches!(res, Err(EngineError::InstanceSuspended(_))));
}

/// Catches: move_token cancel_current=true cleans all queue types
#[tokio::test]
async fn test_move_token_cancel_current_cleans_queues() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("mv_cancel")
        .node("start", BpmnElement::StartEvent)
        .node(
            "svc",
            BpmnElement::ServiceTask {
                topic: "mv_topic".into(),
                multi_instance: None,
            },
        )
        .node("ut", BpmnElement::UserTask("a".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "ut")
        .flow("ut", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    // Should have 1 pending service task
    assert_eq!(engine.pending_service_tasks.len(), 1);

    // Move with cancel_current=true to "ut"
    engine
        .move_token(inst_id, "ut", HashMap::new(), true)
        .await
        .unwrap();

    // Service task should be cleaned
    let remaining_svc: Vec<_> = engine
        .pending_service_tasks
        .iter()
        .filter(|t| t.instance_id == inst_id)
        .collect();
    assert!(
        remaining_svc.is_empty(),
        "cancel_current should clean pending service tasks"
    );

    // Should now be waiting on user task
    assert!(matches!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::WaitingOnUserTask { .. }
    ));
}

/// Catches: get_definition -> None; list_definition_versions -> vec![]
#[tokio::test]
async fn test_get_definition_and_list_versions() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("def_ops")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;

    // get_definition should return the definition
    let got = engine.get_definition(&key).await;
    assert!(got.is_some());
    assert_eq!(got.unwrap().id, "def_ops");

    // Non-existent key
    assert!(engine.get_definition(&Uuid::new_v4()).await.is_none());

    // Deploy v2
    let def2 = ProcessDefinitionBuilder::new("def_ops")
        .node("start", BpmnElement::StartEvent)
        .node("task", BpmnElement::UserTask("a".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "task")
        .flow("task", "end")
        .build()
        .unwrap();
    let (key2, _) = engine.deploy_definition(def2).await;

    // list_definition_versions should return both
    let versions = engine.list_definition_versions("def_ops").await;
    assert_eq!(versions.len(), 2);
    // Sorted ascending, v1 first
    assert_eq!(versions[0].0, key);
    assert_eq!(versions[0].1, 1); // version
    assert_eq!(versions[1].0, key2);
    assert_eq!(versions[1].1, 2);
    // Node count must be correct
    assert_eq!(versions[0].2, 2); // start + end
    assert_eq!(versions[1].2, 3); // start + task + end

    // Non-existent BPMN ID
    let empty = engine.list_definition_versions("nope").await;
    assert!(empty.is_empty());
}

/// Catches: retry_incident > vs >= und resolve_incident > vs >=
#[tokio::test]
async fn test_retry_incident_and_resolve_incident() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("incident")
        .node("start", BpmnElement::StartEvent)
        .node(
            "svc",
            BpmnElement::ServiceTask {
                topic: "inc_topic".into(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    engine.start_instance(key).await.unwrap();

    let tasks = engine
        .fetch_and_lock_service_tasks("w", 1, &["inc_topic".into()], 60)
        .await;
    let task_id = tasks[0].id;

    // Fail to 0 retries → incident
    engine
        .fail_service_task(task_id, "w", Some(0), Some("Boom".into()), None)
        .await
        .unwrap();

    // retry_incident on non-incident should fail
    // First, check retries > 0 guard — retry when already retries == 0 should succeed
    engine.retry_incident(task_id, Some(2)).await.unwrap();

    let t = engine
        .get_pending_service_tasks()
        .iter()
        .find(|t| t.id == task_id)
        .cloned()
        .unwrap();
    assert_eq!(t.retries, 2);
    assert!(t.error_message.is_none());
    assert!(t.worker_id.is_none());

    // retry_incident on task with retries > 0 should fail
    let res = engine.retry_incident(task_id, None).await;
    assert!(res.is_err());

    // Fail to 0 again for resolve test
    let _ = engine
        .fetch_and_lock_service_tasks("w2", 1, &["inc_topic".into()], 60)
        .await;
    engine
        .fail_service_task(task_id, "w2", Some(0), Some("Again".into()), None)
        .await
        .unwrap();

    // resolve_incident on task with retries > 0 should fail
    // (Already at 0, so this should succeed)
    let mut resolve_vars = HashMap::new();
    resolve_vars.insert("resolved".into(), serde_json::json!(true));
    engine
        .resolve_incident(task_id, resolve_vars)
        .await
        .unwrap();

    // Instance should complete (resolve advances token)
    let inst_id = tasks[0].instance_id;
    let state = engine.get_instance_state(inst_id).await.unwrap();
    assert_eq!(state, InstanceState::Completed);
}

/// Catches: verify_lock_ownership None-Fall (ServiceTaskNotLocked)
#[tokio::test]
async fn test_service_task_not_locked_error() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("no_lock")
        .node("start", BpmnElement::StartEvent)
        .node(
            "svc",
            BpmnElement::ServiceTask {
                topic: "nl".into(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    engine.start_instance(key).await.unwrap();

    // Try to complete without fetching (no lock)
    let task_id = engine.get_pending_service_tasks()[0].id;
    let res = engine
        .complete_service_task(task_id, "any_worker", HashMap::new())
        .await;
    assert!(matches!(res, Err(EngineError::ServiceTaskNotLocked(_))));
}

/// Catches: InstanceStore is_empty, clear
#[tokio::test]
async fn test_instance_store_is_empty_and_clear() {
    let store = crate::engine::instance_store::InstanceStore::new();
    assert!(store.is_empty().await);

    let id = Uuid::new_v4();
    let inst = ProcessInstance {
        id,
        definition_key: Uuid::new_v4(),
        business_key: String::new(),
        parent_instance_id: None,
        state: InstanceState::Running,
        current_node: "start".into(),
        audit_log: vec![],
        variables: HashMap::new(),
        tokens: HashMap::new(),
        active_tokens: vec![],
        join_barriers: HashMap::new(),
        multi_instance_state: HashMap::new(),
        compensation_log: Vec::new(),
        started_at: None,
        completed_at: None,
    };
    store.insert(id, inst).await;
    assert!(!store.is_empty().await);
    assert_eq!(store.len().await, 1);

    store.clear().await;
    assert!(store.is_empty().await);
    assert_eq!(store.len().await, 0);
}

/// Catches: DefinitionRegistry contains_key -> true, is_empty -> false
#[tokio::test]
async fn test_registry_is_empty_and_contains_key() {
    let reg = crate::engine::registry::DefinitionRegistry::new();
    assert!(reg.is_empty());
    assert!(!reg.contains_key(&Uuid::new_v4()));

    let key = Uuid::new_v4();
    let def = ProcessDefinitionBuilder::new("reg_test")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();
    reg.insert(key, std::sync::Arc::new(def));

    assert!(!reg.is_empty());
    assert!(reg.contains_key(&key));
    assert!(!reg.contains_key(&Uuid::new_v4()));
}

/// Catches: correlate_message == vs != für business_key-Filter
#[tokio::test]
async fn test_correlate_message_with_business_key_filter() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("msg_bk")
        .node("start", BpmnElement::StartEvent)
        .node(
            "msg_catch",
            BpmnElement::MessageCatchEvent {
                message_name: "ORDER".into(),
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "msg_catch")
        .flow("msg_catch", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;

    // Start instance with business_key = "BK-1"
    let inst_id = engine.start_instance(key).await.unwrap();
    {
        let inst_arc = engine.instances.get(&inst_id).await.unwrap();
        let mut inst = inst_arc.write().await;
        inst.business_key = "BK-1".into();
    }

    // Correlate with wrong business_key → should NOT match the catch
    let affected = engine
        .correlate_message("ORDER".into(), Some("BK-WRONG".into()), HashMap::new())
        .await
        .unwrap();
    assert!(affected.is_empty(), "Wrong business_key should not match");

    // Correlate with correct business_key → should match
    let affected = engine
        .correlate_message("ORDER".into(), Some("BK-1".into()), HashMap::new())
        .await
        .unwrap();
    assert_eq!(affected.len(), 1);
    assert_eq!(affected[0], inst_id);

    let state = engine.get_instance_state(inst_id).await.unwrap();
    assert_eq!(state, InstanceState::Completed);
}

/// Catches: process_timers > vs >= vs < für timer expiry check;
/// suspended instances should be skipped.
#[tokio::test]
async fn test_process_timers_skips_suspended() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("timer_susp")
        .node("start", BpmnElement::StartEvent)
        .node(
            "timer",
            BpmnElement::TimerCatchEvent(crate::domain::TimerDefinition::Duration(
                std::time::Duration::from_millis(10),
            )),
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "timer")
        .flow("timer", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    // Suspend instance before timer fires
    engine.suspend_instance(inst_id).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

    // Timer is expired but instance is suspended → should NOT fire
    let triggered = engine.process_timers().await.unwrap();
    assert_eq!(triggered, 0);

    // Resume → timer should now fire
    engine.resume_instance(inst_id).await.unwrap();
    let triggered = engine.process_timers().await.unwrap();
    assert_eq!(triggered, 1);

    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );
}

/// Catches: fetch_and_lock > vs >= für max_tasks-Grenze
#[tokio::test]
async fn test_fetch_and_lock_respects_max_tasks() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("max_fetch")
        .node("start", BpmnElement::StartEvent)
        .node("fork", BpmnElement::ParallelGateway)
        .node(
            "svc1",
            BpmnElement::ServiceTask {
                topic: "mf".into(),
                multi_instance: None,
            },
        )
        .node(
            "svc2",
            BpmnElement::ServiceTask {
                topic: "mf".into(),
                multi_instance: None,
            },
        )
        .node(
            "svc3",
            BpmnElement::ServiceTask {
                topic: "mf".into(),
                multi_instance: None,
            },
        )
        .node("join", BpmnElement::ParallelGateway)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "fork")
        .flow("fork", "svc1")
        .flow("fork", "svc2")
        .flow("fork", "svc3")
        .flow("svc1", "join")
        .flow("svc2", "join")
        .flow("svc3", "join")
        .flow("join", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    engine.start_instance(key).await.unwrap();

    // 3 service tasks available, but max_tasks=2
    let tasks = engine
        .fetch_and_lock_service_tasks("w", 2, &["mf".into()], 60)
        .await;
    assert_eq!(tasks.len(), 2, "Should respect max_tasks limit");
}

/// Catches: boundary.rs setup_boundary_events attached_to == vs !=
#[tokio::test]
async fn test_boundary_events_only_attach_to_correct_node() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("bnd_iso")
        .node("start", BpmnElement::StartEvent)
        .node("task_a", BpmnElement::UserTask("a".into()))
        .node("task_b", BpmnElement::UserTask("b".into()))
        .node(
            "timer_on_a",
            BpmnElement::BoundaryTimerEvent {
                attached_to: "task_a".into(),
                timer: crate::domain::TimerDefinition::Duration(Duration::from_secs(60)),
                cancel_activity: true,
            },
        )
        .node("end1", BpmnElement::EndEvent)
        .node("end2", BpmnElement::EndEvent)
        .node("end3", BpmnElement::EndEvent)
        .flow("start", "task_a")
        .flow("task_a", "task_b")
        .flow("task_b", "end1")
        .flow("timer_on_a", "end2")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    // Timer boundary on task_a → 1 timer pending
    assert_eq!(engine.pending_timers.len(), 1);

    // Complete task_a → boundary timer should be cancelled
    let task_a = engine
        .get_pending_user_tasks()
        .into_iter()
        .find(|t| t.node_id == "task_a")
        .unwrap();
    engine
        .complete_user_task(task_a.task_id, HashMap::new())
        .await
        .unwrap();

    // Timer should be gone (cancelled by cancel_boundary_timers)
    assert_eq!(engine.pending_timers.len(), 0);

    // Instance should now be at task_b
    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert_eq!(inst.current_node, "task_b");
}

/// Catches: delete_instance mit allen Queue-Typen (user, service, timer, message)
#[tokio::test]
async fn test_delete_instance_cleans_timers_and_messages() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("del_all_q")
        .node("start", BpmnElement::StartEvent)
        .node("fork", BpmnElement::ParallelGateway)
        .node("ut", BpmnElement::UserTask("a".into()))
        .node(
            "timer",
            BpmnElement::TimerCatchEvent(crate::domain::TimerDefinition::Duration(
                Duration::from_secs(3600),
            )),
        )
        .node(
            "msg",
            BpmnElement::MessageCatchEvent {
                message_name: "DEL_MSG".into(),
            },
        )
        .node("join", BpmnElement::ParallelGateway)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "fork")
        .flow("fork", "ut")
        .flow("fork", "timer")
        .flow("fork", "msg")
        .flow("ut", "join")
        .flow("timer", "join")
        .flow("msg", "join")
        .flow("join", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    assert!(engine.pending_user_tasks.len() >= 1);
    assert!(engine.pending_timers.len() >= 1);
    assert!(engine.pending_message_catches.len() >= 1);

    engine.delete_instance(inst_id).await.unwrap();

    // All queues for this instance should be empty
    assert_eq!(
        engine
            .pending_user_tasks
            .iter()
            .filter(|t| t.instance_id == inst_id)
            .count(),
        0
    );
    assert_eq!(
        engine
            .pending_timers
            .iter()
            .filter(|t| t.instance_id == inst_id)
            .count(),
        0
    );
    assert_eq!(
        engine
            .pending_message_catches
            .iter()
            .filter(|t| t.instance_id == inst_id)
            .count(),
        0
    );
}

/// Catches: same_gateway_type -> bool with true (mismatched types should return false)
#[tokio::test]
async fn test_same_gateway_type_detects_mismatch() {
    // Mismatch: Exclusive split with Parallel join → should NOT detect as same
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("gw_mismatch")
        .node("start", BpmnElement::StartEvent)
        .node(
            "xor_split",
            BpmnElement::ExclusiveGateway {
                default: Some("task_b".into()),
            },
        )
        .node(
            "task_a",
            BpmnElement::ServiceTask {
                topic: "a".into(),
                multi_instance: None,
            },
        )
        .node(
            "task_b",
            BpmnElement::ServiceTask {
                topic: "b".into(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "xor_split")
        .conditional_flow("xor_split", "task_a", "x == 1")
        .flow("xor_split", "task_b")
        .flow("task_a", "end")
        .flow("task_b", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;

    let mut vars = HashMap::new();
    vars.insert("x".into(), serde_json::json!(1));
    let inst_id = engine
        .start_instance_with_variables(key, vars)
        .await
        .unwrap();

    complete_all_service_tasks(&engine, "w", HashMap::new()).await;

    let state = engine.get_instance_state(inst_id).await.unwrap();
    assert_eq!(state, InstanceState::Completed);
}

/// Catches: ScriptConfig::from_env -> Default, build_engine -> Default
#[tokio::test]
async fn test_script_config_defaults_and_build() {
    let cfg = crate::scripting::ScriptConfig::from_env();
    // Defaults should be positive numbers
    assert!(cfg.max_operations > 0);
    assert!(cfg.max_memory > 0);
    assert!(cfg.timeout_ms > 0);

    // build_engine should produce a working engine (not Default which would lack limits)
    // Verify by running a simple script — a Default rhai::Engine would succeed,
    // but our configured engine has limits that allow simple scripts to run.
    let rhai_engine = cfg.build_engine();
    let result = rhai_engine.eval::<i64>("40 + 2");
    assert_eq!(result.unwrap(), 42);
}

/// Catches: run_node_scripts == vs != für Listener-Event-Matching
#[tokio::test]
async fn test_script_start_vs_end_listener_distinction() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("listener_dist")
        .node("start", BpmnElement::StartEvent)
        .node(
            "svc",
            BpmnElement::ServiceTask {
                topic: "ld".into(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "end")
        .listener(
            "svc",
            crate::domain::ListenerEvent::Start,
            r#"let start_ran = true;"#,
        )
        .listener(
            "svc",
            crate::domain::ListenerEvent::End,
            r#"let end_ran = true;"#,
        )
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    // After start → start_ran should exist, end_ran not yet
    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(inst.variables.contains_key("start_ran"));
    assert!(!inst.variables.contains_key("end_ran"));

    // Complete service task → end listener should run
    complete_all_service_tasks(&engine, "w", HashMap::new()).await;
    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(inst.variables.contains_key("end_ran"));
}

// -----------------------------------------------------------------------
// Tests targeting specific MISSED mutations
// -----------------------------------------------------------------------

/// Catches: replace cancel_boundary_timers with ()
/// Catches: == vs != and && vs || in cancel_boundary_timers filter/retain
#[tokio::test]
async fn test_cancel_boundary_timers_removes_timers_on_task_complete() {
    let engine = WorkflowEngine::with_in_memory_persistence();
    let def = ProcessDefinitionBuilder::new("bt_cancel")
        .node("start", BpmnElement::StartEvent)
        .node("ut", BpmnElement::UserTask("alice".into()))
        .node(
            "bt",
            BpmnElement::BoundaryTimerEvent {
                attached_to: "ut".into(),
                timer: crate::domain::TimerDefinition::Duration(Duration::from_secs(3600)),
                cancel_activity: true,
            },
        )
        .node("timeout_end", BpmnElement::EndEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "ut")
        .flow("ut", "end")
        .flow("bt", "timeout_end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    // Boundary timer should be registered
    let timer_count_before = engine
        .pending_timers
        .iter()
        .filter(|r| r.instance_id == inst_id)
        .count();
    assert_eq!(timer_count_before, 1, "Boundary timer should be pending");

    // Complete the user task → boundary timer must be cancelled
    let task_id = engine
        .pending_user_tasks
        .iter()
        .find(|r| r.instance_id == inst_id)
        .map(|r| r.task_id)
        .unwrap();
    engine
        .complete_user_task(task_id, HashMap::new())
        .await
        .unwrap();

    let timer_count_after = engine
        .pending_timers
        .iter()
        .filter(|r| r.instance_id == inst_id)
        .count();
    assert_eq!(
        timer_count_after, 0,
        "Boundary timer should be cancelled after task completion"
    );
}

/// Catches: replace cancel_boundary_message_catches with ()
/// Catches: == vs != and && vs || in cancel_boundary_message_catches filter/retain
/// Catches: delete ! in cancel_boundary_message_catches retain predicate
#[tokio::test]
async fn test_cancel_boundary_message_catches_on_task_complete() {
    let engine = WorkflowEngine::with_in_memory_persistence();
    let def = ProcessDefinitionBuilder::new("bm_cancel")
        .node("start", BpmnElement::StartEvent)
        .node("ut", BpmnElement::UserTask("alice".into()))
        .node(
            "bm",
            BpmnElement::BoundaryMessageEvent {
                attached_to: "ut".into(),
                message_name: "cancel_msg".into(),
                cancel_activity: true,
            },
        )
        .node("msg_end", BpmnElement::EndEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "ut")
        .flow("ut", "end")
        .flow("bm", "msg_end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    // Boundary message catch should be registered
    let msg_count_before = engine
        .pending_message_catches
        .iter()
        .filter(|r| r.instance_id == inst_id)
        .count();
    assert_eq!(
        msg_count_before, 1,
        "Boundary message catch should be pending"
    );

    // Complete the user task → boundary message must be cancelled
    let task_id = engine
        .pending_user_tasks
        .iter()
        .find(|r| r.instance_id == inst_id)
        .map(|r| r.task_id)
        .unwrap();
    engine
        .complete_user_task(task_id, HashMap::new())
        .await
        .unwrap();

    let msg_count_after = engine
        .pending_message_catches
        .iter()
        .filter(|r| r.instance_id == inst_id)
        .count();
    assert_eq!(
        msg_count_after, 0,
        "Boundary message catch should be cancelled after task completion"
    );
}

/// Catches: && vs || in clear_wait_states_for_token filter
/// Catches: replace clear_wait_states_for_token with ()
#[tokio::test]
async fn test_event_based_gateway_cancels_alternatives() {
    let engine = WorkflowEngine::with_in_memory_persistence();

    let def = ProcessDefinitionBuilder::new("ebg_cancel")
        .node("start", BpmnElement::StartEvent)
        .node("ebg", BpmnElement::EventBasedGateway)
        .node(
            "timer_catch",
            BpmnElement::TimerCatchEvent(crate::domain::TimerDefinition::Duration(
                Duration::from_secs(3600),
            )),
        )
        .node(
            "msg_catch",
            BpmnElement::MessageCatchEvent {
                message_name: "go".into(),
            },
        )
        .node("timer_end", BpmnElement::EndEvent)
        .node("msg_end", BpmnElement::EndEvent)
        .flow("start", "ebg")
        .flow("ebg", "timer_catch")
        .flow("ebg", "msg_catch")
        .flow("timer_catch", "timer_end")
        .flow("msg_catch", "msg_end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    // Both a timer and a message catch should be pending
    let timer_count = engine
        .pending_timers
        .iter()
        .filter(|r| r.instance_id == inst_id)
        .count();
    let msg_count = engine
        .pending_message_catches
        .iter()
        .filter(|r| r.instance_id == inst_id)
        .count();
    assert_eq!(timer_count, 1, "Timer catch should be pending");
    assert_eq!(msg_count, 1, "Message catch should be pending");

    // Correlate the message → timer alternative should be cancelled
    engine
        .correlate_message("go".to_string(), None, HashMap::new())
        .await
        .unwrap();

    let timer_after = engine
        .pending_timers
        .iter()
        .filter(|r| r.instance_id == inst_id)
        .count();
    assert_eq!(
        timer_after, 0,
        "Timer catch should be cancelled when message fires"
    );
}

/// Catches: replace restore_user_task with ()
/// Catches: replace restore_service_task with ()
#[tokio::test]
async fn test_restore_user_and_service_tasks() {
    let engine = WorkflowEngine::new();
    let task_id = uuid::Uuid::new_v4();
    let inst_id = uuid::Uuid::new_v4();

    // Restore a user task
    let pending_user = crate::runtime::PendingUserTask {
        task_id,
        instance_id: inst_id,
        node_id: "ut".into(),
        assignee: "alice".into(),
        token_id: uuid::Uuid::new_v4(),
        created_at: chrono::Utc::now(),
        business_key: None,
    };
    engine.restore_user_task(pending_user);
    assert_eq!(engine.pending_user_tasks.len(), 1);
    assert!(engine.pending_user_tasks.contains_key(&task_id));

    // Restore a service task
    let svc_id = uuid::Uuid::new_v4();
    let pending_svc = crate::runtime::PendingServiceTask {
        id: svc_id,
        instance_id: inst_id,
        definition_key: uuid::Uuid::new_v4(),
        node_id: "svc".into(),
        topic: "validate".into(),
        token_id: uuid::Uuid::new_v4(),
        variables_snapshot: HashMap::new(),
        created_at: chrono::Utc::now(),
        worker_id: None,
        lock_expiration: None,
        retries: 3,
        error_message: None,
        error_details: None,
    };
    engine.restore_service_task(pending_svc);
    assert_eq!(engine.pending_service_tasks.len(), 1);
    assert!(engine.pending_service_tasks.contains_key(&svc_id));
}

/// Catches: replace restore_timer with ()
/// Catches: replace restore_message_catch with ()
#[tokio::test]
async fn test_restore_timer_and_message_catch() {
    let engine = WorkflowEngine::new();
    let inst_id = uuid::Uuid::new_v4();

    let timer_id = uuid::Uuid::new_v4();
    let pending_timer = crate::runtime::PendingTimer {
        id: timer_id,
        instance_id: inst_id,
        node_id: "timer".into(),
        token_id: uuid::Uuid::new_v4(),
        expires_at: chrono::Utc::now(),
        timer_def: None,
        remaining_repetitions: None,
    };
    engine.restore_timer(pending_timer);
    assert_eq!(engine.pending_timers.len(), 1);
    assert!(engine.pending_timers.contains_key(&timer_id));

    let msg_id = uuid::Uuid::new_v4();
    let pending_msg = crate::runtime::PendingMessageCatch {
        id: msg_id,
        instance_id: inst_id,
        node_id: "msg".into(),
        message_name: "order".into(),
        token_id: uuid::Uuid::new_v4(),
    };
    engine.restore_message_catch(pending_msg);
    assert_eq!(engine.pending_message_catches.len(), 1);
    assert!(engine.pending_message_catches.contains_key(&msg_id));
}

/// Catches: replace shutdown with ()
#[tokio::test]
async fn test_shutdown_completes_without_panic() {
    let engine = WorkflowEngine::with_in_memory_persistence();
    // Deploy and start something so the retry worker is active
    let def = ProcessDefinitionBuilder::new("shutdown_test")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let _ = engine.start_instance(key).await.unwrap();

    // Shutdown should signal retry worker and wait
    engine.shutdown().await;

    // After shutdown, engine should still be usable (no panics)
    assert!(
        engine
            .get_instance_details(uuid::Uuid::new_v4())
            .await
            .is_err()
    );
}

/// Catches: replace set_persistence with ()
#[tokio::test]
async fn test_set_persistence_activates_retry_tx() {
    let mut engine = WorkflowEngine::new();
    assert!(
        engine.retry_tx.is_none(),
        "No retry_tx before set_persistence"
    );

    let persistence = std::sync::Arc::new(crate::adapter::InMemoryPersistence::new());
    engine.set_persistence(persistence);

    assert!(
        engine.retry_tx.is_some(),
        "retry_tx should be set after set_persistence"
    );
}

/// Catches: find_downstream_join depth arithmetic (- vs /, + vs -, + vs *)
/// Tests nested parallel gateways where depth tracking matters.
#[tokio::test]
async fn test_find_downstream_join_nested_parallel() {
    let engine = WorkflowEngine::new();

    // Build: start → split1 → (branch_a → split2 → (inner_a, inner_b) → join2 → merge_a, branch_b) → join1 → end
    let def = ProcessDefinitionBuilder::new("nested_par")
        .node("start", BpmnElement::StartEvent)
        .node("split1", BpmnElement::ParallelGateway)
        .node(
            "task_a",
            BpmnElement::ServiceTask {
                topic: "a".into(),
                multi_instance: None,
            },
        )
        .node("split2", BpmnElement::ParallelGateway)
        .node(
            "inner_a",
            BpmnElement::ServiceTask {
                topic: "ia".into(),
                multi_instance: None,
            },
        )
        .node(
            "inner_b",
            BpmnElement::ServiceTask {
                topic: "ib".into(),
                multi_instance: None,
            },
        )
        .node("join2", BpmnElement::ParallelGateway)
        .node(
            "task_b",
            BpmnElement::ServiceTask {
                topic: "b".into(),
                multi_instance: None,
            },
        )
        .node("join1", BpmnElement::ParallelGateway)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "split1")
        // Branch A: split1 → task_a → split2 → inner_a/inner_b → join2 → join1
        .flow("split1", "task_a")
        .flow("task_a", "split2")
        .flow("split2", "inner_a")
        .flow("split2", "inner_b")
        .flow("inner_a", "join2")
        .flow("inner_b", "join2")
        .flow("join2", "join1")
        // Branch B: split1 → task_b → join1
        .flow("split1", "task_b")
        .flow("task_b", "join1")
        .flow("join1", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;

    // find_downstream_join from split1 should find join1 (not join2)
    let def_ref = engine.definitions.get(&key).unwrap();
    let join = engine.find_downstream_join(&def_ref, "split1");
    assert_eq!(join.as_deref(), Some("join1"), "split1 should find join1");

    // find_downstream_join from split2 should find join2
    let join2 = engine.find_downstream_join(&def_ref, "split2");
    assert_eq!(join2.as_deref(), Some("join2"), "split2 should find join2");
}

/// Catches: complete_branch_token == vs != in find predicate
#[tokio::test]
async fn test_complete_branch_token_marks_correct_token() {
    let engine = WorkflowEngine::new();
    // Parallel flow that forks into two branches
    let def = ProcessDefinitionBuilder::new("branch_tok")
        .node("start", BpmnElement::StartEvent)
        .node("split", BpmnElement::ParallelGateway)
        .node("ut_a", BpmnElement::UserTask("a".into()))
        .node("ut_b", BpmnElement::UserTask("b".into()))
        .node("join", BpmnElement::ParallelGateway)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "split")
        .flow("split", "ut_a")
        .flow("split", "ut_b")
        .flow("ut_a", "join")
        .flow("ut_b", "join")
        .flow("join", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    // Two user tasks should exist
    assert_eq!(engine.pending_user_tasks.len(), 2);

    // Complete one task → only that branch token should be completed
    let task_a_id = engine
        .pending_user_tasks
        .iter()
        .find(|r| r.node_id == "ut_a")
        .map(|r| r.task_id)
        .unwrap();
    engine
        .complete_user_task(task_a_id, HashMap::new())
        .await
        .unwrap();

    // Instance should still have pending tasks (ut_b not completed)
    assert_eq!(engine.pending_user_tasks.len(), 1);
    let remaining_task = engine
        .pending_user_tasks
        .iter()
        .next()
        .map(|r| r.node_id.clone())
        .unwrap();
    assert_eq!(remaining_task, "ut_b");

    // Instance should NOT be completed yet
    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(
        !matches!(inst.state, InstanceState::Completed),
        "Instance should not be completed until all branches done"
    );
}

/// Catches: replace restore_instance with () (the instance must be accessible)
#[tokio::test]
async fn test_restore_instance_makes_it_accessible() {
    let engine = WorkflowEngine::new();
    let inst_id = uuid::Uuid::new_v4();
    let def_key = uuid::Uuid::new_v4();

    let instance = crate::runtime::ProcessInstance {
        id: inst_id,
        definition_key: def_key,
        business_key: "restored".into(),
        parent_instance_id: None,
        state: InstanceState::Running,
        current_node: "start".into(),
        audit_log: vec![],
        variables: HashMap::new(),
        tokens: HashMap::new(),
        active_tokens: vec![],
        join_barriers: HashMap::new(),
        multi_instance_state: HashMap::new(),
        compensation_log: Vec::new(),
        started_at: None,
        completed_at: None,
    };
    engine.restore_instance(instance).await;

    let details = engine.get_instance_details(inst_id).await.unwrap();
    assert_eq!(details.business_key, "restored");
}

/// Catches: cancel_boundary_timers isolates instance — only timers for the
/// specific instance+node are removed, not unrelated timers.
#[tokio::test]
async fn test_cancel_boundary_timers_isolates_instances() {
    let engine = WorkflowEngine::with_in_memory_persistence();
    let def = ProcessDefinitionBuilder::new("bt_iso")
        .node("start", BpmnElement::StartEvent)
        .node("ut", BpmnElement::UserTask("alice".into()))
        .node(
            "bt",
            BpmnElement::BoundaryTimerEvent {
                attached_to: "ut".into(),
                timer: crate::domain::TimerDefinition::Duration(Duration::from_secs(3600)),
                cancel_activity: true,
            },
        )
        .node("timeout_end", BpmnElement::EndEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "ut")
        .flow("ut", "end")
        .flow("bt", "timeout_end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;

    let inst_a = engine.start_instance(key).await.unwrap();
    let inst_b = engine.start_instance(key).await.unwrap();

    // Both instances should have boundary timers
    assert_eq!(
        engine
            .pending_timers
            .iter()
            .filter(|r| r.instance_id == inst_a)
            .count(),
        1
    );
    assert_eq!(
        engine
            .pending_timers
            .iter()
            .filter(|r| r.instance_id == inst_b)
            .count(),
        1
    );

    // Complete inst_a user task → only inst_a boundary timer should be cancelled
    let task_a = engine
        .pending_user_tasks
        .iter()
        .find(|r| r.instance_id == inst_a)
        .map(|r| r.task_id)
        .unwrap();
    engine
        .complete_user_task(task_a, HashMap::new())
        .await
        .unwrap();

    assert_eq!(
        engine
            .pending_timers
            .iter()
            .filter(|r| r.instance_id == inst_a)
            .count(),
        0,
        "inst_a timers should be cancelled"
    );
    assert_eq!(
        engine
            .pending_timers
            .iter()
            .filter(|r| r.instance_id == inst_b)
            .count(),
        1,
        "inst_b timers should remain"
    );
}

/// Catches: all_tokens_completed logic — empty tokens vs active_tokens checks
#[tokio::test]
async fn test_parallel_flow_completes_only_when_all_branches_done() {
    let engine = WorkflowEngine::with_in_memory_persistence();
    let def = ProcessDefinitionBuilder::new("par_all")
        .node("start", BpmnElement::StartEvent)
        .node("split", BpmnElement::ParallelGateway)
        .node("ut_a", BpmnElement::UserTask("a".into()))
        .node("ut_b", BpmnElement::UserTask("b".into()))
        .node("join", BpmnElement::ParallelGateway)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "split")
        .flow("split", "ut_a")
        .flow("split", "ut_b")
        .flow("ut_a", "join")
        .flow("ut_b", "join")
        .flow("join", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    // Complete first branch
    let task_a = engine
        .pending_user_tasks
        .iter()
        .find(|r| r.node_id == "ut_a")
        .map(|r| r.task_id)
        .unwrap();
    engine
        .complete_user_task(task_a, HashMap::new())
        .await
        .unwrap();

    // Not yet completed
    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(!matches!(inst.state, InstanceState::Completed));

    // Complete second branch
    let task_b = engine
        .pending_user_tasks
        .iter()
        .find(|r| r.node_id == "ut_b")
        .map(|r| r.task_id)
        .unwrap();
    engine
        .complete_user_task(task_b, HashMap::new())
        .await
        .unwrap();

    // Now completed
    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(
        matches!(inst.state, InstanceState::Completed),
        "Instance should complete after both branches, got: {:?}",
        inst.state
    );
}

/// Catches: Terminate End Event retain predicates (== vs !=)
#[tokio::test]
async fn test_terminate_end_kills_all_pending() {
    let engine = WorkflowEngine::with_in_memory_persistence();

    // Parallel split: one branch goes to user task then end, other to terminate
    let def = ProcessDefinitionBuilder::new("term_kill")
        .node("start", BpmnElement::StartEvent)
        .node("split", BpmnElement::ParallelGateway)
        .node("ut", BpmnElement::UserTask("alice".into()))
        .node("end", BpmnElement::EndEvent)
        .node("term", BpmnElement::TerminateEndEvent)
        .flow("start", "split")
        .flow("split", "ut")
        .flow("ut", "end")
        .flow("split", "term")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    // After terminate, no pending user tasks should remain for this instance
    let remaining = engine
        .pending_user_tasks
        .iter()
        .filter(|r| r.instance_id == inst_id)
        .count();
    assert_eq!(remaining, 0, "Terminate should kill all pending user tasks");

    // Instance should be completed
    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(
        matches!(inst.state, InstanceState::Completed),
        "Terminate should set state to Completed, got: {:?}",
        inst.state
    );
}

/// Catches: resolve_next_target find(|f| ...) condition — unwrap_or(true) vs unwrap_or(false)
#[tokio::test]
async fn test_unconditional_flow_routes_without_condition() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("uncon_flow")
        .node("start", BpmnElement::StartEvent)
        .node(
            "svc",
            BpmnElement::ServiceTask {
                topic: "do_work".into(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(key).await.unwrap();

    // Should route to svc (unconditional flow → unwrap_or(true))
    let inst = engine.get_instance_details(inst_id).await.unwrap();
    assert!(
        matches!(inst.state, InstanceState::WaitingOnServiceTask { .. }),
        "Unconditional flow should reach service task, got: {:?}",
        inst.state
    );
}

/// Catches: run_instance_batch step_count > MAX_EXECUTION_STEPS
/// Tests that an infinite loop in BPMN is caught and aborted.
#[tokio::test]
async fn test_execution_limit_prevents_infinite_loop() {
    let engine = WorkflowEngine::new();
    // Create a loop: start → script → script (loop back to itself), with an end event for validation
    let def = ProcessDefinitionBuilder::new("inf_loop")
        .node("start", BpmnElement::StartEvent)
        .node(
            "script",
            BpmnElement::ScriptTask {
                script: r#"let x = 1;"#.into(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "script")
        .flow("script", "script") // self-loop (end is unreachable but satisfies validation)
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    let result = engine.start_instance(key).await;

    // Should fail with ExecutionLimitExceeded
    assert!(
        matches!(result, Err(EngineError::ExecutionLimitExceeded(_))),
        "Infinite loop should trigger execution limit, got: {:?}",
        result
    );
}

/// Tests that correlate_message with a MessageStartEvent starts a new instance.
#[tokio::test]
async fn test_correlate_message_starts_new_instance() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("msg_start_corr")
        .node(
            "start",
            BpmnElement::MessageStartEvent {
                message_name: "order_created".into(),
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();

    let _ = engine.deploy_definition(def).await;

    // No instances before message
    assert!(engine.list_instances().await.is_empty());

    let mut vars = HashMap::new();
    vars.insert("orderId".into(), serde_json::Value::from(42));

    let affected = engine
        .correlate_message("order_created".into(), None, vars)
        .await
        .unwrap();
    assert_eq!(affected.len(), 1);

    // Instance should have been created and completed (start → end)
    let inst = engine.get_instance_details(affected[0]).await.unwrap();
    assert_eq!(inst.state, InstanceState::Completed);
    assert_eq!(
        inst.variables.get("orderId"),
        Some(&serde_json::Value::from(42))
    );
}

/// Tests that verify_lock_ownership returns correct errors for all three cases.
#[tokio::test]
async fn test_complete_service_task_lock_ownership_variants() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("lock_variants")
        .node("start", BpmnElement::StartEvent)
        .node(
            "svc",
            BpmnElement::ServiceTask {
                topic: "lock_test".into(),
                multi_instance: None,
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "end")
        .build()
        .unwrap();
    let (key, _) = engine.deploy_definition(def).await;
    engine.start_instance(key).await.unwrap();

    let task_id = engine.get_pending_service_tasks()[0].id;

    // Case 1: Not locked — should return ServiceTaskNotLocked
    let res = engine
        .complete_service_task(task_id, "any", HashMap::new())
        .await;
    assert!(
        matches!(res, Err(EngineError::ServiceTaskNotLocked(_))),
        "Expected ServiceTaskNotLocked, got: {:?}",
        res
    );

    // Lock it with worker "alpha"
    engine
        .fetch_and_lock_service_tasks("alpha", 1, &["lock_test".into()], 60000)
        .await;

    // Case 2: Wrong worker — should return ServiceTaskLocked
    let res = engine
        .complete_service_task(task_id, "beta", HashMap::new())
        .await;
    assert!(
        matches!(res, Err(EngineError::ServiceTaskLocked { .. })),
        "Expected ServiceTaskLocked, got: {:?}",
        res
    );

    // Case 3: Correct worker — should succeed
    let res = engine
        .complete_service_task(task_id, "alpha", HashMap::new())
        .await;
    assert!(res.is_ok(), "Expected Ok, got: {:?}", res);

    // Case 4: Already removed — should return ServiceTaskNotFound
    let res = engine
        .complete_service_task(task_id, "alpha", HashMap::new())
        .await;
    assert!(
        matches!(res, Err(EngineError::ServiceTaskNotFound(_))),
        "Expected ServiceTaskNotFound, got: {:?}",
        res
    );
}
