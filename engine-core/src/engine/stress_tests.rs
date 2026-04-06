//! Stress and Performance Tests for the Workflow Engine.
//!
//! This module contains rigorous mass-testing logic that validates the NFRs
//! (Non-Functional Requirements) of the workflow engine according to the Massentest-Plan.

use super::*;
use crate::model::ProcessDefinitionBuilder;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Infrastructure Helpers
// ---------------------------------------------------------------------------

/// Builds a strictly linear sequence of service tasks: Start -> Svc1 -> Svc2 -> ... -> End
fn linear_definition(id: &str, n_tasks: usize) -> ProcessDefinition {
    let mut builder = ProcessDefinitionBuilder::new(id).node("start", BpmnElement::StartEvent);

    let mut last_node = "start".to_string();

    for i in 1..=n_tasks {
        let node_id = format!("svc_{i}");
        builder = builder
            .node(
                &node_id,
                BpmnElement::ServiceTask {
                    topic: format!("topic_{i}"),
                },
            )
            .flow(&last_node, &node_id);
        last_node = node_id;
    }

    builder
        .node("end", BpmnElement::EndEvent)
        .flow(&last_node, "end")
        .build()
        .unwrap()
}

/// Builds a parallel gateway that forks into `n_branches` service tasks and joins them again.
fn parallel_definition(id: &str, n_branches: usize) -> ProcessDefinition {
    let mut builder = ProcessDefinitionBuilder::new(id)
        .node("start", BpmnElement::StartEvent)
        .node("fork", BpmnElement::ParallelGateway)
        .node("join", BpmnElement::ParallelGateway)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "fork")
        .flow("join", "end");

    for i in 1..=n_branches {
        let node_id = format!("branch_{i}");
        builder = builder
            .node(
                &node_id,
                BpmnElement::ServiceTask {
                    topic: "parallel_task".to_string(),
                },
            )
            .flow("fork", &node_id)
            .flow(&node_id, "join");
    }

    builder.build().unwrap()
}

