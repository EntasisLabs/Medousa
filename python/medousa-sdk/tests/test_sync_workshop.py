from __future__ import annotations

from medousa import MedousaClientSync
from medousa.transport import WorkshopTransport


def test_workshop_transport_bearer_header():
    transport = WorkshopTransport(bearer_token="secret-token")
    assert transport._headers["Authorization"] == "Bearer secret-token"


def test_sync_client_health(monkeypatch):
    calls: list[tuple[str, str]] = []

    class FakeSyncTransport:
        def get_json(self, base_url, path):
            calls.append((base_url, path))
            return {
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
                "now_utc": "2026-01-01T00:00:00Z",
            }

        def close(self):
            pass

    client = MedousaClientSync("http://127.0.0.1:7419")
    monkeypatch.setattr(client, "_transport", FakeSyncTransport())

    health = client.health().get()
    assert health.status == "ok"
    assert calls == [("http://127.0.0.1:7419", "/v1/health")]
