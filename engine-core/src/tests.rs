//! Unit tests for the workflow engine.
//!
//! Extracted from `engine.rs` to keep the main module focused on production
//! logic.

use super::*;
use crate::model::ProcessDefinitionBuilder;

async fn setup_linear_engine() -> (WorkflowEngine, Uuid) {
    let mut engine = WorkflowEngine::new();

    // Register a simple service handler
    engine.register_service_handler(
        "validate",
        Arc::new(|vars: &mut HashMap<String, Value>| {
            vars.insert("validated".into(), Value::Bool(true));
            Ok(())
        }),
    );

    let def = ProcessDefinitionBuilder::new("linear")
        .node("start", BpmnElement::StartEvent)
        .node("svc", BpmnElement::ServiceTask("validate".into()))
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
    engine.register_service_handler(
        "noop",
        Arc::new(|_vars: &mut HashMap<String, Value>| Ok(())),
    );

    let def = ProcessDefinitionBuilder::new("cond_svc")
        .node("start", BpmnElement::StartEvent)
        .node("svc", BpmnElement::ServiceTask("noop".into()))
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

    let task_id = engine.pending_user_tasks[0].task_id;
    engine
        .complete_user_task(task_id, HashMap::new())
        .await
        .unwrap();

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
async fn missing_handler_gives_error() {
    let mut engine = WorkflowEngine::new();

    let def = ProcessDefinitionBuilder::new("p1")
        .node("start", BpmnElement::StartEvent)
        .node("svc", BpmnElement::ServiceTask("unknown_handler".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "svc")
        .flow("svc", "end")
        .build()
        .unwrap();

    let def_key = engine.deploy_definition(def).await;
    let result = engine.start_instance(def_key).await;
    assert!(matches!(result, Err(EngineError::HandlerNotFound(_))));
}

#[tokio::test]
async fn audit_log_captures_all_steps() {
    let (mut engine, def_key) = setup_linear_engine().await;
    let inst_id = engine.start_instance(def_key).await.unwrap();

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
    engine.register_service_handler(
        "noop",
        Arc::new(|_vars: &mut HashMap<String, Value>| Ok(())),
    );

    // Start → XOR Gateway → (amount > 100 → high) / (default → low) → End
    let def = ProcessDefinitionBuilder::new("xor_test")
        .node("start", BpmnElement::StartEvent)
        .node(
            "gw",
            BpmnElement::ExclusiveGateway {
                default: Some("low".into()),
            },
        )
        .node("high", BpmnElement::ServiceTask("noop".into()))
        .node("low", BpmnElement::ServiceTask("noop".into()))
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
    engine.register_service_handler(
        "noop",
        Arc::new(|_vars: &mut HashMap<String, Value>| Ok(())),
    );

    let def = ProcessDefinitionBuilder::new("xor_default")
        .node("start", BpmnElement::StartEvent)
        .node(
            "gw",
            BpmnElement::ExclusiveGateway {
                default: Some("low".into()),
            },
        )
        .node("high", BpmnElement::ServiceTask("noop".into()))
        .node("low", BpmnElement::ServiceTask("noop".into()))
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
    engine.register_service_handler(
        "track_a",
        Arc::new(|vars: &mut HashMap<String, Value>| {
            vars.insert("path_a".into(), Value::Bool(true));
            Ok(())
        }),
    );
    engine.register_service_handler(
        "track_b",
        Arc::new(|vars: &mut HashMap<String, Value>| {
            vars.insert("path_b".into(), Value::Bool(true));
            Ok(())
        }),
    );

    // Start → Inclusive GW → (a > 0 → svc_a → end) / (b > 0 → svc_b → end)
    let def = ProcessDefinitionBuilder::new("or_test")
        .node("start", BpmnElement::StartEvent)
        .node("gw", BpmnElement::InclusiveGateway)
        .node("svc_a", BpmnElement::ServiceTask("track_a".into()))
        .node("svc_b", BpmnElement::ServiceTask("track_b".into()))
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
    engine.register_service_handler(
        "noop",
        Arc::new(|_vars: &mut HashMap<String, Value>| Ok(())),
    );

    let def = ProcessDefinitionBuilder::new("or_single")
        .node("start", BpmnElement::StartEvent)
        .node("gw", BpmnElement::InclusiveGateway)
        .node("a", BpmnElement::ServiceTask("noop".into()))
        .node("b", BpmnElement::ServiceTask("noop".into()))
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
        .node("svc", BpmnElement::ServiceTask("calculate".into()))
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

    engine.register_service_handler(
        "calculate",
        Arc::new(|_| Ok(())),
    );

    let mut vars = HashMap::new();
    vars.insert("x".into(), serde_json::json!(6));

    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();

    assert_eq!(
        *engine.get_instance_state(inst_id).unwrap(),
        InstanceState::Completed
    );

    let details = engine.get_instance_details(inst_id).unwrap();

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
