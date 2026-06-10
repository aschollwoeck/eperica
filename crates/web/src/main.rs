//! Eperica web — the server entrypoint (HTTP/UI), wiring the application and infrastructure layers.
//!
//! Slice 001 establishes the foundation incrementally; the HTTP server, configuration, and
//! persistence are added in later tasks (T3–T16). This file currently sets up structured tracing.
#![forbid(unsafe_code)]

use tracing_subscriber::EnvFilter;

fn main() {
    init_tracing();
    tracing::info!("Eperica web starting (slice 001 scaffolding).");
}

/// Initialize structured logging/tracing, reading the filter from `RUST_LOG` (default: `info`).
///
/// Observability is wired from the start so latency is measurable per the constitution (P11).
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
