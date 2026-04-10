use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use crate::domain::{EngineError, EngineResult};
use crate::domain::{BpmnElement, SequenceFlow, ExecutionListener, ScopeEventListener, ListenerEvent};

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
            .filter(|e| {
                matches!(
                    e,
                    BpmnElement::StartEvent
                        | BpmnElement::TimerStartEvent(_)
                        | BpmnElement::MessageStartEvent { .. }
                )
            })
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
            .filter(|e| {
                matches!(
                    e,
                    BpmnElement::EndEvent
                        | BpmnElement::ErrorEndEvent { .. }
                        | BpmnElement::EscalationEndEvent { .. }
                        | BpmnElement::CompensationEndEvent { .. }
                        | BpmnElement::TerminateEndEvent
                        | BpmnElement::SubProcessEndEvent { .. }
                )
            })
            .count();
        if end_count == 0 {
            return Err(EngineError::InvalidDefinition(
                "No end event defined".into(),
            ));
        }

        // --- all flow targets reference existing nodes ---
        for (from, targets) in &flows {
            if !nodes.contains_key::<String>(from) {
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
            | BpmnElement::BoundaryMessageEvent { attached_to, .. }
            | BpmnElement::BoundaryErrorEvent { attached_to, .. }
            | BpmnElement::BoundaryEscalationEvent { attached_to, .. }
            | BpmnElement::BoundaryCompensationEvent { attached_to, .. } = element
                && !nodes.contains_key(attached_to)
            {
                return Err(EngineError::InvalidDefinition(format!(
                    "Boundary event '{node_id}' attached to missing node '{attached_to}'"
                )));
            }
        }

        // --- every non-end node must have an outgoing flow ---
        for (node_id, element) in &nodes {
            if matches!(
                element,
                BpmnElement::EndEvent
                    | BpmnElement::ErrorEndEvent { .. }
                    | BpmnElement::EscalationEndEvent { .. }
                    | BpmnElement::CompensationEndEvent { .. }
                    | BpmnElement::TerminateEndEvent
                    | BpmnElement::SubProcessEndEvent { .. }
                    | BpmnElement::EmbeddedSubProcess { .. }
                    | BpmnElement::BoundaryCompensationEvent { .. }
            ) {
                continue;
            }
            let outgoing = flows.get(node_id).map_or(0, |v: &Vec<SequenceFlow>| v.len());
            if outgoing == 0 {
                // SubProcess boundaries themselves need outgoing flows, but internal nodes act normally.
                return Err(EngineError::InvalidDefinition(format!(
                    "Node '{node_id}' has no outgoing sequence flow"
                )));
            }
        }

        // --- gateways must have at least 2 incoming or outgoing flows ---
        for (node_id, element) in &nodes {
            if matches!(
                element,
                BpmnElement::ExclusiveGateway { .. }
                    | BpmnElement::InclusiveGateway
                    | BpmnElement::ParallelGateway
                    | BpmnElement::EventBasedGateway
                    | BpmnElement::ComplexGateway { .. }
            ) {
                let outgoing = flows.get(node_id).map_or(0, |v: &Vec<SequenceFlow>| v.len());
                let incoming = flows
                    .values()
                    .flat_map(|f: &Vec<SequenceFlow>| f.iter())
                    .filter(|sf| &sf.target == node_id)
                    .count();
                if outgoing < 2 && incoming < 2 {
                    return Err(EngineError::InvalidDefinition(format!(
                        "Gateway '{node_id}' must have at least 2 incoming or 2 outgoing flows"
                    )));
                }
            }

            // --- EventBasedGateway constraints ---
            if matches!(element, BpmnElement::EventBasedGateway)
                && let Some(outgoing_flows) = flows.get(node_id)
            {
                for sf in outgoing_flows {
                    if let Some(target_element) = nodes.get(&sf.target)
                        && !matches!(
                            target_element,
                            BpmnElement::MessageCatchEvent { .. } | BpmnElement::TimerCatchEvent(_)
                        )
                    {
                        return Err(EngineError::InvalidDefinition(format!(
                            "EventBasedGateway '{node_id}' can only connect to MessageCatchEvent or TimerCatchEvent targets. Node '{}' is invalid.",
                            sf.target
                        )));
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
            if matches!(
                e,
                BpmnElement::StartEvent
                    | BpmnElement::TimerStartEvent(_)
                    | BpmnElement::MessageStartEvent { .. }
            ) {
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
        self.flows.get(from_id).map_or(&[] as &[SequenceFlow], |v| v.as_slice())
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
            .flat_map(|flows: &Vec<SequenceFlow>| flows.iter())
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
                    | BpmnElement::ComplexGateway { .. }
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
    pub fn listener(
        mut self,
        node_id: impl Into<String>,
        event: ListenerEvent,
        script: impl Into<String>,
    ) -> Self {
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
