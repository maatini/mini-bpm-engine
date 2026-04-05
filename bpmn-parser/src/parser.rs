use std::collections::HashMap;
use std::time::Duration;

use quick_xml::de::from_str;

use engine_core::error::{EngineError, EngineResult};
use engine_core::model::{BpmnElement, ListenerEvent, ProcessDefinition, ProcessDefinitionBuilder};

use crate::models::{BpmnDefinitions, BpmnExtensionElements};

/// Helper to attach parsed listeners to the builder
fn add_listeners(
    mut builder: ProcessDefinitionBuilder,
    node_id: &str,
    ext_elements: Option<BpmnExtensionElements>,
) -> ProcessDefinitionBuilder {
    if let Some(exts) = ext_elements {
        for l in exts.execution_listeners {
            let evt = match l.event.as_str() {
                "start" => ListenerEvent::Start,
                "end" => ListenerEvent::End,
                _ => continue,
            };
            if let Some(s) = l.script {
                builder = builder.listener(node_id, evt, s.content.trim());
            }
        }
    }
    builder
}

/// Parse ISO 8601 time-duration (PT subset: hours, minutes, seconds).
///
/// Supported formats: `PT5S`, `PT1H30M`, `PT10M`, `PT1H`.
/// Returns `Err` for invalid input (empty, no PT prefix, unknown units).
fn parse_iso8601_duration(s: &str) -> EngineResult<Duration> {
    let s = s.trim();
    if s.is_empty() {
        return Err(EngineError::InvalidDefinition(
            "Timer duration is empty".to_string(),
        ));
    }
    if !s.starts_with("PT") {
        return Err(EngineError::InvalidDefinition(format!(
            "Invalid ISO 8601 duration '{}': must start with 'PT'",
            s
        )));
    }
    let body = &s[2..];
    if body.is_empty() {
        return Err(EngineError::InvalidDefinition(format!(
            "Invalid ISO 8601 duration '{}': no value after 'PT'",
            s
        )));
    }

    let mut total_secs: u64 = 0;
    let mut current_num = String::new();

    for c in body.chars() {
        if c.is_ascii_digit() {
            current_num.push(c);
        } else {
            if current_num.is_empty() {
                return Err(EngineError::InvalidDefinition(format!(
                    "Invalid ISO 8601 duration '{}': missing number before '{}'",
                    s, c
                )));
            }
            let val: u64 = current_num.parse().map_err(|_| {
                EngineError::InvalidDefinition(format!("Invalid number in duration '{}'", s))
            })?;
            match c {
                'H' => total_secs += val * 3600,
                'M' => total_secs += val * 60,
                'S' => total_secs += val,
                other => {
                    return Err(EngineError::InvalidDefinition(format!(
                        "Invalid ISO 8601 duration '{}': unknown unit '{}'",
                        s, other
                    )));
                }
            }
            current_num.clear();
        }
    }

    // Trailing digits without unit (e.g. "PT5")
    if !current_num.is_empty() {
        return Err(EngineError::InvalidDefinition(format!(
            "Invalid ISO 8601 duration '{}': trailing digits without unit (H/M/S)",
            s
        )));
    }

    Ok(Duration::from_secs(total_secs))
}