/// Completes all currently pending service tasks by fetching and executing them repeatedly until none are left.
async fn drain_service_tasks(engine: &WorkflowEngine, worker: &str) {
    loop {
        let topics: Vec<String> = engine
            .get_pending_service_tasks()
            .iter()
            .map(|t| t.topic.clone())
            .collect();
        if topics.is_empty() {
            break;
        }

        let tasks = engine
            .fetch_and_lock_service_tasks(worker, 1000, &topics, 30_000)
            .await;
        if tasks.is_empty() {
            break;
        }
        for task in tasks {
            engine
                .complete_service_task(task.id, worker, std::collections::HashMap::new())
                .await
                .unwrap();
        }
    }
}
// ---------------------------------------------------------------------------
// Category A: Throughput & Latenz
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_throughput_1000_linear_instances() {
    let engine = WorkflowEngine::new();
    let def = linear_definition("linear_1", 1);
    let (def_key, _) = engine.deploy_definition(def).await;

    let start_time = Instant::now();
    let mut instances = Vec::with_capacity(1000);

    for _ in 0..1000 {
        instances.push(engine.start_instance(def_key).await.unwrap());
    }

    drain_service_tasks(&engine, "worker_1").await;

    let elapsed = start_time.elapsed();
    assert!(elapsed.as_secs() < 15, "Throughput too slow: {:?}", elapsed);

    let stats = engine.get_stats().await;
    assert_eq!(stats.instances_completed, 1000);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_concurrent_fetch_and_lock() {
    let engine = WorkflowEngine::new();
    let def = parallel_definition("par_50", 50);
    let (def_key, _) = engine.deploy_definition(def).await;

    engine.start_instance(def_key).await.unwrap();
    assert_eq!(engine.get_stats().await.pending_service_tasks, 50);

    let engine_arc = std::sync::Arc::new(engine);
    let mut handles = Vec::new();

    for i in 0..20 {
        let engine_clone = engine_arc.clone();
        handles.push(tokio::spawn(async move {
            let eng = engine_clone;
            eng.fetch_and_lock_service_tasks(
                &format!("worker_{}", i),
                5,
                &["parallel_task".to_string()],
                60_000,
            )
            .await
        }));
    }

    let mut total_locked = 0;
    for handle in handles {
        let tasks = handle.await.unwrap();
        total_locked += tasks.len();
    }

    assert_eq!(total_locked, 50);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_parallel_gateway_100_branches() {
    let engine = WorkflowEngine::new();
    let def = parallel_definition("par_100", 100);
    let (def_key, _) = engine.deploy_definition(def).await;

    let start_time = Instant::now();
    let inst_id = engine.start_instance(def_key).await.unwrap();

    assert_eq!(engine.get_stats().await.pending_service_tasks, 100);
    drain_service_tasks(&engine, "worker_1").await;

    let elapsed = start_time.elapsed();
    assert!(
        elapsed.as_secs() < 30,
        "Parallel handling too slow: {:?}",
        elapsed
    );
    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );
}

#[tokio::test]
async fn stress_memory_10000_instances() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("wait")
        .node("start", BpmnElement::StartEvent)
        .node("wait", BpmnElement::UserTask("assignee".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "wait")
        .flow("wait", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    for _ in 0..10_000 {
        let mut vars = std::collections::HashMap::new();
        vars.insert("var1".into(), serde_json::Value::Number(1.into()));
        vars.insert("var2".into(), serde_json::Value::Number(2.into()));
        vars.insert("var3".into(), serde_json::Value::Number(3.into()));
        engine
            .start_instance_with_variables(def_key, vars)
            .await
            .unwrap();
    }

    let stats = engine.get_stats().await;
    assert_eq!(
        stats.instances_running + stats.instances_waiting_user,
        10_000
    );
    assert_eq!(stats.pending_user_tasks, 10_000);
}

// ---------------------------------------------------------------------------
// Category B: Gateway-Korrektheit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn correctness_nested_parallel_3_levels() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("nested_par")
        .node("start", BpmnElement::StartEvent)
        .node("split1", BpmnElement::ParallelGateway)
        .node("split2", BpmnElement::ParallelGateway)
        .node(
            "t2a",
            BpmnElement::ServiceTask {
                topic: "t2a".into(),
            },
        )
        .node(
            "t2b",
            BpmnElement::ServiceTask {
                topic: "t2b".into(),
            },
        )
        .node("join2", BpmnElement::ParallelGateway)
        .node(
            "t_b",
            BpmnElement::ServiceTask {
                topic: "t_b".into(),
            },
        )
        .node("join1", BpmnElement::ParallelGateway)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "split1")
        .flow("split1", "split2")
        .flow("split2", "t2a")
        .flow("split2", "t2b")
        .flow("t2a", "join2")
        .flow("t2b", "join2")
        .flow("join2", "join1")
        .flow("split1", "t_b")
        .flow("t_b", "join1")
        .flow("join1", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();

    let topics = vec!["t2b", "t_b", "t2a"];
    for topic in topics {
        let mut vars = std::collections::HashMap::new();
        vars.insert(topic.to_string(), serde_json::Value::Bool(true));
        let locked = engine
            .fetch_and_lock_service_tasks("w", 1, &[topic.into()], 1000)
            .await;
        engine
            .complete_service_task(locked[0].id, "w", vars)
            .await
            .unwrap();
    }

    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );
    let details = engine.get_instance_details(inst_id).await.unwrap();
    assert!(details.variables.contains_key("t2a"));
    assert!(details.variables.contains_key("t2b"));
    assert!(details.variables.contains_key("t_b"));
}

