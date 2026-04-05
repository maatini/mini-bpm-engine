//! Script execution for BPMN execution listeners (start / end scripts).
//!
//! Uses the Rhai scripting engine embedded in `WorkflowEngine` to evaluate
//! scripts attached to process nodes.

use std::collections::HashMap;

use rhai::Dynamic;
use serde_json::Value;
use uuid::Uuid;

use crate::error::{EngineError, EngineResult};
use crate::model::{ListenerEvent, ProcessDefinition, Token};

/// Executes all scripts of a given `ListenerEvent` on the specified node.
///
/// Mutates token variables based on script output and appends audit entries.
pub fn run_node_scripts(
    script_engine: &rhai::Engine,
    _instance_id: Uuid,
    token: &mut Token,
    def: &ProcessDefinition,
    node_id: &str,
    event: ListenerEvent,
    audit_log: &mut Vec<String>,
) -> EngineResult<()> {
    if let Some(listeners) = def.listeners.get(node_id) {
        for l in listeners {
            if l.event == event {
                let mut scope = rhai::Scope::new();
                for (k, v) in &token.variables {
                    scope.push_dynamic(k, rhai::serde::to_dynamic(v).unwrap_or(Dynamic::UNIT));
                }

                script_engine
                    .eval_with_scope::<()>(&mut scope, &l.script)
                    .map_err(|e| EngineError::ScriptError(e.to_string()))?;

                for (k, _, v) in scope.iter_raw() {
                    if let Ok(json_val) = rhai::serde::from_dynamic(v) {
                        token.variables.insert(k.to_string(), json_val);
                    }
                }

                let event_name = match event {
                    ListenerEvent::Start => "start",
                    ListenerEvent::End => "end",
                };
                tracing::info!(
                    "Instance {_instance_id}: executed {event_name} script on node '{node_id}'"
                );
                audit_log.push(format!("📜 Executed {event_name} script on '{node_id}'"));
            }
        }
    }
    Ok(())
}

/// Runs End-event scripts and merges the resulting variables back
/// into the instance state.
///
/// This is a convenience wrapper around `run_node_scripts` that also
/// updates the `variables` field on the `ProcessInstance`.
pub fn run_end_scripts(
    script_engine: &rhai::Engine,
    instance_id: Uuid,
    token: &mut Token,
    def: &ProcessDefinition,
    node_id: &str,
    instance_audit_log: &mut Vec<String>,
    instance_variables: &mut HashMap<String, Value>,
) -> EngineResult<()> {
    let mut end_audits = Vec::new();
    run_node_scripts(
        script_engine,
        instance_id,
        token,
        def,
        node_id,
        ListenerEvent::End,
        &mut end_audits,
    )?;
    instance_audit_log.append(&mut end_audits);
    *instance_variables = token.variables.clone();
    Ok(())
}
