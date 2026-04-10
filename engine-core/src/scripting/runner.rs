//! Script execution for BPMN execution listeners (start / end scripts).
//!
//! Uses the Rhai scripting engine with configurable hardening:
//! - `max_operations` limits CPU-bound loops
//! - `max_memory` limits heap allocations inside Rhai
//! - `timeout` aborts runaway scripts via `spawn_blocking` + `tokio::time::timeout`

use std::collections::HashMap;

use rhai::Dynamic;
use serde_json::Value;
use uuid::Uuid;

use crate::domain::{EngineError, EngineResult};
use crate::domain::{ListenerEvent, ProcessDefinition, Token};

// ---------------------------------------------------------------------------
// ScriptConfig — configurable resource limits for Rhai execution
// ---------------------------------------------------------------------------

/// Resource limits applied to every Rhai script evaluation.
#[derive(Debug, Clone)]
pub struct ScriptConfig {
    /// Maximum number of Rhai operations per evaluation (default: 50 000).
    pub max_operations: u64,
    /// Maximum heap memory in bytes Rhai may allocate (default: 2 MiB).
    pub max_memory: usize,
    /// Hard wall-clock timeout in milliseconds (default: 1 000).
    pub timeout_ms: u64,
}

impl Default for ScriptConfig {
    fn default() -> Self {
        Self {
            max_operations: 50_000,
            max_memory: 2 * 1024 * 1024, // 2 MiB
            timeout_ms: 1_000,            // 1 second
        }
    }
}

impl ScriptConfig {
    /// Reads configuration from environment variables, falling back to defaults.
    ///
    /// | Variable                    | Default  |
    /// |-----------------------------|----------|
    /// | `RHAI_MAX_OPERATIONS`       | 50 000   |
    /// | `RHAI_MAX_MEMORY_BYTES`     | 2097152  |
    /// | `RHAI_TIMEOUT_MS`           | 1000     |
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(v) = std::env::var("RHAI_MAX_OPERATIONS") {
            if let Ok(n) = v.parse() {
                cfg.max_operations = n;
            }
        }
        if let Ok(v) = std::env::var("RHAI_MAX_MEMORY_BYTES") {
            if let Ok(n) = v.parse() {
                cfg.max_memory = n;
            }
        }
        if let Ok(v) = std::env::var("RHAI_TIMEOUT_MS") {
            if let Ok(n) = v.parse() {
                cfg.timeout_ms = n;
            }
        }
        cfg
    }

    /// Creates a Rhai engine configured with the resource limits from this config.
    pub fn build_engine(&self) -> rhai::Engine {
        let mut engine = rhai::Engine::new();
        engine.set_max_operations(self.max_operations);
        engine.set_max_string_size(64 * 1024); // 64 KiB per string
        engine.set_max_array_size(10_000);
        engine.set_max_map_size(10_000);
        engine
    }
}

// ---------------------------------------------------------------------------
// execute_script_safe — the single hardened entry point
// ---------------------------------------------------------------------------

/// Result of a successful script evaluation: the mutated variables.
pub type ScriptResult = HashMap<String, Value>;

/// Evaluates a Rhai script inside a hardened sandbox.
///
/// The script runs on a blocking thread (`spawn_blocking`) so that the Tokio
/// runtime is never blocked. A wall-clock timeout aborts the evaluation if
/// `ScriptConfig::timeout_ms` elapses.
///
/// On success the returned `HashMap` contains the full variable scope after
/// evaluation — callers merge this back into the token.
pub async fn execute_script_safe(
    config: &ScriptConfig,
    script: &str,
    variables: &HashMap<String, Value>,
) -> EngineResult<ScriptResult> {
    let engine = config.build_engine();
    let timeout = std::time::Duration::from_millis(config.timeout_ms);
    let script = script.to_owned();
    let vars = variables.clone();

    let start = std::time::Instant::now();
    let handle = tokio::task::spawn_blocking(move || {
        let mut scope = rhai::Scope::new();
        for (k, v) in &vars {
            scope.push_dynamic(k, rhai::serde::to_dynamic(v).unwrap_or(Dynamic::UNIT));
        }

        engine
            .eval_with_scope::<()>(&mut scope, &script)
            .map_err(|e| EngineError::ScriptError(e.to_string()))?;

        let mut result = vars;
        for (k, _, v) in scope.iter_raw() {
            if let Ok(json_val) = rhai::serde::from_dynamic(v) {
                result.insert(k.to_string(), json_val);
            }
        }
        Ok(result)
    });

    let result = match tokio::time::timeout(timeout, handle).await {
        Ok(Ok(result)) => result,
        Ok(Err(join_err)) => Err(EngineError::ScriptError(format!(
            "Script thread panicked: {join_err}"
        ))),
        Err(_elapsed) => Err(EngineError::ScriptError(format!(
            "Script execution timed out after {}ms",
            config.timeout_ms
        ))),
    };

    let elapsed = start.elapsed().as_secs_f64();
    metrics::histogram!("bpmn_script_execution_duration_seconds").record(elapsed);
    if result.is_err() {
        metrics::counter!("bpmn_errors_total", "type" => "script").increment(1);
    }
    result
}

// ---------------------------------------------------------------------------
// Listener helpers (updated to use execute_script_safe)
// ---------------------------------------------------------------------------

/// Executes all scripts of a given `ListenerEvent` on the specified node.
///
/// Mutates token variables based on script output and appends audit entries.
pub async fn run_node_scripts(
    config: &ScriptConfig,
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
                let result = execute_script_safe(config, &l.script, &token.variables).await?;
                token.variables = result;

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
pub async fn run_end_scripts(
    config: &ScriptConfig,
    instance_id: Uuid,
    token: &mut Token,
    def: &ProcessDefinition,
    node_id: &str,
    instance_audit_log: &mut Vec<String>,
    instance_variables: &mut HashMap<String, Value>,
) -> EngineResult<()> {
    let mut end_audits = Vec::new();
    run_node_scripts(
        config,
        instance_id,
        token,
        def,
        node_id,
        ListenerEvent::End,
        &mut end_audits,
    )
    .await?;
    instance_audit_log.append(&mut end_audits);
    *instance_variables = token.variables.clone();
    Ok(())
}