#[tokio::test]
async fn correctness_inclusive_gateway_partial_match() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("incl_part")
        .node("start", BpmnElement::StartEvent)
        .node("gw", BpmnElement::InclusiveGateway)
        .node("t1", BpmnElement::ServiceTask { topic: "t1".into() })
        .node("t2", BpmnElement::ServiceTask { topic: "t2".into() })
        .node("t3", BpmnElement::ServiceTask { topic: "t3".into() })
        .node("end", BpmnElement::EndEvent)
        .flow("start", "gw")
        .conditional_flow("gw", "t1", "x == 1")
        .conditional_flow("gw", "t2", "y == 2")
        .conditional_flow("gw", "t3", "z == 3")
        .flow("t1", "end")
        .flow("t2", "end")
        .flow("t3", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;

    let mut vars = std::collections::HashMap::new();
    vars.insert("x".into(), serde_json::Value::Number(1.into()));
    vars.insert("z".into(), serde_json::Value::Number(3.into())); // only 2 matches

    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();
    assert_eq!(engine.get_stats().await.pending_service_tasks, 2);

    drain_service_tasks(&engine, "worker_1").await;

    // As it was 2 partial matches, completing them completes the workflow.
    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );
}

#[tokio::test]
async fn correctness_exclusive_no_default_all_false() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("xor")
        .node("start", BpmnElement::StartEvent)
        .node("gw", BpmnElement::ExclusiveGateway { default: None })
        .node("t1", BpmnElement::EndEvent)
        .node("t2", BpmnElement::EndEvent)
        .flow("start", "gw")
        .conditional_flow("gw", "t1", "x == 1")
        .conditional_flow("gw", "t2", "x == 2")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    let res = engine.start_instance(def_key).await;
    assert!(matches!(res, Err(EngineError::NoMatchingCondition(_))));
}

#[tokio::test]
async fn correctness_mixed_xor_and_parallel() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("mixed")
        .node("start", BpmnElement::StartEvent)
        .node("split_par", BpmnElement::ParallelGateway)
        .node("split_xor", BpmnElement::ExclusiveGateway { default: None })
        .node("t1", BpmnElement::ServiceTask { topic: "t1".into() })
        .node("t2", BpmnElement::ServiceTask { topic: "t2".into() })
        .node("t3", BpmnElement::ServiceTask { topic: "t3".into() })
        .node("join_par", BpmnElement::ParallelGateway)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "split_par")
        .flow("split_par", "split_xor")
        .conditional_flow("split_xor", "t1", "x == 1")
        .conditional_flow("split_xor", "t2", "x != 1")
        .flow("split_par", "t3")
        .flow("t1", "join_par")
        .flow("t2", "join_par")
        .flow("t3", "join_par")
        .flow("join_par", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    let mut vars = std::collections::HashMap::new();
    vars.insert("x".into(), serde_json::Value::Number(1.into())); // chooses t1
    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();

    assert_eq!(engine.get_stats().await.pending_service_tasks, 2); // t1 and t3 should be pending

    drain_service_tasks(&engine, "w").await;
    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );
}

// ---------------------------------------------------------------------------
// Category C: Persistence & Recovery
// ---------------------------------------------------------------------------

