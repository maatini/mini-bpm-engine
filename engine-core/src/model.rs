use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{EngineError, EngineResult};

// ---------------------------------------------------------------------------
// Execution Listeners (Scripts)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ListenerEvent {
    Start,
    End,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionListener {
    pub event: ListenerEvent,
    pub script: String,
}

// ---------------------------------------------------------------------------
// BPMN element types
// ---------------------------------------------------------------------------

/// A BPMN flow-node element.
///
/// Closed enum — the compiler enforces exhaustive matching, so adding a new
/// variant later will break every unhandled `match`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BpmnElement {
    /// A plain (none) start event — the process starts immediately.
    StartEvent,
    /// A timer-triggered start event that fires after the given duration.
    TimerStartEvent(Duration),
    /// An end event — the process terminates here.
    EndEvent,
    /// A service task that pauses the workflow and must be fetched and completed by remote workers.
    ServiceTask { topic: String },
    /// A user task assigned to a specific role or user.
    UserTask(String),
    /// An exclusive gateway (XOR) — exactly one outgoing path is taken based
    /// on condition evaluation. An optional `default` flow is followed when
    /// no condition matches.
    ExclusiveGateway { default: Option<String> },
    /// An inclusive gateway (OR) — all outgoing paths whose condition
    /// evaluates to `true` are taken (token forking).
    InclusiveGateway,
    /// A parallel gateway (AND) — all outgoing paths are taken unconditionally;
    /// as a join, waits for ALL incoming tokens.
    ParallelGateway,
    /// An event-based gateway — execution pauses until exactly one of the target catch events is triggered.
    EventBasedGateway,
    /// A timer intermediate catch event that pauses the token until the duration elapses.
    TimerCatchEvent(Duration),
    /// A boundary timer event attached to an activity.
    BoundaryTimerEvent {
        attached_to: String,
        duration: Duration,
        cancel_activity: bool,
    },
    /// A start event triggered by a named message.
    MessageStartEvent { message_name: String },
    /// An intermediate catch event waiting for a named message.
    MessageCatchEvent { message_name: String },
    /// A boundary error event attached to an activity.
    BoundaryErrorEvent {
        attached_to: String,
        error_code: Option<String>,
    },
    /// An end event that throws a specific BPMN error.
    ErrorEndEvent { error_code: String },
    /// A Call Activity that invokes another process definition.
    CallActivity { called_element: String },
    /// An Embedded Sub-Process that references a child process definition.
    SubProcess { called_element: String },
}

// ---------------------------------------------------------------------------
// Scope Event Listeners (Event Sub-Processes)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScopeEventListener {
    Timer {
        duration: Duration,
        is_interrupting: bool,
        target_definition: String,
    },
    Message {
        message_name: String,
        is_interrupting: bool,
        target_definition: String,
    },
    Error {
        error_code: Option<String>,
        target_definition: String,
    },
}

// ---------------------------------------------------------------------------
// Sequence flow (edge with optional condition)
// ---------------------------------------------------------------------------

/// A directed edge between two BPMN flow-nodes.
///
/// Carries an optional condition expression that gates whether the flow is
/// taken (relevant for exclusive and inclusive gateways).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SequenceFlow {
    /// Target node ID this flow points to.
    pub target: String,
    /// Optional condition expression, e.g. `"amount > 100"`.
    pub condition: Option<String>,
}

impl SequenceFlow {
    /// Creates a simple (unconditional) sequence flow.
    pub fn simple(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            condition: None,
        }
    }

    /// Creates a conditional sequence flow.
    pub fn conditional(target: impl Into<String>, condition: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            condition: Some(condition.into()),
        }
    }
}

// ---------------------------------------------------------------------------
// Token
// ---------------------------------------------------------------------------

/// A token traveling through the process graph.
///
/// Carries a unique ID, its current position, and a bag of process variables.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Token {
    #[allow(dead_code)]
    pub id: Uuid,
    pub current_node: String,
    pub variables: HashMap<String, Value>,
    #[serde(default)]
    pub is_merged: bool,
}

