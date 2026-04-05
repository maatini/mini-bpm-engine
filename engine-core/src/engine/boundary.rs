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
            duration,
            ..
        } = node
        {
            if attached_to == attached_node_id {
                bounds.push((node_id.clone(), *duration));
            }
        }
    }

    for (node_id, duration) in bounds {
        let pending = PendingTimer {
            id: Uuid::new_v4(),
            instance_id,
            node_id,
            expires_at: Utc::now()
                + chrono::Duration::from_std(duration).unwrap_or(chrono::Duration::seconds(0)),
            token_id: token.id,
        };
        pending_timers.push(pending);
    }

    pending_timers
}
