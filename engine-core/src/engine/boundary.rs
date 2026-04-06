use chrono::Utc;
use uuid::Uuid;

use crate::engine::types::PendingTimer;
use crate::model::{BpmnElement, ProcessDefinition, Token};

/// Scans the process definition for boundary events attached to the given `node_id`
/// and creates the corresponding pending timers or other wait states.
pub(crate) fn setup_boundary_events(
    def: &ProcessDefinition,
    attached_node_id: &str,
    instance_id: Uuid,
    token: &Token,
) -> Vec<PendingTimer> {
    let mut pending_timers = Vec::new();

    let mut bounds = Vec::new();
    for (node_id, node) in &def.nodes {
        if let BpmnElement::BoundaryTimerEvent {
            attached_to,
            timer,
            ..
        } = node
        {
            if attached_to == attached_node_id {
                bounds.push((node_id.clone(), timer.clone()));
            }
        }
    }

    for (node_id, timer_def) in bounds {
        let now = Utc::now();
        let expires_at = timer_def.next_expiry(now).unwrap_or(now);
        let pending = PendingTimer {
            id: Uuid::new_v4(),
            instance_id,
            node_id,
            expires_at,
            token_id: token.id,
            timer_def: Some(timer_def),
            remaining_repetitions: None,
        };
        pending_timers.push(pending);
    }

    pending_timers
}
