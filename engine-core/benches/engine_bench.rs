use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use engine_core::engine::WorkflowEngine;
use engine_core::model::{BpmnElement, ProcessDefinitionBuilder};

fn linear_definition(key: &str, node_count: usize) -> engine_core::model::ProcessDefinition {
    let mut builder = ProcessDefinitionBuilder::new(key).node("start", BpmnElement::StartEvent);
    builder = builder.flow("start", "t_0");

    for i in 0..node_count {
        let node_id = format!("t_{}", i);
        builder = builder.node(
            &node_id,
            BpmnElement::ServiceTask {
                topic: "topic_1".into(),
                multi_instance: None,
            },
        );

        if i < node_count - 1 {
            builder = builder.flow(&node_id, format!("t_{}", i + 1));
        }
    }

    builder = builder
        .node("end", BpmnElement::EndEvent)
        .flow(format!("t_{}", node_count - 1), "end");

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

pub fn bench_execution_throughput(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut group = c.benchmark_group("Engine Throughput");

    for size in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("Linear Process Execution", size),
            size,
            |b, &s| {
                b.to_async(&rt).iter(|| async {
                    let engine = WorkflowEngine::new();
                    let def = linear_definition("bench_linear", s);
                    let (def_key, _) = engine.deploy_definition(def).await;

                    let inst_id = engine.start_instance(def_key).await.unwrap();
                    drain_service_tasks(&engine, "bench_worker").await;

                    assert_eq!(
                        engine.get_instance_state(inst_id).await.unwrap(),
                        engine_core::engine::InstanceState::Completed
                    );
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_execution_throughput);
criterion_main!(benches);
