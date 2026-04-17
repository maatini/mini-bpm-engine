pub mod adapter;
pub mod condition;
pub mod domain;
pub mod engine;
pub mod history;
pub mod port;
pub mod runtime;
pub mod scripting;

pub use condition::evaluate_condition;

// Backward-compatible re-exports (existing code doesn't break)
pub use domain::*;
pub use engine::WorkflowEngine;
pub use history::{HistoryDiff, HistoryEntry, HistoryEventType, VariableDiff};
pub use port::*;
pub use runtime::*;
pub use scripting::*;

// Legacy module aliases for downstream crates
// TODO: Remove once all downstream crates are migrated to new paths
pub use domain as model;
pub use domain::timer as timer_definition;
pub use port as persistence;
