from medousa.reconnect import (
    BackoffPolicy,
    OverlapGuard,
    apply_stream_seq,
    stream_path_with_since,
)
from medousa.types import InteractiveTurnStreamEvent


def test_stream_path_with_since():
    assert stream_path_with_since("/v1/interactive/turn/t1/stream", 0) == (
        "/v1/interactive/turn/t1/stream"
    )
    assert stream_path_with_since("/v1/interactive/turn/t1/stream", 42) == (
        "/v1/interactive/turn/t1/stream?since=42"
    )
    assert stream_path_with_since("/v1/interactive/turn/t1/stream?since=1", 99) == (
        "/v1/interactive/turn/t1/stream?since=99"
    )


def test_apply_stream_seq_dedupes():
    event = InteractiveTurnStreamEvent.model_construct(
        turn_id="t1",
        seq=2,
        event_type="status",
        phase="running",
        message="",
        terminal=False,
        emitted_at_utc="2026-01-01T00:00:00Z",
    )
    last, keep = apply_stream_seq(1, event)
    assert keep is True
    assert last == 2
    last, keep = apply_stream_seq(last, event)
    assert keep is False


def test_backoff_caps():
    policy = BackoffPolicy()
    assert policy.delay(10) <= policy.max_ms


def test_overlap_guard():
    guard = OverlapGuard()
    assert guard.try_enter() is True
    assert guard.try_enter() is False
    guard.release()
    assert guard.try_enter() is True
