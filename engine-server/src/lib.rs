// Re-export the app builder so integration tests can use it.
pub mod log_buffer;
pub mod log_nats;
pub mod observability;
pub mod startup;
mod server;
pub use log_buffer::LogBuffer;
pub use log_nats::NatsLogSink;
pub use server::{build_app, build_app_with_engine};
pub use startup::StartupCoordinator;
