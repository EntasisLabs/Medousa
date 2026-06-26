from __future__ import annotations

from datetime import datetime, timezone

import pytest

from medousa import MedousaClient
from mock_transport import MockTransport

HEALTH_PAYLOAD = {
    "status": "ok",
    "backend": "sqlite",
    "worker_id": "w1",
    "now_utc": datetime.now(timezone.utc).isoformat(),
    "agent_runtime_version": "centralized-v1",
    "tool_registry_count": 3,
}


@pytest.mark.asyncio
async def test_health_get():
    transport = MockTransport({("GET", "/health"): lambda *_: HEALTH_PAYLOAD})
    client = MedousaClient("http://127.0.0.1:7419", transport=transport)

    health = await client.health().get()

    assert health.status == "ok"
    assert health.backend == "sqlite"
    assert transport.calls[0] == ("GET", "/health", None)