impl Token {
    /// Creates a new token positioned at the given node with empty variables.
    pub fn new(start_node: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            current_node: start_node.to_string(),
            variables: HashMap::new(),
            is_merged: false,
        }
    }

    /// Creates a new token with pre-populated variables.
    #[allow(dead_code)]
    pub fn with_variables(start_node: &str, variables: HashMap<String, Value>) -> Self {
        Self {
            id: Uuid::new_v4(),
            current_node: start_node.to_string(),
            variables,
            is_merged: false,
        }
    }
}

// ---------------------------------------------------------------------------
// File Reference
// ---------------------------------------------------------------------------

/// A typed wrapper for file-variable references stored in ProcessInstance.variables.
/// The JSON stored in variables has `"type": "file"` as discriminator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileReference {
    pub object_key: String,    // "file:instance-{uuid}-{varname}-{filename}"
    pub filename: String,      // "report.pdf"
    pub mime_type: String,     // "application/pdf"
    pub size_bytes: u64,       // 1245678
    pub uploaded_at: String,   // ISO 8601 timestamp
}

impl FileReference {
    /// Creates a new FileReference and generates the object_key.
    pub fn new(instance_id: Uuid, var_name: &str, filename: &str, mime_type: &str, size_bytes: u64) -> Self {
        let object_key = format!("file:{instance_id}-{var_name}-{filename}");
        Self {
            object_key,
            filename: filename.to_string(),
            mime_type: mime_type.to_string(),
            size_bytes,
            uploaded_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Converts this reference to a serde_json::Value for storage in variables.
    pub fn to_variable_value(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "file",
            "object_key": self.object_key,
            "filename": self.filename,
            "mime_type": self.mime_type,
            "size_bytes": self.size_bytes,
            "uploaded_at": self.uploaded_at
        })
    }

