//! Correlation-id helpers for turn-scoped tracing spans.

use tracing::Span;

use medousa_engine::TurnEnvelope;

/// Open a span for a full turn, stamping `turn_id` and `correlation_id`.
pub fn turn_span(envelope: &TurnEnvelope) -> Span {
    tracing::info_span!(
        "turn",
        turn_id = %envelope.turn_id,
        correlation_id = %envelope.correlation_id,
        seq = envelope.seq,
    )
}

/// Open a span when only the correlation id is known (e.g. persistence writer).
pub fn correlation_span(correlation_id: &str) -> Span {
    tracing::info_span!("correlated", correlation_id = %correlation_id)
}

/// Enter a turn span and return the guard (drops on scope end).
pub fn enter_turn_span(envelope: &TurnEnvelope) -> tracing::span::EnteredSpan {
    turn_span(envelope).entered()
}
