use serde::{Deserialize, Serialize};
use crate::domain::{TimerDefinition, MultiInstanceDef};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BpmnElement {
    /// A plain (none) start event — the process starts immediately.
    StartEvent,
    /// A timer-triggered start event that fires after the given duration.
    TimerStartEvent(TimerDefinition),
    /// An end event — the process terminates here.
    EndEvent,
    /// A terminate end event that immediately kills all active tokens.
    TerminateEndEvent,
    /// A service task that pauses the workflow and must be fetched and completed by remote workers.
    ServiceTask {
        topic: String,
        multi_instance: Option<MultiInstanceDef>,
    },
    /// A user task assigned to a specific role or user.
    UserTask(String),
    /// A script task that executes a Rhai script inline and automatically advances.
    ScriptTask {
        script: String,
        multi_instance: Option<MultiInstanceDef>,
    },
    /// A send task / intermediate message throw that publishes a message and auto-advances.
    SendTask {
        message_name: String,
        multi_instance: Option<MultiInstanceDef>,
    },
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
    /// A complex gateway — custom logic to decide when to split and join tokens.
    ComplexGateway {
        join_condition: Option<String>,
        default: Option<String>,
    },
    /// A timer intermediate catch event that pauses the token until the duration elapses.
    TimerCatchEvent(TimerDefinition),
    /// A boundary timer event attached to an activity.
    BoundaryTimerEvent {
        attached_to: String,
        timer: TimerDefinition,
        cancel_activity: bool,
    },
    /// A start event triggered by a named message.
    MessageStartEvent { message_name: String },
    /// An intermediate catch event waiting for a named message.
    MessageCatchEvent { message_name: String },
    /// A boundary message event attached to an activity.
    BoundaryMessageEvent {
        attached_to: String,
        message_name: String,
        cancel_activity: bool,
    },
    /// A boundary error event attached to an activity.
    BoundaryErrorEvent {
        attached_to: String,
        error_code: Option<String>,
    },
    /// An end event that throws a specific BPMN error.
    ErrorEndEvent { error_code: String },
    /// An end event that throws a BPMN escalation (non-fatal, propagates to parent scope).
    EscalationEndEvent { escalation_code: String },
    /// An intermediate throw event that fires an escalation signal.
    EscalationThrowEvent { escalation_code: String },
    /// A boundary escalation event attached to an activity.
    BoundaryEscalationEvent {
        attached_to: String,
        escalation_code: Option<String>,
        cancel_activity: bool,
    },
    /// An intermediate throw event that triggers compensation.
    CompensationThrowEvent { activity_ref: Option<String> },
    /// An end event that triggers compensation before completing.
    CompensationEndEvent { activity_ref: Option<String> },
    /// A boundary compensation event attached to an activity (registered on successful completion).
    BoundaryCompensationEvent { attached_to: String },
    /// A Call Activity that invokes another globally deployed process definition.
    CallActivity { called_element: String },
    /// An Embedded Sub-Process acting as a nested scope within the same instance.
    EmbeddedSubProcess { start_node_id: String },
    /// Internal end event of a Sub-Process, signaling completion to the parent scope.
    SubProcessEndEvent { sub_process_id: String },
}

// ---------------------------------------------------------------------------
// Scope Event Listeners (Event Sub-Processes)
// ---------------------------------------------------------------------------