#[tokio::test]
async fn persistence_crash_recovery_user_task() {
    let engine = WorkflowEngine::with_in_memory_persistence();
    let def = ProcessDefinitionBuilder::new("usr1")
        .node("start", BpmnElement::StartEvent)
        .node("ut", BpmnElement::UserTask("u".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "ut")
        .flow("ut", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def.clone()).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();

    assert_eq!(engine.pending_user_tasks.len(), 1);

    // Save state
    let persistence = engine.persistence.clone().unwrap();

    // Drop engine
    drop(engine);

    // Create new engine and restore instance
    let mut new_engine = WorkflowEngine::new();
    new_engine.persistence = Some(persistence.clone());

    let state = persistence
        .list_instances()
        .await
        .unwrap()
        .into_iter()
        .find(|i| i.id == inst_id)
        .unwrap();
    new_engine
        .definitions
        .insert(def_key, std::sync::Arc::new(def))
        .await;
    new_engine.restore_instance(state).await;

    // Validate state was fully restored
    let tasks = persistence.list_user_tasks().await.unwrap();
    for task in &tasks {
        new_engine
            .pending_user_tasks
            .insert(task.task_id, task.clone());
    }
    assert_eq!(tasks.len(), 1);

    new_engine
        .complete_user_task(tasks[0].task_id, std::collections::HashMap::new())
        .await
        .unwrap();
    assert_eq!(
        new_engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );
}

#[tokio::test]
async fn persistence_crash_recovery_parallel() {
    let engine = WorkflowEngine::with_in_memory_persistence();
    let def = ProcessDefinitionBuilder::new("parcr")
        .node("start", BpmnElement::StartEvent)
        .node("split", BpmnElement::ParallelGateway)
        .node("t1", BpmnElement::UserTask("u".into()))
        .node("t2", BpmnElement::UserTask("u".into()))
        .node("join", BpmnElement::ParallelGateway)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "split")
        .flow("split", "t1")
        .flow("split", "t2")
        .flow("t1", "join")
        .flow("t2", "join")
        .flow("join", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def.clone()).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();

    let tasks = engine.get_pending_user_tasks();
    assert_eq!(tasks.len(), 2);

    // Complete T1
    engine
        .complete_user_task(tasks[0].task_id, std::collections::HashMap::new())
        .await
        .unwrap();

    let persistence = engine.persistence.clone().unwrap();
    drop(engine);

    let mut new_engine = WorkflowEngine::new();
    new_engine.persistence = Some(persistence.clone());

    let state = persistence
        .list_instances()
        .await
        .unwrap()
        .into_iter()
        .find(|i| i.id == inst_id)
        .unwrap();
    new_engine
        .definitions
        .insert(def_key, std::sync::Arc::new(def))
        .await;
    new_engine.restore_instance(state).await;
    let tasks_persisted = persistence.list_user_tasks().await.unwrap();
    for t in &tasks_persisted {
        new_engine.pending_user_tasks.insert(t.task_id, t.clone());
    }

    // T2 should still be pending
    let tasks2 = new_engine.get_pending_user_tasks();
    assert_eq!(tasks2.len(), 1);

    // Complete T2 (Should not deadlock)
    new_engine
        .complete_user_task(tasks2[0].task_id, std::collections::HashMap::new())
        .await
        .unwrap();
    assert_eq!(
        new_engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );
}

