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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    #[allow(dead_code)]
    pub id: Uuid,
    pub current_node: String,
    pub variables: HashMap<String, Value>,
}

impl Token {
    /// Creates a new token positioned at the given node with empty variables.
    pub fn new(start_node: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            current_node: start_node.to_string(),
            variables: HashMap::new(),
        }
    }

    /// Creates a new token with pre-populated variables.
    #[allow(dead_code)]
    pub fn with_variables(start_node: &str, variables: HashMap<String, Value>) -> Self {
        Self {
            id: Uuid::new_v4(),
            current_node: start_node.to_string(),
            variables,
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
    pub nodes: HashMap<String, BpmnElement>,
    pub flows: HashMap<String, Vec<SequenceFlow>>,
    #[serde(default)]
    pub listeners: HashMap<String, Vec<ExecutionListener>>,
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
        nodes: HashMap<String, BpmnElement>,
        flows: HashMap<String, Vec<SequenceFlow>>,
        listeners: HashMap<String, Vec<ExecutionListener>>,
    ) -> EngineResult<Self> {
        let id = id.into();

        // --- exactly one start event ---
        let start_count = nodes
            .values()
            .filter(|e| matches!(e, BpmnElement::StartEvent | BpmnElement::TimerStartEvent(_)))
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
            .filter(|e| matches!(e, BpmnElement::EndEvent))
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

        // --- every non-end node must have an outgoing flow ---
        for (node_id, element) in &nodes {
            if matches!(element, BpmnElement::EndEvent) {
                continue;
            }
            let outgoing = flows.get(node_id).map_or(0, |v| v.len());
            if outgoing == 0 {
                return Err(EngineError::InvalidDefinition(format!(
                    "Node '{node_id}' has no outgoing sequence flow"
                )));
            }
        }

        // --- gateways must have at least 2 outgoing flows ---
        for (node_id, element) in &nodes {
            if matches!(
                element,
                BpmnElement::ExclusiveGateway { .. } | BpmnElement::InclusiveGateway
            ) {
                let outgoing = flows.get(node_id).map_or(0, |v| v.len());
                if outgoing < 2 {
                    return Err(EngineError::InvalidDefinition(format!(
                        "Gateway '{node_id}' must have at least 2 outgoing flows, has {outgoing}"
                    )));
                }
            }
        }

        Ok(Self { key: Uuid::new_v4(), id, nodes, flows, listeners })
    }

    /// Returns the (id, element) of the start event.
    pub fn start_event(&self) -> Option<(&str, &BpmnElement)> {
        self.nodes.iter().find_map(|(id, e)| {
            if matches!(e, BpmnElement::StartEvent | BpmnElement::TimerStartEvent(_)) {
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
}

// ---------------------------------------------------------------------------
// Builder helper (ergonomic construction)
// ---------------------------------------------------------------------------

/// Fluent builder for creating a `ProcessDefinition`.
pub struct ProcessDefinitionBuilder {
    id: String,
    key: Option<Uuid>,
    nodes: HashMap<String, BpmnElement>,
    flows: HashMap<String, Vec<SequenceFlow>>,
    listeners: HashMap<String, Vec<ExecutionListener>>,
}

impl ProcessDefinitionBuilder {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            key: None,
            nodes: HashMap::new(),
            flows: HashMap::new(),
            listeners: HashMap::new(),
        }
    }

    /// Sets an explicit key for the definition (used during restore).
    pub fn with_key(mut self, key: Uuid) -> Self {
        self.key = Some(key);
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

    /// Builds and validates the definition.
    pub fn build(self) -> EngineResult<ProcessDefinition> {
        let mut def = ProcessDefinition::new(self.id, self.nodes, self.flows, self.listeners)?;
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
}