/// Parses a subset of BPMN 2.0 XML and builds a `ProcessDefinition`.
///
/// Note: Since `quick-xml` expects exact structure, the parsed XML must match
/// the structs above (elements rather than attributes where specified, etc.).
pub fn parse_bpmn_xml(xml: &str) -> EngineResult<ProcessDefinition> {
    let mut defs: BpmnDefinitions = from_str(xml).map_err(|e| {
        EngineError::InvalidDefinition(format!(
            "BPMN XML parsing failed (hint: ensure the XML contains a valid <process> element and supported entities): {}",
            e
        ))
    })?;

    if defs.processes.is_empty() {
        return Err(EngineError::InvalidDefinition(
            "No <process> element found in BPMN XML".to_string(),
        ));
    }

    // Find the executable process, or fallback to the first one available
    let executable_idx = defs
        .processes
        .iter()
        .position(|p| p.is_executable == Some(true))
        .unwrap_or(0);
    let process = defs.processes.remove(executable_idx);

    let process_id = process.id.clone();
    let mut builder = ProcessDefinitionBuilder::new(process_id.clone());

    // Separate event sub-processes from regular embedded sub-processes.
    // Event sub-processes (triggeredByEvent="true") are scope-level handlers
    // that will be supported in a future release — skip them gracefully.
    // Regular embedded sub-processes are not yet supported and cause an error.
    let has_regular_subprocess = process
        .sub_processes
        .iter()
        .any(|sp| sp.triggered_by_event != Some(true));
    if has_regular_subprocess {
        return Err(EngineError::InvalidDefinition(
            "Embedded subprocesses are not yet supported. Please use flat processes or event sub-processes (triggeredByEvent=\"true\").".to_string(),
        ));
    }
    if !process.sub_processes.is_empty() {
        tracing::info!(
            "Process '{}': skipping {} event sub-process(es) (not yet executed, but parsing succeeds)",
            process_id,
            process.sub_processes.len()
        );
    }

    // Lookup maps for messages and errors
    let message_lookup: HashMap<String, String> = defs
        .messages
        .iter()
        .map(|m| (m.id.clone(), m.name.clone()))
        .collect();

    let error_lookup: HashMap<String, String> = defs
        .errors
        .iter()
        .map(|e| {
            let code = e
                .error_code
                .clone()
                .or_else(|| e.name.clone())
                .unwrap_or_else(|| e.id.clone());
            (e.id.clone(), code)
        })
        .collect();

    // Process-level listeners
    builder = add_listeners(builder, &process_id, process.extension_elements);

    // 1. Process Start Events
    for start in process.start_events {
        let node_id = start.id.clone();
        if let Some(timer) = start.timer_event_definition {
            let dur = if let Some(time) = timer.time_duration {
                parse_iso8601_duration(&time)?
            } else {
                Duration::from_secs(0)
            };
            builder = builder.node(start.id, BpmnElement::TimerStartEvent(dur));
        } else if let Some(msg) = start.message_event_definition {
            let message_name = msg
                .message_ref
                .and_then(|ref_id| message_lookup.get(&ref_id).cloned())
                .unwrap_or_else(|| "generic_message".into());
            builder = builder.node(start.id, BpmnElement::MessageStartEvent { message_name });
        } else {
            builder = builder.node(start.id, BpmnElement::StartEvent);
        }
        builder = add_listeners(builder, &node_id, start.extension_elements);
    }

    // 2. Process End Events
    for end in process.end_events {
        let node_id = end.id.clone();
        if let Some(err) = end.error_event_definition {
            let error_code = err
                .error_ref
                .and_then(|ref_id| error_lookup.get(&ref_id).cloned())
                .unwrap_or_else(|| "generic_error".into());
            builder = builder.node(end.id, BpmnElement::ErrorEndEvent { error_code });
        } else {
            builder = builder.node(end.id, BpmnElement::EndEvent);
        }
        builder = add_listeners(builder, &node_id, end.extension_elements);
    }

    // 3. Process Service Tasks (All now use external fetching API via topics)
    for task in process.service_tasks {
        let node_id = task.id.clone();
        // Fallback: use topic, then handler (backward compat), then node_id
        let topic = task
            .topic
            .or(task.handler)
            .unwrap_or_else(|| task.id.clone());
        builder = builder.node(task.id, BpmnElement::ServiceTask { topic });
        builder = add_listeners(builder, &node_id, task.extension_elements);
    }

    // 4. Process User Tasks
    for task in process.user_tasks {
        let node_id = task.id.clone();
        let assignee = task.assignee.unwrap_or_else(|| "unassigned".into());
        builder = builder.node(task.id, BpmnElement::UserTask(assignee));
        builder = add_listeners(builder, &node_id, task.extension_elements);
    }

    // 5. Generic tasks (bpmn-js default task element)
    //    Also covers scriptTask, sendTask, receiveTask, manualTask,
    //    businessRuleTask, and callActivity — all map to ServiceTask.
    let all_generic_tasks = process
        .generic_tasks
        .into_iter()
        .chain(process.script_tasks)
        .chain(process.send_tasks)
        .chain(process.receive_tasks)
        .chain(process.manual_tasks)
        .chain(process.business_rule_tasks)
        .chain(process.call_activities);

    for task in all_generic_tasks {
        let node_id = task.id.clone();
        let topic = task.name.unwrap_or_else(|| task.id.clone());
        builder = builder.node(task.id, BpmnElement::ServiceTask { topic });
        builder = add_listeners(builder, &node_id, task.extension_elements);
    }

    // 6. Build a flow lookup (flow-ID → target-ref) for resolving the
    //    `default` attribute on exclusive gateways.
    let flow_lookup: HashMap<String, String> = process
        .sequence_flows
        .iter()
        .map(|f| (f._id.clone(), f.target_ref.clone()))
        .collect();

    // 6a. Exclusive gateways — resolve `default` flow ID → target node ID
    for gw in process.exclusive_gateways {
        let node_id = gw.id.clone();
        let default_target = gw
            .default
            .and_then(|flow_id| flow_lookup.get(&flow_id).cloned());
        builder = builder.node(
            gw.id,
            BpmnElement::ExclusiveGateway {
                default: default_target,
            },
        );
        builder = add_listeners(builder, &node_id, gw.extension_elements);
    }

    // 6b. Inclusive gateways
    for gw in process.inclusive_gateways {
        let node_id = gw.id.clone();
        builder = builder.node(gw.id, BpmnElement::InclusiveGateway);
        builder = add_listeners(builder, &node_id, gw.extension_elements);
    }

    // 6c. Parallel gateways
    for gw in process.parallel_gateways {
        let node_id = gw.id.clone();
        builder = builder.node(gw.id, BpmnElement::ParallelGateway);
        builder = add_listeners(builder, &node_id, gw.extension_elements);
    }

    // 6d. Event-based gateways
    for gw in process.event_based_gateways {
        let node_id = gw.id.clone();
        builder = builder.node(gw.id, BpmnElement::EventBasedGateway);
        builder = add_listeners(builder, &node_id, gw.extension_elements);
    }

    // 7. Intermediate catch events
    for catch_evt in process.intermediate_catch_events {
        let node_id = catch_evt.id.clone();
        if let Some(timer) = catch_evt.timer_event_definition {
            let dur = if let Some(time) = timer.time_duration {
                parse_iso8601_duration(&time)?
            } else {
                Duration::from_secs(0)
            };
            builder = builder.node(catch_evt.id, BpmnElement::TimerCatchEvent(dur));
        } else if let Some(msg) = catch_evt.message_event_definition {
            let message_name = msg
                .message_ref
                .and_then(|ref_id| message_lookup.get(&ref_id).cloned())
                .unwrap_or_else(|| "generic_message".into());
            builder = builder.node(
                catch_evt.id,
                BpmnElement::MessageCatchEvent { message_name },
            );
        } else {
            // generic pass through
            builder = builder.node(
                catch_evt.id,
                BpmnElement::ServiceTask {
                    topic: "event_passthrough".into(),
                },
            );
        }
        builder = add_listeners(builder, &node_id, catch_evt.extension_elements);
    }

    // 8. Intermediate throw events
    for evt in process.intermediate_throw_events {
        let node_id = evt.id.clone();
        builder = builder.node(
            evt.id,
            BpmnElement::ServiceTask {
                topic: "event_passthrough".into(),
            },
        );
        builder = add_listeners(builder, &node_id, evt.extension_elements);
    }

    // 9. Boundary Events
    for bd in process.boundary_events {
        let node_id = bd.id.clone();
        let attached_to = bd.attached_to_ref.clone();
        // cancelActivity is true by default
        let cancel_activity = bd.cancel_activity.unwrap_or(true);

        if let Some(timer) = bd.timer_event_definition {
            let dur = if let Some(time) = timer.time_duration {
                parse_iso8601_duration(&time)?
            } else {
                Duration::from_secs(0)
            };
            builder = builder.node(
                bd.id,
                BpmnElement::BoundaryTimerEvent {
                    attached_to,
                    duration: dur,
                    cancel_activity,
                },
            );
        } else if let Some(err) = bd.error_event_definition {
            let error_code = err
                .error_ref
                .and_then(|ref_id| error_lookup.get(&ref_id).cloned());
            builder = builder.node(
                bd.id,
                BpmnElement::BoundaryErrorEvent {
                    attached_to,
                    error_code,
                },
            );
        } else {
            // Unhandled boundary event, map to noop
            builder = builder.node(
                bd.id,
                BpmnElement::ServiceTask {
                    topic: "noop".into(),
                },
            );
        }
        builder = add_listeners(builder, &node_id, None);
    }

    // 10. Process Sequence Flows
    for flow in process.sequence_flows {
        if let Some(cond) = flow.condition_expression {
            builder = builder.conditional_flow(flow.source_ref, flow.target_ref, cond.value.trim());
        } else {
            builder = builder.flow(flow.source_ref, flow.target_ref);
        }
    }

    builder.build()
}
