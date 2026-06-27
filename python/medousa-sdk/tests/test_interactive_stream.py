from __future__ import annotations

from datetime import datetime, timezone

from mock_transport import MockTransport

from medousa import MedousaClient
from medousa.types import InteractiveTurnRequest

TURN_RESPONSE = {
    "turn_id": "turn-1",
    "accepted_at_utc": datetime.now(timezone.utc).isoformat(),
    "stream_url": "/v1/interactive/turn/turn-1/stream",
    "stream_ready": True,
    "fallback_to_local": False,
}

EVENT_JSON = (
    '{"turn_id":"turn-1","event_type":"delta","phase":"responding",'
    '"message":"","content_delta":"Hi","terminal":false,'
    f'"emitted_at_utc":"{datetime.now(timezone.utc).isoformat()}"}}'
)


async def test_interactive_stream_turn(monkeypatch):
    async def fake_iter(_response):
        yield EVENT_JSON

    monkeypatch.setattr("medousa.interactive.iter_sse_events", fake_iter)

    transport = MockTransport(
        {
            ("POST", "/v1/interactive/turn"): lambda *_a, **_k: TURN_RESPONSE,
            ("SSE", "/v1/interactive/turn/turn-1/stream"): lambda *_a, **_k: object(),
        },
    )
    client = MedousaClient("http://127.0.0.1:7419", transport=transport)

    async with client.interactive().stream_turn(
        InteractiveTurnRequest(
            session_id="s1",
            prompt="Hello",
            model="gpt-4",
            provider="openai",
            persist_user_turn=True,
            response_depth_mode="standard",
            stage_routing={},
        ),
    ) as events:
        collected = [event async for event in events]

    assert len(collected) == 1
    assert collected[0].content_delta == "Hi"
    assert collected[0].terminal is False
