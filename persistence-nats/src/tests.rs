use engine_core::model::Token;
use engine_core::persistence::WorkflowPersistence;
use std::sync::Arc;
use uuid::Uuid;

use crate::client::NatsPersistence;

pub async fn setup_nats_test() -> Option<Arc<NatsPersistence>> {
    let url = "nats://localhost:4222";
    let stream = format!("TEST_STREAM_{}", Uuid::new_v4());

    match NatsPersistence::connect(url, &stream).await {
        Ok(persistence) => Some(Arc::new(persistence)),
        Err(e) => {
            tracing::warn!("Skipping NATS test, could not connect: {}", e);
            None
        }
    }
}

#[tokio::test]
async fn test_save_and_load_token() {
    let persistence = match setup_nats_test().await {
        Some(p) => p,
        None => return, // Ignore if NATS container is not running
    };

    let mut token = Token::new("start_node");
    token.variables.insert(
        "test_key".into(),
        serde_json::Value::String("test_value".into()),
    );

    persistence.save_token(&token).await.unwrap();

    // Event-Sourcing Light Scenario
    token.current_node = "next_node".to_string();
    persistence.save_token(&token).await.unwrap();

    let loaded_tokens = persistence.load_tokens("some_process_id").await.unwrap();

    assert_eq!(loaded_tokens.len(), 1);
    let loaded_token = &loaded_tokens[0];

    assert_eq!(loaded_token.id, token.id);
    assert_eq!(loaded_token.current_node, "next_node");
    assert_eq!(
        loaded_token
            .variables
            .get("test_key")
            .unwrap()
            .as_str()
            .unwrap(),
        "test_value"
    );
}

#[tokio::test]
async fn test_history_append_and_load() {
    let persistence = match setup_nats_test().await {
        Some(p) => p,
        None => return, // Ignore if NATS container is not running
    };

    let instance_id = Uuid::new_v4();
    let entry1 = engine_core::history::HistoryEntry::new(
        instance_id,
        engine_core::history::HistoryEventType::InstanceStarted,
        "Instance Started",
        engine_core::history::ActorType::Engine,
        None,
    );

    let entry2 = engine_core::history::HistoryEntry::new(
        instance_id,
        engine_core::history::HistoryEventType::TokenAdvanced,
        "Token moved",
        engine_core::history::ActorType::Engine,
        None,
    )
    .with_node("task_1");

    // Append to stream
    persistence.append_history_entry(&entry1).await.unwrap();
    persistence.append_history_entry(&entry2).await.unwrap();

    // Give NATS JetStream a tiny bit of time to flush/index
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let history = persistence
        .query_history(engine_core::persistence::HistoryQuery {
            instance_id,
            ..Default::default()
        })
        .await
        .unwrap();

    assert_eq!(history.len(), 2);
    assert_eq!(history[0].id, entry1.id);
    assert_eq!(
        history[0].event_type,
        engine_core::history::HistoryEventType::InstanceStarted
    );
    assert_eq!(history[1].id, entry2.id);
    assert_eq!(
        history[1].event_type,
        engine_core::history::HistoryEventType::TokenAdvanced
    );
    assert_eq!(history[1].node_id.as_deref(), Some("task_1"));
}
