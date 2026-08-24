from __future__ import annotations

from datetime import datetime, timezone

import pytest
from mock_transport import MockTransport

from medousa import CompatibilityError, MedousaClient

HEALTH_PAYLOAD = {
    "runtime": {
        "authority_id": f"auth_{'a' * 64}",
        "product_version": "0.9.1",
        "build_revision": "test-build-42",
        "contract_revision": 1,
        "base_schema_revision": 1,
        "deployment_profile": "full",
        "deployment_target": "full:macos:aarch64",
        "advertised_capabilities": ["transport.http"],
    },
    "status": "ok",
    "backend": "sqlite",
    "worker_id": "w1",
    "now_utc": datetime.now(timezone.utc).isoformat(),
    "agent_runtime_version": "centralized-v1",
    "tool_registry_count": 3,
}


@pytest.mark.asyncio
async def test_health_get():
    transport = MockTransport({("GET", "/v1/health"): lambda *_: HEALTH_PAYLOAD})
    client = MedousaClient("http://127.0.0.1:7419", transport=transport)

    health = await client.health().get()

    assert health.status == "ok"
    assert health.backend == "sqlite"
    assert health.runtime.build_revision == "test-build-42"
    assert transport.calls[0] == ("GET", "/v1/health", None)


@pytest.mark.asyncio
async def test_health_rejects_missing_or_incompatible_runtime_identity():
    missing = MockTransport({("GET", "/v1/health"): lambda *_: {"status": "ok"}})
    client = MedousaClient("http://127.0.0.1:7419", transport=missing)
    with pytest.raises(CompatibilityError, match="omitted the required runtime descriptor"):
        await client.health().get()

    incompatible = {
        **HEALTH_PAYLOAD,
        "runtime": {**HEALTH_PAYLOAD["runtime"], "contract_revision": 2},
    }
    transport = MockTransport({("GET", "/v1/health"): lambda *_: incompatible})
    client = MedousaClient("http://127.0.0.1:7419", transport=transport)
    with pytest.raises(CompatibilityError, match="test-build-42"):
        await client.health().get()
