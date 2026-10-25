import pytest
from mock_transport import MockTransport

from medousa import MedousaClient, MedousaClientSync
from medousa.types import CreativeHistoryRequest


def view():
    return {"document": {"id": "project_one", "revision": 4, "title": "Release", "pinned": False,
            "created_at": "now", "updated_at": "now", "assets": {},
            "scene": {
                "schema": "medousa-create",
                "version": 3,
                "background": "transparent",
                "nodes": [],
            }},
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


def creative_job():
    return {
        "request": {
            "id": "retry_same",
            "expected_revision": 3,
            "selected_ids": ["frame"],
            "prompt": "Add a title",
            "refine_proposal": False,
        },
        "document_id": "doc",
        "context": {
            "document_id": "doc",
            "revision": 3,
            "selected_ids": ["frame"],
            "nodes": [],
            "assets": [],
            "fingerprint": "digest",
        },
        "status": "interrupted",
        "session_id": "ses_create_test",
        "turn_id": "creative_job_test",
        "created_at": "now",
        "updated_at": "now",
        "result": None,
        "assets": [],
        "error": "Interrupted",
        "applied_revision": None,
    }


@pytest.mark.asyncio
async def test_creative_jobs_preserve_admission_identity_and_explicit_decisions():
    from medousa.types import CreativeJobRequest, CreativeProposalDecision
    transport = MockTransport({
        ("POST", "/v1/create/projects/doc/jobs"): lambda *_: creative_job(),
        ("GET", "/v1/create/projects/doc/jobs"): lambda *_: {"jobs": [creative_job()]},
        ("GET", "/v1/create/projects/doc/jobs/retry_same"): lambda *_: creative_job(),
        ("POST", "/v1/create/projects/doc/jobs/retry_same/decision"): lambda *_: {
            **creative_job(),
            "status": "cancelled",
        },
    })
    client = MedousaClient("http://workshop", transport=transport)
    request = CreativeJobRequest(**creative_job()["request"])
    assert (await client.creative().create_job("doc", request)).status.value == "interrupted"
    assert len((await client.creative().jobs("doc")).jobs) == 1
    assert (await client.creative().job("doc", "retry_same")).context.revision == 3
    decision = CreativeProposalDecision(accept=False, expected_revision=3)
    decided = await client.creative().decide_job("doc", "retry_same", decision)
    assert decided.status.value == "cancelled"
    assert transport.calls[0][2] == creative_job()["request"]
    assert transport.calls[-1][2] == {"accept": False, "expected_revision": 3}


def test_sync_creative_jobs_use_bound_transport(monkeypatch):
    class Transport:
        def get_json(self, base_url, path):
            assert base_url == "http://workshop"
            assert path == "/v1/create/projects/doc/jobs/retry_same"
            return creative_job()
    client = MedousaClientSync("http://workshop")
    monkeypatch.setattr(client, "_transport", Transport())
    assert client.creative().job("doc", "retry_same").status.value == "interrupted"


@pytest.mark.asyncio
async def test_code_handoff_preserves_the_frame_visual_revision_and_session():
    from medousa.types import CreativeCodeContextRequest
    transport = MockTransport({
        ("POST", "/v1/create/projects/doc/code-context"): lambda *_: {
            "context": {"document_id": "doc", "revision": 7, "selected_ids": ["frame"],
                        "nodes": [], "assets": [], "fingerprint": "frozen"}, "snapshot_id": "frozen_context", "media_refs": []},
    })
    client = MedousaClient("http://workshop", transport=transport)
    request = CreativeCodeContextRequest(
        expected_revision=7,
        frame_id="frame",
        visual_asset_id="visual",
        session_id="code_thread",
    )
    result = await client.creative().code_context("doc", request)
    assert result.context.revision == 7
    assert result.snapshot_id == "frozen_context"
    assert transport.calls[0][2] == request.model_dump(mode="json")
