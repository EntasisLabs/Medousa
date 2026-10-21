import pytest
from mock_transport import MockTransport
from medousa import MedousaClient, MedousaClientSync
from medousa.types import CreativeHistoryRequest


def view():
    return {"document": {"id": "project_one", "revision": 4, "title": "Release", "pinned": False,
            "created_at": "now", "updated_at": "now", "assets": {},
            "scene": {"schema": "medousa-create", "version": 3, "background": "transparent", "nodes": []}},
            "applied_revision": 4, "duplicate": True, "can_undo": True, "can_redo": False}


@pytest.mark.asyncio
async def test_creative_sdk_decodes_and_preserves_history_request_identity():
    transport = MockTransport({
        ("GET", "/v1/create/projects/project_one"): lambda *_: view(),
        ("POST", "/v1/create/projects/project_one/history"): lambda *_: view(),
    })
    client = MedousaClient("http://workshop", transport=transport)
    assert (await client.creative().get("project_one")).document.revision == 4
    request = CreativeHistoryRequest(id="undo_one", expected_revision=3, redo=False)
    assert (await client.creative().history("project_one", request)).duplicate
    assert transport.calls[1][2]["id"] == "undo_one"
    assert transport.calls[1][2]["expected_revision"] == 3


def test_sync_creative_sdk_uses_its_bound_transport(monkeypatch):
    class Transport:
        def get_json(self, base_url, path):
            assert base_url == "http://workshop"
            assert path == "/v1/create/projects/project_one"
            return view()
    client = MedousaClientSync("http://workshop")
    monkeypatch.setattr(client, "_transport", Transport())
    assert client.creative().get("project_one").document.revision == 4
