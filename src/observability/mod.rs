//! Phase 3 — structured tracing, log hygiene, and dead-letter bounds.
//!
//! * [`tracing_setup`] — init the process-wide `tracing` subscriber (fmt by default;
//!   optional OTLP export via `MEDOUSA_OTEL_ENABLED` + `--features otel-export`).
//! * [`correlation`] — turn-span helpers threading `correlation_id` from [`TurnEnvelope`].
//! * [`rate_limit`] — dedupe repetitive log lines (proxy errors, scheduler ticks).
//! * [`log_rotation`] — size-capped rotation for append-only file sinks.
//! * [`dead_letter`] — bounded dead-letter pileup enforcement.

pub mod correlation;
pub mod dead_letter;
pub mod log_rotation;
pub mod rate_limit;
pub mod tracing_setup;

pub use correlation::{correlation_span, enter_turn_span, turn_span};
pub use dead_letter::{
    dead_letter_cap, enforce_dead_letter_cap, DeadLetterCapMetrics, DeadLetterCapReport,
};
pub use log_rotation::{rotate_if_oversized, RotateConfig};
pub use rate_limit::{rate_limited_debug, rate_limited_error, rate_limited_warn};
pub use tracing_setup::{init_tracing, init_tracing_from_env, tracing_status_line};
