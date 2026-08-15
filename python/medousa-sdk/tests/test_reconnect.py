import json

import httpx
import pytest

from medousa import MedousaClient
from medousa.reconnect import (
    BackoffPolicy,
    OverlapGuard,
    ReconnectPolicy,
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


async def test_v2_reconnect_negotiates_cursor_and_dedupes_replay():
    content = {
        "schema_version": 2,
        "turn_id": "turn-1",
        "seq": 1,
        "emitted_at_utc": "2026-08-14T00:00:00Z",
        "event": {"type": "content_append", "text": "Hel"},
    }
    final = {
        "schema_version": 2,
        "turn_id": "turn-1",
        "seq": 2,
        "emitted_at_utc": "2026-08-14T00:00:01Z",
        "event": {"type": "final", "text": "Hello"},
    }

    class V2Transport:
        def __init__(self) -> None:
            self.calls: list[tuple[str, str]] = []

        async def stream_sse_with_accept(
            self, _base_url: str, path: str, accept: str
        ) -> httpx.Response:
            self.calls.append((path, accept))
            events = [content] if len(self.calls) == 1 else [content, final]
            body = "".join(f"data: {json.dumps(event)}\n\n" for event in events)
            return httpx.Response(200, content=body.encode())

    transport = V2Transport()
    client = MedousaClient("http://127.0.0.1:7419", transport=transport)  # type: ignore[arg-type]
    policy = ReconnectPolicy(backoff=BackoffPolicy(base_ms=0, max_ms=0))

    events = [
        event
        async for event in client.interactive().stream_reconnecting_v2(
            "/v1/interactive/turn/turn-1/stream",
            policy=policy,
        )
    ]

    assert [event.seq for event in events] == [1, 2]
    assert transport.calls == [
        ("/v1/interactive/turn/turn-1/stream", "text/event-stream; medousa-version=2"),
        ("/v1/interactive/turn/turn-1/stream?since=1", "text/event-stream; medousa-version=2"),
    ]


async def test_v2_reconnect_exhausts_without_sequence_progress():
    class EmptyTransport:
        async def stream_sse_with_accept(
            self, _base_url: str, _path: str, _accept: str
        ) -> httpx.Response:
            return httpx.Response(200, content=b"")

    client = MedousaClient(
        "http://127.0.0.1:7419",
        transport=EmptyTransport(),  # type: ignore[arg-type]
    )
    policy = ReconnectPolicy(
        backoff=BackoffPolicy(base_ms=0, max_ms=0, max_attempts=1)
    )

    with pytest.raises(RuntimeError, match="attempts exhausted"):
        async for _event in client.interactive().stream_reconnecting_v2(
            "/v1/interactive/turn/turn-1/stream",
            policy=policy,
        ):
            pass
