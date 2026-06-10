//! Scheduled events — the discrete, due-timestamped outcomes that drive the world (P1).
//!
//! Continuous processes are computed on read; *discrete* outcomes (build completion, troop arrival,
//! …) are modeled as events with a due timestamp and processed only when due.

/// A point in time as Unix-epoch **milliseconds**, UTC (P11: millisecond precision).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub i64);

/// The kind of a scheduled event. Extended in later slices; `Heartbeat` is the trivial event used to
/// prove the scheduler in slice 001.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EventKind {
    /// A no-op event used to exercise the scheduler.
    Heartbeat,
}

/// An event scheduled to be processed at or after its due timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledEvent {
    /// What should happen.
    pub kind: EventKind,
    /// When it becomes due (Unix-epoch ms, UTC).
    pub due_at: Timestamp,
}