    /// Tries to parse a serde_json::Value as a FileReference.
    /// Returns None if the value doesn't have `"type": "file"`.
    pub fn from_variable_value(value: &serde_json::Value) -> Option<Self> {
        if value.get("type").and_then(|t| t.as_str()) == Some("file") {
            serde_json::from_value(value.clone()).ok()
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Process definition (validated at construction time)
// ---------------------------------------------------------------------------

/// An immutable, structurally validated BPMN process definition.
///
/// - `nodes`: maps each node ID → its `BpmnElement` type.
/// - `flows`: maps each source node ID → a list of outgoing `SequenceFlow`s.
///   Linear nodes have exactly one entry; gateways have two or more.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessDefinition {
    /// Unique key for this definition (UUID). Persists across re-deployments.
    pub key: Uuid,
    /// Human-readable BPMN process ID (e.g. "Process_1").
    pub id: String,
    /// Model version, increments on deploy if id corresponds.
    pub version: i32,
    pub nodes: HashMap<String, BpmnElement>,
    pub flows: HashMap<String, Vec<SequenceFlow>>,
    #[serde(default)]
    pub listeners: HashMap<String, Vec<ExecutionListener>>,
    #[serde(default)]
    pub event_listeners: Vec<ScopeEventListener>,
    #[serde(default)]
    pub sub_processes: Vec<ProcessDefinition>,
}

impl ProcessDefinition {
    /// Creates a new process definition after validating structural integrity.
    ///
    /// # Validation rules
    /// - Exactly one start event (StartEvent or TimerStartEvent) must exist.
    /// - At least one end event must exist.
    /// - All flow targets must reference existing node IDs.
    /// - Every non-end node must have at least one outgoing flow.
    /// - Gateways must have at least 2 outgoing flows.
    pub fn new(
        id: impl Into<String>,
        version: i32,
        nodes: HashMap<String, BpmnElement>,
        flows: HashMap<String, Vec<SequenceFlow>>,
        listeners: HashMap<String, Vec<ExecutionListener>>,
        event_listeners: Vec<ScopeEventListener>,
        sub_processes: Vec<ProcessDefinition>,
    ) -> EngineResult<Self> {
        let id = id.into();

        // --- exactly one start event ---
        let start_count = nodes
            .values()
            .filter(|e| matches!(e, BpmnElement::StartEvent | BpmnElement::TimerStartEvent(_) | BpmnElement::MessageStartEvent { .. }))
            .count();

        if start_count == 0 {
            return Err(EngineError::InvalidDefinition(
                "No start event defined".into(),
            ));
        }
        if start_count > 1 {
            return Err(EngineError::InvalidDefinition(
                "Multiple start events are not supported".into(),
            ));
        }

        // --- at least one end event ---
        let end_count = nodes
            .values()
            .filter(|e| matches!(e, BpmnElement::EndEvent | BpmnElement::ErrorEndEvent { .. }))
            .count();
        if end_count == 0 {
            return Err(EngineError::InvalidDefinition(
                "No end event defined".into(),
            ));
        }

        // --- all flow targets reference existing nodes ---
        for (from, targets) in &flows {
            if !nodes.contains_key(from) {
                return Err(EngineError::NoSuchNode(from.clone()));
            }
            for sf in targets {
                if !nodes.contains_key(&sf.target) {
                    return Err(EngineError::NoSuchNode(sf.target.clone()));
                }
            }
        }

        // --- boundary events must attach to existing tasks ---
        for (node_id, element) in &nodes {
            if let BpmnElement::BoundaryTimerEvent { attached_to, .. }
            | BpmnElement::BoundaryErrorEvent { attached_to, .. } = element
            {
                if !nodes.contains_key(attached_to) {
                    return Err(EngineError::InvalidDefinition(format!(
                        "Boundary event '{node_id}' attached to missing node '{attached_to}'"
                    )));
                }
            }
        }

        // --- every non-end node must have an outgoing flow ---
        for (node_id, element) in &nodes {
            if matches!(element, BpmnElement::EndEvent | BpmnElement::ErrorEndEvent { .. }) {
                continue;
            }
            let outgoing = flows.get(node_id).map_or(0, |v| v.len());
            if outgoing == 0 {
                return Err(EngineError::InvalidDefinition(format!(
                    "Node '{node_id}' has no outgoing sequence flow"
                )));
            }
        }

        // --- gateways must have at least 2 incoming or outgoing flows ---
        for (node_id, element) in &nodes {
            if matches!(
                element,
                BpmnElement::ExclusiveGateway { .. } | BpmnElement::InclusiveGateway | BpmnElement::ParallelGateway | BpmnElement::EventBasedGateway
            ) {
                let outgoing = flows.get(node_id).map_or(0, |v| v.len());
                let incoming = flows.values().flat_map(|f| f.iter()).filter(|sf| &sf.target == node_id).count();
                if outgoing < 2 && incoming < 2 {
                    return Err(EngineError::InvalidDefinition(format!(
                        "Gateway '{node_id}' must have at least 2 incoming or 2 outgoing flows"
                    )));
                }
            }
            
            // --- EventBasedGateway constraints ---
            if matches!(element, BpmnElement::EventBasedGateway) {
                if let Some(outgoing_flows) = flows.get(node_id) {
                    for sf in outgoing_flows {
                        if let Some(target_element) = nodes.get(&sf.target) {
                            if !matches!(target_element, BpmnElement::MessageCatchEvent { .. } | BpmnElement::TimerCatchEvent(_)) {
                                return Err(EngineError::InvalidDefinition(format!(
                                    "EventBasedGateway '{node_id}' can only connect to MessageCatchEvent or TimerCatchEvent targets. Node '{}' is invalid.",
                                    sf.target
                                )));
                            }
                        }
                    }
                }
            }
        }

        Ok(Self {
            key: Uuid::new_v4(),
            id,
            version,
            nodes,
            flows,
            listeners,
            event_listeners,
            sub_processes,
        })
    }

    /// Returns the (id, element) of the start event.
    pub fn start_event(&self) -> Option<(&str, &BpmnElement)> {
        self.nodes.iter().find_map(|(id, e)| {
            if matches!(e, BpmnElement::StartEvent | BpmnElement::TimerStartEvent(_) | BpmnElement::MessageStartEvent { .. }) {
                Some((id.as_str(), e))
            } else {
                None
            }
        })
    }

    /// Returns the element at the given node ID.
    pub fn get_node(&self, id: &str) -> Option<&BpmnElement> {
        self.nodes.get(id)
    }

    /// Returns all outgoing sequence flows from the given node.
    pub fn next_nodes(&self, from_id: &str) -> &[SequenceFlow] {
        self.flows.get(from_id).map_or(&[], |v| v.as_slice())
    }

    /// Backward-compatible helper: returns the single outgoing target for
    /// linear (non-gateway) nodes.
    ///
    /// Returns `None` if the node has no outgoing flows or if it has
    /// multiple outgoing flows (use `next_nodes` for gateways).
    pub fn next_node(&self, from_id: &str) -> Option<&str> {
        let flows = self.next_nodes(from_id);
        if flows.len() == 1 {
            Some(flows[0].target.as_str())
        } else {
            None
        }
    }

    /// Returns the number of incoming sequence flows to the given node.
    pub fn incoming_flow_count(&self, node_id: &str) -> usize {
        self.flows
            .values()
            .flat_map(|flows| flows.iter())
            .filter(|sf| sf.target == node_id)
            .count()
    }

    /// Returns true if this node is a converging gateway (join).
    pub fn is_join_gateway(&self, node_id: &str) -> bool {
        self.incoming_flow_count(node_id) >= 2
    }

    /// Returns true if this node is a splitting gateway.
    pub fn is_split_gateway(&self, node_id: &str) -> bool {
        if let Some(element) = self.nodes.get(node_id) {
            matches!(
                element,
                BpmnElement::ExclusiveGateway { .. }
                    | BpmnElement::InclusiveGateway
                    | BpmnElement::ParallelGateway
                    | BpmnElement::EventBasedGateway
            ) && self.next_nodes(node_id).len() >= 2
        } else {
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Builder helper (ergonomic construction)
// ---------------------------------------------------------------------------

/// Fluent builder for creating a `ProcessDefinition`.
pub struct ProcessDefinitionBuilder {
    id: String,
    key: Option<Uuid>,
    version: i32,
    nodes: HashMap<String, BpmnElement>,
    flows: HashMap<String, Vec<SequenceFlow>>,
    listeners: HashMap<String, Vec<ExecutionListener>>,
    event_listeners: Vec<ScopeEventListener>,
    sub_processes: Vec<ProcessDefinition>,
}

impl ProcessDefinitionBuilder {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            key: None,
            version: 1,
            nodes: HashMap::new(),
            flows: HashMap::new(),
            listeners: HashMap::new(),
            event_listeners: Vec::new(),
            sub_processes: Vec::new(),
        }
    }

    /// Sets an explicit key for the definition (used during restore).
    pub fn with_key(mut self, key: Uuid) -> Self {
        self.key = Some(key);
        self
    }

    /// Sets an explicit version for the definition.
    pub fn with_version(mut self, version: i32) -> Self {
        self.version = version;
        self
    }

    /// Adds a node to the definition.
    pub fn node(mut self, id: impl Into<String>, element: BpmnElement) -> Self {
        self.nodes.insert(id.into(), element);
        self
    }

    /// Adds an unconditional sequence flow (edge) between two nodes.
    pub fn flow(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.flows
            .entry(from.into())
            .or_default()
            .push(SequenceFlow::simple(to));
        self
    }

    /// Adds a conditional sequence flow between two nodes.
    pub fn conditional_flow(
        mut self,
        from: impl Into<String>,
        to: impl Into<String>,
        condition: impl Into<String>,
    ) -> Self {
        self.flows
            .entry(from.into())
            .or_default()
            .push(SequenceFlow::conditional(to, condition));
        self
    }

    /// Adds an execution listener to a specific node.
    pub fn listener(mut self, node_id: impl Into<String>, event: ListenerEvent, script: impl Into<String>) -> Self {
        self.listeners
            .entry(node_id.into())
            .or_default()
            .push(ExecutionListener {
                event,
                script: script.into(),
            });
        self
    }

    /// Adds a scope event listener.
    pub fn scope_event(mut self, listener: ScopeEventListener) -> Self {
        self.event_listeners.push(listener);
        self
    }

    /// Adds an embedded sub-process definition.
    pub fn sub_process(mut self, sub_process: ProcessDefinition) -> Self {
        self.sub_processes.push(sub_process);
        self
    }

    /// Builds and validates the definition.
    pub fn build(self) -> EngineResult<ProcessDefinition> {
        let mut def = ProcessDefinition::new(
            self.id,
            self.version,
            self.nodes,
            self.flows,
            self.listeners,
            self.event_listeners,
            self.sub_processes,
        )?;
        if let Some(key) = self.key {
            def.key = key;
        }
        Ok(def)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_definition_with_builder() {
        let def = ProcessDefinitionBuilder::new("p1")
            .node("start", BpmnElement::StartEvent)
            .node("svc", BpmnElement::ServiceTask { topic: "do_it".into() })
            .node("end", BpmnElement::EndEvent)
            .flow("start", "svc")
            .flow("svc", "end")
            .build();
        assert!(def.is_ok());
    }

    #[test]
    fn rejects_missing_start() {
        let def = ProcessDefinitionBuilder::new("p1")
            .node("end", BpmnElement::EndEvent)
            .build();
        assert!(matches!(
            def,
            Err(EngineError::InvalidDefinition(msg)) if msg.contains("No start event")
        ));
    }

    #[test]
    fn rejects_missing_end() {
        let def = ProcessDefinitionBuilder::new("p1")
            .node("start", BpmnElement::StartEvent)
            .flow("start", "nowhere")
            .build();
        assert!(def.is_err());
    }

    #[test]
    fn rejects_dangling_flow() {
        let def = ProcessDefinitionBuilder::new("p1")
            .node("start", BpmnElement::StartEvent)
            .node("end", BpmnElement::EndEvent)
            .flow("start", "end")
            .flow("end", "ghost")
            .build();
        assert!(matches!(def, Err(EngineError::NoSuchNode(id)) if id == "ghost"));
    }

    #[test]
    fn rejects_node_without_outgoing_flow() {
        let def = ProcessDefinitionBuilder::new("p1")
            .node("start", BpmnElement::StartEvent)
            .node("orphan", BpmnElement::ServiceTask { topic: "noop".into() })
            .node("end", BpmnElement::EndEvent)
            .flow("start", "end")
            .build();
        assert!(matches!(
            def,
            Err(EngineError::InvalidDefinition(msg)) if msg.contains("orphan")
        ));
    }

    #[test]
    fn find_node_and_next_work() {
        let def = ProcessDefinitionBuilder::new("p1")
            .node("start", BpmnElement::StartEvent)
            .node("svc", BpmnElement::ServiceTask { topic: "action".into() })
            .node("end", BpmnElement::EndEvent)
            .flow("start", "svc")
            .flow("svc", "end")
            .build()
            .unwrap();

        assert_eq!(def.get_node("svc"), Some(&BpmnElement::ServiceTask { topic: "action".into() }));
        assert_eq!(def.next_node("start"), Some("svc"));
        assert_eq!(def.next_node("end"), None);
    }

    #[test]
    fn token_creation() {
        let token = Token::new("start");
        assert_eq!(token.current_node, "start");
        assert!(token.variables.is_empty());
    }

    #[test]
    fn token_is_merged_survives_serialization() {
        let mut token = Token::new("gw_join");
        token.is_merged = true;
        let json = serde_json::to_string(&token).unwrap();
        let restored: Token = serde_json::from_str(&json).unwrap();
        assert!(restored.is_merged, "is_merged must survive roundtrip");
    }

    #[test]
    fn timer_start_event_definition() {
        let def = ProcessDefinitionBuilder::new("timer")
            .node("ts", BpmnElement::TimerStartEvent(Duration::from_secs(5)))
            .node("end", BpmnElement::EndEvent)
            .flow("ts", "end")
            .build();
        assert!(def.is_ok());
    }

    // --- Gateway-specific tests ---

    #[test]
    fn exclusive_gateway_definition() {
        let def = ProcessDefinitionBuilder::new("xor")
            .node("start", BpmnElement::StartEvent)
            .node("gw", BpmnElement::ExclusiveGateway { default: Some("end2".into()) })
            .node("end1", BpmnElement::EndEvent)
            .node("end2", BpmnElement::EndEvent)
            .flow("start", "gw")
            .conditional_flow("gw", "end1", "approved == true")
            .flow("gw", "end2")
            .build();
        assert!(def.is_ok());
    }

    #[test]
    fn inclusive_gateway_definition() {
        let def = ProcessDefinitionBuilder::new("or")
            .node("start", BpmnElement::StartEvent)
            .node("gw", BpmnElement::InclusiveGateway)
            .node("end1", BpmnElement::EndEvent)
            .node("end2", BpmnElement::EndEvent)
            .flow("start", "gw")
            .conditional_flow("gw", "end1", "notify_email == true")
            .conditional_flow("gw", "end2", "notify_sms == true")
            .build();
        assert!(def.is_ok());
    }

    #[test]
    fn gateway_rejects_single_outgoing() {
        let def = ProcessDefinitionBuilder::new("bad")
            .node("start", BpmnElement::StartEvent)
            .node("gw", BpmnElement::ExclusiveGateway { default: None })
            .node("end", BpmnElement::EndEvent)
            .flow("start", "gw")
            .flow("gw", "end")
            .build();
        assert!(matches!(
            def,
            Err(EngineError::InvalidDefinition(msg)) if msg.contains("at least 2")
        ));
    }
    
    #[test]
    fn event_based_gateway_rejects_non_catch_targets() {
        let def = ProcessDefinitionBuilder::new("bad_gw")
            .node("start", BpmnElement::StartEvent)
            .node("gw", BpmnElement::EventBasedGateway)
            .node("task", BpmnElement::ServiceTask { topic: "noop".into() })
            .node("catch", BpmnElement::TimerCatchEvent(Duration::from_secs(5)))
            .node("end", BpmnElement::EndEvent)
            .flow("start", "gw")
            .flow("gw", "task")
            .flow("gw", "catch")
            .flow("task", "end")
            .flow("catch", "end")
            .build();
            
        assert!(matches!(
            def,
            Err(EngineError::InvalidDefinition(msg)) if msg.contains("EventBasedGateway") && msg.contains("can only connect to MessageCatchEvent or TimerCatchEvent")
        ));
    }

    #[test]
    fn conditional_flow_builder() {
        let def = ProcessDefinitionBuilder::new("cond")
            .node("start", BpmnElement::StartEvent)
            .node("gw", BpmnElement::ExclusiveGateway { default: None })
            .node("a", BpmnElement::EndEvent)
            .node("b", BpmnElement::EndEvent)
            .flow("start", "gw")
            .conditional_flow("gw", "a", "x == 1")
            .conditional_flow("gw", "b", "x == 2")
            .build()
            .unwrap();

        let flows = def.next_nodes("gw");
        assert_eq!(flows.len(), 2);
        assert_eq!(flows[0].condition, Some("x == 1".into()));
        assert_eq!(flows[1].condition, Some("x == 2".into()));
    }

    #[test]
    fn next_nodes_returns_multiple() {
        let def = ProcessDefinitionBuilder::new("multi")
            .node("start", BpmnElement::StartEvent)
            .node("gw", BpmnElement::InclusiveGateway)
            .node("a", BpmnElement::EndEvent)
            .node("b", BpmnElement::EndEvent)
            .node("c", BpmnElement::EndEvent)
            .flow("start", "gw")
            .conditional_flow("gw", "a", "x > 0")
            .conditional_flow("gw", "b", "y > 0")
            .conditional_flow("gw", "c", "z > 0")
            .build()
            .unwrap();

        assert_eq!(def.next_nodes("gw").len(), 3);
        // next_node returns None for multi-out nodes
        assert_eq!(def.next_node("gw"), None);
    }

    #[test]
    fn test_file_reference_roundtrip() {
        let instance_id = Uuid::new_v4();
        let file_ref = FileReference::new(instance_id, "contract", "contract.pdf", "application/pdf", 1024 * 500);
        
        let value = file_ref.to_variable_value();
        
        assert_eq!(value.get("type").unwrap().as_str().unwrap(), "file");
        assert_eq!(value.get("filename").unwrap().as_str().unwrap(), "contract.pdf");
        
        let restored = FileReference::from_variable_value(&value).unwrap();
        assert_eq!(file_ref, restored);
    }

    #[test]
    fn test_get_file_reference_returns_none_for_non_file() {
        let not_a_file = serde_json::json!({
            "type": "string",
            "value": "hello"
        });
        
        let result = FileReference::from_variable_value(&not_a_file);
        assert!(result.is_none());
        
        // Also just a string
        let just_a_str = serde_json::json!("file");
        assert!(FileReference::from_variable_value(&just_a_str).is_none());
    }
}
