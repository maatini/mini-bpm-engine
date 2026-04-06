use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::engine::ProcessInstance;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActorType {
    Engine,
    User,
    ServiceWorker,
    Timer,
    Listener,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HistoryEventType {
    InstanceStarted,
    InstanceCompleted,
    InstanceDeleted,
    TaskCompleted,    // User Task / Service Task completed
    VariableUpdated,  // explicit update
    GatewayTaken,     // XOR / OR fork
    ListenerExecuted, // Scripts
    TokenAdvanced,    // generic token move
    TokenForked,      // Gateway split created N tokens
    TokenJoined,      // Gateway join merged N tokens into one
    BranchCompleted,  // One branch of a parallel execution reached EndEvent
    Error,            // something failed
    CallActivityStarted,
    CallActivityCompleted,
}

impl HistoryEventType {
    /// Returns a human-readable description of the event type.
    pub fn human_description(&self) -> String {
        match self {
            Self::InstanceStarted => "Process instance started".into(),
            Self::InstanceCompleted => "Process instance completed".into(),
            Self::InstanceDeleted => "Process instance was deleted".into(),
            Self::TaskCompleted => "Task was completed".into(),
            Self::VariableUpdated => "Variable was updated".into(),
            Self::GatewayTaken => "Gateway path was taken".into(),
            Self::ListenerExecuted => "Execution listener finished".into(),
            Self::TokenAdvanced => "Token advanced to the next node".into(),
            Self::TokenForked => "Token forked into multiple branches".into(),
            Self::TokenJoined => "Multiple tokens joined into one".into(),
            Self::BranchCompleted => "Branch execution completed".into(),
            Self::Error => "An execution error occurred".into(),
            Self::CallActivityStarted => "Call activity started".into(),
            Self::CallActivityCompleted => "Call activity completed".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableDiff {
    pub added: HashMap<String, serde_json::Value>,
    pub removed: Vec<String>,
    pub changed: HashMap<String, (serde_json::Value, serde_json::Value)>, // (old, new)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryDiff {
    pub variables: Option<VariableDiff>,
    pub status: Option<(String, String)>, // "RUNNING" -> "COMPLETED"
    pub current_node: Option<(String, String)>, // "start" -> "task1"
    pub human_readable: Option<String>,   // Auto-generated human-readable text
}

/// Returns true if the diff has actually recorded any changes.
impl HistoryDiff {
    pub fn is_empty(&self) -> bool {
        self.variables.is_none()
            && self.status.is_none()
            && self.current_node.is_none()
            && self.human_readable.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub event_type: HistoryEventType,
    pub node_id: Option<String>, // BPMN element ID
    pub description: String,
    pub actor_type: ActorType,
    pub actor_id: Option<String>,
    pub diff: Option<HistoryDiff>,
    #[serde(default)]
    pub context: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub definition_version: Option<i32>,
    #[serde(default)]
    pub is_snapshot: bool,
    #[serde(default)]
    pub full_state_snapshot: Option<serde_json::Value>,
}

impl HistoryEntry {
    pub fn new(
        instance_id: Uuid,
        event_type: HistoryEventType,
        description: impl Into<String>,
        actor_type: ActorType,
        actor_id: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            instance_id,
            timestamp: Utc::now(),
            event_type,
            node_id: None,
            description: description.into(),
            actor_type,
            actor_id,
            diff: None,
            context: HashMap::new(),
            metadata: None,
            definition_version: None,
            is_snapshot: false,
            full_state_snapshot: None,
        }
    }

    pub fn with_node(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = Some(node_id.into());
        self
    }

    pub fn with_diff(mut self, diff: HistoryDiff) -> Self {
        if !diff.is_empty() {
            if let Some(human_text) = &diff.human_readable {
                self.description = human_text.clone();
            }
            self.diff = Some(diff);
        }
        self
    }

    pub fn with_metadata(mut self, meta: serde_json::Value) -> Self {
        self.metadata = Some(meta);
        self
    }

    pub fn with_context(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.context.insert(key.into(), value);
        self
    }

    pub fn with_snapshot(mut self, snapshot: serde_json::Value) -> Self {
        self.is_snapshot = true;
        self.full_state_snapshot = Some(snapshot);
        self
    }
}

fn format_file_human_text(_key: &str, value: &serde_json::Value) -> Option<String> {
    if value.get("type").and_then(|t| t.as_str()) == Some("file") {
        let filename = value
            .get("filename")
            .and_then(|f| f.as_str())
            .unwrap_or("unknown");
        let size = value
            .get("size_bytes")
            .and_then(|s| s.as_u64())
            .unwrap_or(0);
        let size_human = if size > 1_048_576 {
            format!("{:.1} MB", size as f64 / 1_048_576.0)
        } else {
            format!("{:.1} KB", size as f64 / 1024.0)
        };
        Some(format!("File '{}' uploaded ({}).", filename, size_human))
    } else {
        None
    }
}

fn truncate_value_for_diff(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::String(s) if s.len() > 1024 => serde_json::Value::String(format!(
            "{}... <truncated {} chars>",
            &s[..1024],
            s.len() - 1024
        )),
        serde_json::Value::Array(a) if a.len() > 128 => {
            serde_json::Value::String(format!("<Large Array: {} elements>", a.len()))
        }
        _ => v.clone(),
    }
}

/// Calculates the difference between two process instance states.
pub fn calculate_diff(old: &ProcessInstance, new: &ProcessInstance) -> HistoryDiff {
    let mut diff = HistoryDiff {
        variables: None,
        status: None,
        current_node: None,
        human_readable: None,
    };

    let mut human_texts = Vec::new();

    // Calculate status diff
    let old_status = format!("{:?}", old.state);
    let new_status = format!("{:?}", new.state);
    if old_status != new_status {
        diff.status = Some((old_status, new_status));
        human_texts.push(format!("Status changed to {:?}.", new.state));
    }

    // Calculate current_node diff
    if old.current_node != new.current_node {
        diff.current_node = Some((old.current_node.clone(), new.current_node.clone()));
        human_texts.push(format!(
            "Advanced from node '{}' to '{}'.",
            old.current_node, new.current_node
        ));
    }

    // Calculate variable diff
    let mut var_diff = VariableDiff {
        added: HashMap::new(),
        removed: Vec::new(),
        changed: HashMap::new(),
    };

    // Check for removed and changed
    for (k, v_old) in &old.variables {
        if let Some(v_new) = new.variables.get(k) {
            if v_old != v_new {
                var_diff.changed.insert(
                    k.clone(),
                    (
                        truncate_value_for_diff(v_old),
                        truncate_value_for_diff(v_new),
                    ),
                );
                if let Some(file_text) = format_file_human_text(k, v_new) {
                    human_texts.push(file_text);
                } else {
                    let text = format!(
                        "Variable '{}' changed from {} to {}.",
                        k,
                        truncate_value_for_diff(v_old),
                        truncate_value_for_diff(v_new)
                    );
                    human_texts.push(text.chars().take(500).collect::<String>());
                }
            }
        } else {
            var_diff.removed.push(k.clone());
            human_texts.push(format!("Variable '{}' was removed.", k));
        }
    }

    // Check for added
    for (k, v_new) in &new.variables {
        if !old.variables.contains_key(k) {
            var_diff
                .added
                .insert(k.clone(), truncate_value_for_diff(v_new));
            if let Some(file_text) = format_file_human_text(k, v_new) {
                human_texts.push(file_text);
            } else {
                let text = format!(
                    "Variable '{}' was added ({}).",
                    k,
                    truncate_value_for_diff(v_new)
                );
                human_texts.push(text.chars().take(500).collect::<String>());
            }
        }
    }

    if !var_diff.added.is_empty() || !var_diff.removed.is_empty() || !var_diff.changed.is_empty() {
        diff.variables = Some(var_diff);
    }

    if !human_texts.is_empty() {
        diff.human_readable = Some(human_texts.join(" "));
    }

    diff
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::InstanceState;
    use serde_json::json;

    #[test]
    fn test_calculate_diff() {
        let mut old = ProcessInstance {
            id: Uuid::new_v4(),
            definition_key: Uuid::new_v4(),
            business_key: "BK-1".into(),
            parent_instance_id: None,
            state: InstanceState::Running,
            current_node: "start".into(),
            audit_log: vec![],
            variables: HashMap::new(),
            tokens: HashMap::new(),
            active_tokens: vec![],
            join_barriers: HashMap::new(), multi_instance_state: HashMap::new(),
        };
        old.variables.insert("a".into(), json!(1));
        old.variables.insert("b".into(), json!(2));

        let mut new = old.clone();
        new.current_node = "task1".into();
        new.state = InstanceState::WaitingOnUserTask {
            task_id: Uuid::new_v4(),
        };
        new.variables.insert("a".into(), json!(100)); // changed
        new.variables.remove("b"); // removed
        new.variables.insert("c".into(), json!(3)); // added

        let diff = calculate_diff(&old, &new);

        assert!(diff.human_readable.is_some());
        assert!(diff.status.is_some());
        assert!(diff.status.unwrap().0.contains("Running"));
        assert!(diff.current_node.is_some());
        assert_eq!(
            diff.current_node.unwrap(),
            ("start".to_string(), "task1".to_string())
        );

        let var_diff = diff.variables.unwrap();
        assert_eq!(var_diff.changed.get("a").unwrap(), &(json!(1), json!(100)));
        assert_eq!(var_diff.removed[0], "b");
        assert_eq!(var_diff.added.get("c").unwrap(), &json!(3));
    }

    #[test]
    fn test_calculate_diff_file_upload_human_text() {
        let old = ProcessInstance {
            id: Uuid::new_v4(),
            definition_key: Uuid::new_v4(),
            business_key: "BK-1".into(),
            parent_instance_id: None,
            state: InstanceState::Running,
            current_node: "start".into(),
            audit_log: vec![],
            variables: HashMap::new(),
            tokens: HashMap::new(),
            active_tokens: vec![],
            join_barriers: HashMap::new(), multi_instance_state: HashMap::new(),
        };

        let mut new = old.clone();
        new.variables.insert(
            "report".into(),
            json!({
                "type": "file",
                "object_key": "file:test",
                "filename": "report.pdf",
                "mime_type": "application/pdf",
                "size_bytes": 1258291, // ~1.2 MB
                "uploaded_at": "2026-04-03T12:00:00Z"
            }),
        );

        let diff = calculate_diff(&old, &new);
        let human = diff.human_readable.unwrap();
        assert_eq!(human, "File 'report.pdf' uploaded (1.2 MB).");

        // test KB format
        let mut new2 = old.clone();
        new2.variables.insert(
            "config".into(),
            json!({
                "type": "file",
                "object_key": "file:test",
                "filename": "config.json",
                "mime_type": "application/json",
                "size_bytes": 1024, // 1.0 KB
                "uploaded_at": "2026-04-03T12:00:00Z"
            }),
        );

        let diff2 = calculate_diff(&old, &new2);
        let human2 = diff2.human_readable.unwrap();
        assert_eq!(human2, "File 'config.json' uploaded (1.0 KB).");
    }

    #[test]
    fn test_human_description() {
        assert_eq!(
            HistoryEventType::InstanceStarted.human_description(),
            "Process instance started"
        );
        assert_eq!(
            HistoryEventType::InstanceCompleted.human_description(),
            "Process instance completed"
        );
        assert_eq!(
            HistoryEventType::InstanceDeleted.human_description(),
            "Process instance was deleted"
        );
        assert_eq!(
            HistoryEventType::TaskCompleted.human_description(),
            "Task was completed"
        );
        assert_eq!(
            HistoryEventType::VariableUpdated.human_description(),
            "Variable was updated"
        );
        assert_eq!(
            HistoryEventType::GatewayTaken.human_description(),
            "Gateway path was taken"
        );
        assert_eq!(
            HistoryEventType::ListenerExecuted.human_description(),
            "Execution listener finished"
        );
        assert_eq!(
            HistoryEventType::TokenAdvanced.human_description(),
            "Token advanced to the next node"
        );
        assert_eq!(
            HistoryEventType::TokenForked.human_description(),
            "Token forked into multiple branches"
        );
        assert_eq!(
            HistoryEventType::TokenJoined.human_description(),
            "Multiple tokens joined into one"
        );
        assert_eq!(
            HistoryEventType::BranchCompleted.human_description(),
            "Branch execution completed"
        );
        assert_eq!(
            HistoryEventType::Error.human_description(),
            "An execution error occurred"
        );
    }

    #[test]
    fn test_calculate_diff_empty() {
        let old = ProcessInstance {
            id: Uuid::new_v4(),
            definition_key: Uuid::new_v4(),
            business_key: "BK-1".into(),
            parent_instance_id: None,
            state: InstanceState::Running,
            current_node: "start".into(),
            audit_log: vec![],
            variables: HashMap::new(),
            tokens: HashMap::new(),
            active_tokens: vec![],
            join_barriers: HashMap::new(), multi_instance_state: HashMap::new(),
        };

        let new = old.clone();
        let diff = calculate_diff(&old, &new);

        assert!(diff.is_empty());
        assert!(diff.variables.is_none());
        assert!(diff.status.is_none());
        assert!(diff.current_node.is_none());
        assert!(diff.human_readable.is_none());
    }
}
