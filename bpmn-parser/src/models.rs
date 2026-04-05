use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnExtensionElements {
    #[serde(rename = "executionListener", default)]
    pub execution_listeners: Vec<BpmnExecutionListener>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnExecutionListener {
    #[serde(rename = "@event")]
    pub event: String,
    
    #[serde(rename = "script")]
    pub script: Option<BpmnScript>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnScript {
    #[serde(rename = "@scriptFormat")]
    #[allow(dead_code)]
    pub script_format: String,
    
    #[serde(rename = "$value")]
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnDefinitions {
    #[allow(dead_code)]
    #[serde(rename = "@id", default)]
    pub id: Option<String>,
    
    #[serde(rename = "process", default)]
    pub processes: Vec<BpmnProcess>,

    #[allow(dead_code)]
    #[serde(rename = "collaboration", default)]
    pub collaborations: Vec<IgnoredElement>,
    
    #[allow(dead_code)]
    #[serde(rename = "BPMNDiagram", default)]
    pub bpmndiagrams: Vec<IgnoredElement>,

    #[serde(rename = "message", default)]
    pub messages: Vec<BpmnMessage>,
    
    #[serde(rename = "error", default)]
    pub errors: Vec<BpmnErrorDef>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnMessage {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "@name")]
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnErrorDef {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "@name", default)]
    pub name: Option<String>,
    #[serde(rename = "@errorCode", default)]
    pub error_code: Option<String>,
}

/// A catch-all struct for elements we want to parse but otherwise ignore.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub(crate) struct IgnoredElement {
    #[serde(rename = "@id", default)]
    pub id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnProcess {
    #[serde(rename = "@id")]
    pub id: String,

    #[serde(rename = "@isExecutable", default)]
    pub is_executable: Option<bool>,
    
    #[serde(rename = "extensionElements")]
    pub extension_elements: Option<BpmnExtensionElements>,
    
    #[serde(rename = "startEvent", default)]
    pub start_events: Vec<BpmnStartEvent>,
    
    #[serde(rename = "endEvent", default)]
    pub end_events: Vec<BpmnEndEvent>,
    
    #[serde(rename = "serviceTask", default)]
    pub service_tasks: Vec<BpmnServiceTask>,
    
    #[serde(rename = "userTask", default)]
    pub user_tasks: Vec<BpmnUserTask>,
    
    #[serde(rename = "sequenceFlow", default)]
    pub sequence_flows: Vec<BpmnSequenceFlow>,

    // --- Ignored Visual/Data Elements ---
    #[allow(dead_code)]
    #[serde(rename = "dataObjectReference", default)]
    pub data_object_references: Vec<IgnoredElement>,
    #[allow(dead_code)]
    #[serde(rename = "dataStoreReference", default)]
    pub data_store_references: Vec<IgnoredElement>,
    #[allow(dead_code)]
    #[serde(rename = "textAnnotation", default)]
    pub text_annotations: Vec<IgnoredElement>,
    #[allow(dead_code)]
    #[serde(rename = "association", default)]
    pub associations: Vec<IgnoredElement>,

    // --- Future/Unsupported Elements ---
    #[serde(rename = "subProcess", default)]
    pub sub_processes: Vec<BpmnSubProcess>,
    
    #[serde(rename = "boundaryEvent", default)]
    pub boundary_events: Vec<BpmnBoundaryEvent>,

    /// Generic `<task>` elements (bpmn-js default when adding a task).
    #[serde(rename = "task", default)]
    pub generic_tasks: Vec<BpmnGenericTask>,

    /// Script, send, receive, manual, businessRule tasks — all map to ServiceTask.
    #[serde(rename = "scriptTask", default)]
    pub script_tasks: Vec<BpmnGenericTask>,
    #[serde(rename = "sendTask", default)]
    pub send_tasks: Vec<BpmnGenericTask>,
    #[serde(rename = "receiveTask", default)]
    pub receive_tasks: Vec<BpmnGenericTask>,
    #[serde(rename = "manualTask", default)]
    pub manual_tasks: Vec<BpmnGenericTask>,
    #[serde(rename = "businessRuleTask", default)]
    pub business_rule_tasks: Vec<BpmnGenericTask>,
    #[serde(rename = "callActivity", default)]
    pub call_activities: Vec<BpmnGenericTask>,

    /// Gateways — mapped to proper BpmnElement variants.
    #[serde(rename = "exclusiveGateway", default)]
    pub exclusive_gateways: Vec<BpmnExclusiveGateway>,
    #[serde(rename = "parallelGateway", default)]
    pub parallel_gateways: Vec<BpmnGateway>,
    #[serde(rename = "inclusiveGateway", default)]
    pub inclusive_gateways: Vec<BpmnGateway>,
    #[serde(rename = "eventBasedGateway", default)]
    pub event_based_gateways: Vec<BpmnGateway>,

    /// Intermediate events — treated as pass-through nodes.
    #[serde(rename = "intermediateThrowEvent", default)]
    pub intermediate_throw_events: Vec<BpmnGenericTask>,
    #[serde(rename = "intermediateCatchEvent", default)]
    pub intermediate_catch_events: Vec<BpmnIntermediateCatchEvent>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnStartEvent {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "extensionElements")]
    pub extension_elements: Option<BpmnExtensionElements>,
    #[serde(rename = "timerEventDefinition")]
    pub timer_event_definition: Option<BpmnTimerEventDefinition>,
    #[serde(rename = "messageEventDefinition")]
    pub message_event_definition: Option<BpmnMessageEventDefinition>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnTimerEventDefinition {
    #[serde(rename = "timeDuration")]
    pub time_duration: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnMessageEventDefinition {
    #[serde(rename = "@messageRef")]
    pub message_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnErrorEventDefinition {
    #[serde(rename = "@errorRef")]
    pub error_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnEndEvent {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "extensionElements")]
    pub extension_elements: Option<BpmnExtensionElements>,
    #[serde(rename = "errorEventDefinition")]
    pub error_event_definition: Option<BpmnErrorEventDefinition>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnServiceTask {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "extensionElements")]
    pub extension_elements: Option<BpmnExtensionElements>,
    #[serde(rename = "@data-handler")]
    pub handler: Option<String>,
    #[serde(rename = "@data-topic")]
    pub topic: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnUserTask {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "extensionElements")]
    pub extension_elements: Option<BpmnExtensionElements>,
    #[serde(rename = "@data-assignee")]
    pub assignee: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnConditionExpression {
    #[serde(rename = "$value")]
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnSequenceFlow {
    #[serde(rename = "@id")]
    pub _id: String,
    #[serde(rename = "@sourceRef")]
    pub source_ref: String,
    #[serde(rename = "@targetRef")]
    pub target_ref: String,
    #[serde(rename = "conditionExpression")]
    pub condition_expression: Option<BpmnConditionExpression>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnGenericTask {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "extensionElements")]
    pub extension_elements: Option<BpmnExtensionElements>,
    #[serde(rename = "@name", default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnExclusiveGateway {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "extensionElements")]
    pub extension_elements: Option<BpmnExtensionElements>,
    #[serde(rename = "@default", default)]
    pub default: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnGateway {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "extensionElements")]
    pub extension_elements: Option<BpmnExtensionElements>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnIntermediateCatchEvent {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "extensionElements")]
    pub extension_elements: Option<BpmnExtensionElements>,
    #[serde(rename = "timerEventDefinition")]
    pub timer_event_definition: Option<BpmnTimerEventDefinition>,
    #[serde(rename = "messageEventDefinition")]
    pub message_event_definition: Option<BpmnMessageEventDefinition>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnBoundaryEvent {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "@attachedToRef")]
    pub attached_to_ref: String,
    #[serde(rename = "@cancelActivity", default)]
    pub cancel_activity: Option<bool>,
    #[serde(rename = "timerEventDefinition")]
    pub timer_event_definition: Option<BpmnTimerEventDefinition>,
    #[serde(rename = "errorEventDefinition")]
    pub error_event_definition: Option<BpmnErrorEventDefinition>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BpmnSubProcess {
    #[allow(dead_code)]
    #[serde(rename = "@id")]
    pub id: String,
}