#[tokio::test]
async fn persistence_audit_log_roundtrip_at_max() {
    let engine = WorkflowEngine::with_in_memory_persistence();
    let def = ProcessDefinitionBuilder::new("al")
        .node("start", BpmnElement::StartEvent)
        .node("ut", BpmnElement::UserTask("u".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "ut")
        .flow("ut", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();

    {
        let inst_arc = engine.instances.get(&inst_id).await.unwrap();
        let mut inst = inst_arc.write().await;
        // Inject 250 log entries
        for i in 0..250 {
            inst.audit_log.push(format!("Entry {}", i));
        }
    }

    engine.persist_instance(inst_id).await; // Should trim to 200

    let persistence = engine.persistence.clone().unwrap();
    drop(engine);

    let state = persistence
        .list_instances()
        .await
        .unwrap()
        .into_iter()
        .find(|i| i.id == inst_id)
        .unwrap();
    assert!(state.audit_log.len() <= 201); // 200 elements + possible truncation message
}

#[tokio::test]
async fn persistence_error_counter_increments() {
    struct FailingPersistence;

    #[async_trait::async_trait]
    impl crate::persistence::WorkflowPersistence for FailingPersistence {
        async fn save_token(&self, _: &crate::model::Token) -> EngineResult<()> {
            Ok(())
        }
        async fn load_tokens(&self, _: &str) -> EngineResult<Vec<crate::model::Token>> {
            Ok(vec![])
        }
        async fn save_instance(&self, _: &ProcessInstance) -> EngineResult<()> {
            Err(crate::error::EngineError::PersistenceError(
                "Injected failure".into(),
            ))
        }
        async fn list_instances(&self) -> EngineResult<Vec<ProcessInstance>> {
            Ok(vec![])
        }
        async fn delete_instance(&self, _: &str) -> EngineResult<()> {
            Ok(())
        }
        async fn save_definition(&self, _: &crate::model::ProcessDefinition) -> EngineResult<()> {
            Ok(())
        }
        async fn list_definitions(&self) -> EngineResult<Vec<crate::model::ProcessDefinition>> {
            Ok(vec![])
        }
        async fn delete_definition(&self, _: &str) -> EngineResult<()> {
            Ok(())
        }
        async fn save_user_task(&self, _: &crate::engine::PendingUserTask) -> EngineResult<()> {
            Ok(())
        }
        async fn delete_user_task(&self, _: uuid::Uuid) -> EngineResult<()> {
            Ok(())
        }
        async fn list_user_tasks(&self) -> EngineResult<Vec<crate::engine::PendingUserTask>> {
            Ok(vec![])
        }
        async fn save_service_task(
            &self,
            _: &crate::engine::PendingServiceTask,
        ) -> EngineResult<()> {
            Ok(())
        }
        async fn delete_service_task(&self, _: uuid::Uuid) -> EngineResult<()> {
            Ok(())
        }
        async fn list_service_tasks(&self) -> EngineResult<Vec<crate::engine::PendingServiceTask>> {
            Ok(vec![])
        }
        async fn save_timer(&self, _: &crate::engine::PendingTimer) -> EngineResult<()> {
            Ok(())
        }
        async fn delete_timer(&self, _: uuid::Uuid) -> EngineResult<()> {
            Ok(())
        }
        async fn list_timers(&self) -> EngineResult<Vec<crate::engine::PendingTimer>> {
            Ok(vec![])
        }
        async fn save_message_catch(
            &self,
            _: &crate::engine::PendingMessageCatch,
        ) -> EngineResult<()> {
            Ok(())
        }
        async fn delete_message_catch(&self, _: uuid::Uuid) -> EngineResult<()> {
            Ok(())
        }
        async fn list_message_catches(
            &self,
        ) -> EngineResult<Vec<crate::engine::PendingMessageCatch>> {
            Ok(vec![])
        }
        async fn save_file(&self, _: &str, _: &[u8]) -> EngineResult<()> {
            Ok(())
        }
        async fn load_file(&self, _: &str) -> EngineResult<Vec<u8>> {
            Ok(vec![])
        }
        async fn delete_file(&self, _: &str) -> EngineResult<()> {
            Ok(())
        }
        async fn save_bpmn_xml(&self, _: &str, _: &str) -> EngineResult<()> {
            Ok(())
        }
        async fn load_bpmn_xml(&self, _: &str) -> EngineResult<String> {
            Ok("".into())
        }
        async fn list_bpmn_xml_ids(&self) -> EngineResult<Vec<String>> {
            Ok(vec![])
        }
        async fn get_storage_info(&self) -> EngineResult<Option<crate::persistence::StorageInfo>> {
            Ok(None)
        }
        async fn append_history_entry(&self, _: &crate::history::HistoryEntry) -> EngineResult<()> {
            Ok(())
        }
        async fn query_history(
            &self,
            _: crate::persistence::HistoryQuery,
        ) -> EngineResult<Vec<crate::history::HistoryEntry>> {
            Ok(vec![])
        }
        async fn get_bucket_entries(
            &self,
            _: &str,
            _: usize,
            _: usize,
        ) -> EngineResult<Vec<crate::persistence::BucketEntry>> {
            Ok(vec![])
        }
        async fn get_bucket_entry_detail(
            &self,
            _: &str,
            _: &str,
        ) -> EngineResult<crate::persistence::BucketEntryDetail> {
            Err(crate::error::EngineError::PersistenceError("Mock".into()))
        }
    }

    let mut engine = WorkflowEngine::new();
    engine.persistence = Some(std::sync::Arc::new(FailingPersistence));

    let def = ProcessDefinitionBuilder::new("failing")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    engine.start_instance(def_key).await.unwrap();

    assert!(engine.get_stats().await.persistence_errors > 0);
}

// ---------------------------------------------------------------------------
// Category D: Concurrency & Race Conditions
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn race_concurrent_complete_same_user_task() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("race1")
        .node("start", BpmnElement::StartEvent)
        .node("ut", BpmnElement::UserTask("worker".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "ut")
        .flow("ut", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    engine.start_instance(def_key).await.unwrap();

    let task_id = engine.get_pending_user_tasks()[0].task_id;
    let engine_arc = std::sync::Arc::new(engine);

    let mut handles = Vec::new();
    for _ in 0..5 {
        let engine_clone = engine_arc.clone();
        handles.push(tokio::spawn(async move {
            let eng = engine_clone;
            eng.complete_user_task(task_id, std::collections::HashMap::new())
                .await
        }));
    }

    let mut ok_count = 0;
    let mut err_count = 0;
    for handle in handles {
        let res = handle.await.unwrap();
        match res {
            Ok(_) => ok_count += 1,
            Err(crate::error::EngineError::TaskNotPending { .. }) => err_count += 1,
            _ => panic!("Unexpected result: {:?}", res),
        }
    }

    assert_eq!(ok_count, 1);
    assert_eq!(err_count, 4);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn race_concurrent_message_correlation() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("race2")
        .node("start", BpmnElement::StartEvent)
        .node(
            "msg",
            BpmnElement::MessageCatchEvent {
                message_name: "MSG_A".into(),
            },
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "msg")
        .flow("msg", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    engine.start_instance(def_key).await.unwrap();

    let engine_arc = std::sync::Arc::new(engine);
    let mut handles = Vec::new();

    for _ in 0..5 {
        let engine_clone = engine_arc.clone();
        handles.push(tokio::spawn(async move {
            let eng = engine_clone;
            eng.correlate_message("MSG_A".to_string(), None, std::collections::HashMap::new())
                .await
        }));
    }

    for handle in handles {
        let _ = handle.await.unwrap(); // correlation doesn't strictly error if no catch is found, it just succeeds gracefully or returns affected instances.
    }

    let eng = engine_arc;
    assert_eq!(eng.get_stats().await.instances_completed, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn race_concurrent_fetch_same_task() {
    let engine = WorkflowEngine::new();
    let def = linear_definition("race3", 1);
    let (def_key, _) = engine.deploy_definition(def).await;
    engine.start_instance(def_key).await.unwrap();

    let engine_arc = std::sync::Arc::new(engine);
    let mut handles = Vec::new();

    for i in 0..10 {
        let engine_clone = engine_arc.clone();
        handles.push(tokio::spawn(async move {
            let eng = engine_clone;
            eng.fetch_and_lock_service_tasks(
                &format!("worker_{}", i),
                1,
                &["topic_1".to_string()],
                30000,
            )
            .await
        }));
    }

    let mut locked_tasks = 0;
    for handle in handles {
        let tasks = handle.await.unwrap();
        locked_tasks += tasks.len();
    }

    assert_eq!(locked_tasks, 1);
}

// ---------------------------------------------------------------------------
// Category E: Edge Cases & Boundaries
// ---------------------------------------------------------------------------

#[tokio::test]
async fn edge_1000_variables_at_start() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("e1")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;

    let mut vars = std::collections::HashMap::with_capacity(1000);
    for i in 0..1000 {
        vars.insert(format!("var_{}", i), serde_json::Value::Number(i.into()));
    }

    let inst_id = engine
        .start_instance_with_variables(def_key, vars)
        .await
        .unwrap();
    let details = engine.get_instance_details(inst_id).await.unwrap();

    assert_eq!(details.variables.len(), 1000);
}

#[tokio::test]
async fn edge_timer_zero_duration() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("e3")
        .node("start", BpmnElement::StartEvent)
        .node(
            "timer",
            BpmnElement::TimerCatchEvent(crate::timer_definition::TimerDefinition::Duration(std::time::Duration::from_secs(0))),
        )
        .node("end", BpmnElement::EndEvent)
        .flow("start", "timer")
        .flow("timer", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();

    let _ = engine.process_timers().await;
    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );
}

#[tokio::test]
async fn edge_rhai_infinite_loop() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("e4")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .listener("start", crate::model::ListenerEvent::Start, "while true {}")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    let res = engine.start_instance(def_key).await;

    assert!(matches!(
        res,
        Err(crate::error::EngineError::ScriptError(_))
    ));
}

#[tokio::test]
async fn edge_empty_start_to_end() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("e5")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();

    assert_eq!(
        engine.get_instance_state(inst_id).await.unwrap(),
        InstanceState::Completed
    );
}

#[tokio::test]
async fn edge_complete_task_on_completed_instance() {
    let engine = WorkflowEngine::new();
    let def = ProcessDefinitionBuilder::new("e7")
        .node("start", BpmnElement::StartEvent)
        .node("end", BpmnElement::EndEvent)
        .flow("start", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    engine.start_instance(def_key).await.unwrap();

    let res = engine
        .complete_user_task(uuid::Uuid::new_v4(), std::collections::HashMap::new())
        .await;
    assert!(matches!(
        res,
        Err(crate::error::EngineError::TaskNotPending { .. })
    ));
}

// ---------------------------------------------------------------------------
// Category H: History & Observability
// ---------------------------------------------------------------------------

#[tokio::test]
async fn history_completeness_complex_process() {
    let engine = WorkflowEngine::with_in_memory_persistence();
    let def = ProcessDefinitionBuilder::new("hist_cplx")
        .node("start", BpmnElement::StartEvent)
        .node("ut", BpmnElement::UserTask("worker".into()))
        .node("end", BpmnElement::EndEvent)
        .flow("start", "ut")
        .flow("ut", "end")
        .build()
        .unwrap();

    let (def_key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();

    let task_id = engine.get_pending_user_tasks()[0].task_id;
    engine
        .complete_user_task(task_id, std::collections::HashMap::new())
        .await
        .unwrap();

    let hist = engine
        .persistence
        .as_ref()
        .unwrap()
        .query_history(crate::persistence::HistoryQuery {
            instance_id: inst_id,
            ..Default::default()
        })
        .await
        .unwrap();

    // Check that we have start event, user task and end event in history, each with STARTED and COMPLETED events (except end event might not have started)
    assert!(hist.len() >= 4);

    let types: Vec<_> = hist.iter().map(|h| &h.event_type).collect();
    assert!(types.contains(&&crate::history::HistoryEventType::InstanceStarted));
    assert!(types.contains(&&crate::history::HistoryEventType::InstanceCompleted));
    assert!(types.contains(&&crate::history::HistoryEventType::TokenAdvanced));
    assert!(types.contains(&&crate::history::HistoryEventType::TaskCompleted));
}

#[tokio::test]
async fn history_query_limit_and_offset() {
    let engine = WorkflowEngine::with_in_memory_persistence();
    let def = linear_definition("hist_pag", 5); // 5 service tasks
    let (def_key, _) = engine.deploy_definition(def).await;
    let inst_id = engine.start_instance(def_key).await.unwrap();

    drain_service_tasks(&engine, "w").await;

    let hist = engine
        .persistence
        .as_ref()
        .unwrap()
        .query_history(crate::persistence::HistoryQuery {
            instance_id: inst_id,
            ..Default::default()
        })
        .await
        .unwrap();

    assert!(hist.len() > 5);

    let hist_pag1 = engine
        .persistence
        .as_ref()
        .unwrap()
        .query_history(crate::persistence::HistoryQuery {
            instance_id: inst_id,
            limit: Some(3),
            offset: Some(0),
            ..Default::default()
        })
        .await
        .unwrap();

    let hist_pag2 = engine
        .persistence
        .as_ref()
        .unwrap()
        .query_history(crate::persistence::HistoryQuery {
            instance_id: inst_id,
            limit: Some(3),
            offset: Some(3),
            ..Default::default()
        })
        .await
        .unwrap();

    assert_eq!(hist_pag1.len(), 3);
    assert_eq!(hist_pag2.len(), 3);

    assert_eq!(hist_pag1[0].id, hist[0].id);
    assert_eq!(hist_pag2[0].id, hist[3].id);
}
