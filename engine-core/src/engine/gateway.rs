use crate::condition::evaluate_condition;
use crate::engine::types::NextAction;
use crate::error::{EngineError, EngineResult};
use crate::model::{ProcessDefinition, Token};

pub(crate) fn execute_parallel_gateway(
    def: &ProcessDefinition,
    current_id: &str,
    token: &mut Token,
) -> EngineResult<NextAction> {
    let outgoing = def.next_nodes(current_id);
    let incoming_count = def.incoming_flow_count(current_id);

    if incoming_count >= 2 && !token.is_merged {
        // --- JOIN LOGIC ---
        return Ok(NextAction::WaitForJoin {
            gateway_id: current_id.to_string(),
            token: token.clone(),
        });
    }
    token.is_merged = false;

    // --- SPLIT LOGIC ---
    if outgoing.len() == 1 {
        token.current_node = outgoing[0].target.clone();
        return Ok(NextAction::Continue(token.clone()));
    }

    let forked: Vec<Token> = outgoing
        .iter()
        .map(|sf| Token::with_variables(&sf.target, token.variables.clone()))
        .collect();

    Ok(NextAction::ContinueMultiple(forked))
}

pub(crate) fn execute_exclusive_gateway(
    def: &ProcessDefinition,
    current_id: &str,
    token: &mut Token,
    default: &Option<String>,
) -> EngineResult<NextAction> {
    let outgoing = def.next_nodes(current_id);
    let mut chosen_target: Option<String> = None;

    // Evaluate conditions in order; first match wins
    for sf in outgoing {
        if let Some(ref cond) = sf.condition
            && evaluate_condition(cond, &token.variables) {
                chosen_target = Some(sf.target.clone());
                break;
            }
    }

    // Fallback to default flow if no condition matched
    if chosen_target.is_none()
        && let Some(default_target) = default {
            chosen_target = Some(default_target.clone());
        }

    let target =
        chosen_target.ok_or_else(|| EngineError::NoMatchingCondition(current_id.to_string()))?;

    token.current_node = target.clone();
    Ok(NextAction::Continue(token.clone()))
}

pub(crate) fn execute_inclusive_gateway(
    def: &ProcessDefinition,
    current_id: &str,
    token: &mut Token,
) -> EngineResult<NextAction> {
    let outgoing = def.next_nodes(current_id);
    let incoming_count = def.incoming_flow_count(current_id);

    if incoming_count >= 2 && !token.is_merged {
        // --- JOIN LOGIC ---
        return Ok(NextAction::WaitForJoin {
            gateway_id: current_id.to_string(),
            token: token.clone(),
        });
    }
    token.is_merged = false;

    // --- SPLIT LOGIC ---
    let mut matched_targets: Vec<String> = Vec::new();

    // Evaluate all conditions; every match is taken
    for sf in outgoing {
        if let Some(ref cond) = sf.condition {
            if evaluate_condition(cond, &token.variables) {
                matched_targets.push(sf.target.clone());
            }
        } else {
            // Unconditional flows are always taken
            matched_targets.push(sf.target.clone());
        }
    }

    if matched_targets.is_empty() {
        return Err(EngineError::NoMatchingCondition(current_id.to_string()));
    }

    if matched_targets.len() == 1 {
        token.current_node = matched_targets[0].clone();
        return Ok(NextAction::Continue(token.clone()));
    }

    // Fork tokens — each gets a copy of the current variables
    let forked: Vec<Token> = matched_targets
        .into_iter()
        .map(|target| Token::with_variables(&target, token.variables.clone()))
        .collect();

    Ok(NextAction::ContinueMultiple(forked))
}

pub(crate) fn execute_complex_gateway(
    def: &ProcessDefinition,
    current_id: &str,
    token: &mut Token,
    default: &Option<String>,
) -> EngineResult<NextAction> {
    let outgoing = def.next_nodes(current_id);
    let incoming_count = def.incoming_flow_count(current_id);

    if incoming_count >= 2 && !token.is_merged {
        // --- JOIN LOGIC ---
        // Basic join handling goes to `arrive_at_join`. Complex join logic with condition
        // is evaluated inside `arrive_at_join` because we need all arrived tokens' merged variables.
        return Ok(NextAction::WaitForJoin {
            gateway_id: current_id.to_string(),
            token: token.clone(),
        });
    }
    token.is_merged = false;

    // --- SPLIT LOGIC ---
    let mut matched_targets: Vec<String> = Vec::new();

    // Evaluate all conditions; every match is taken (like Inclusive)
    for sf in outgoing {
        if let Some(ref cond) = sf.condition {
            if evaluate_condition(cond, &token.variables) {
                matched_targets.push(sf.target.clone());
            }
        } else {
            // Unconditional flow: Is it the default flow?
            if let Some(d) = default {
                if sf.target == *d {
                    continue; // Skip the default flow during normal evaluation
                }
            }
            matched_targets.push(sf.target.clone());
        }
    }

    if matched_targets.is_empty() {
        if let Some(default_target) = default {
            matched_targets.push(default_target.clone());
        } else {
            return Err(EngineError::NoMatchingCondition(current_id.to_string()));
        }
    }

    if matched_targets.len() == 1 {
        token.current_node = matched_targets[0].clone();
        return Ok(NextAction::Continue(token.clone()));
    }

    // Fork tokens
    let forked: Vec<Token> = matched_targets
        .into_iter()
        .map(|target| Token::with_variables(&target, token.variables.clone()))
        .collect();

    Ok(NextAction::ContinueMultiple(forked))
}
