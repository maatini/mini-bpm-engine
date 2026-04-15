//! Engine-interne Event-Typen für Push-Benachrichtigungen.
//!
//! Der `WorkflowEngine` hält einen `tokio::sync::broadcast::Sender<EngineEvent>`.
//! Subscriber (z.B. der SSE-Handler im engine-server) erhalten via
//! `engine.subscribe_events()` einen eigenen Receiver.

use serde::Serialize;

/// Grobgranulare Ereignisse, die die Engine bei Zustandsänderungen aussendet.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineEvent {
    /// Eine Prozessinstanz wurde gestartet, hat einen Schritt ausgeführt oder ist abgeschlossen.
    InstanceChanged,
    /// Ein User Task oder Service Task wurde erstellt oder abgeschlossen.
    TaskChanged,
    /// Eine Prozessdefinition wurde deployed oder gelöscht.
    DefinitionChanged,
}
