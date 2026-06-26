from __future__ import annotations

from datetime import datetime, timezone

import pytest

from medousa import MedousaClient
from medousa.types import ArtifactFetchRequest, ArtifactListUiRequest
from mock_transport import MockTransport

FETCH_RESPONSE = {
    "artifact_id": "art-1",
    "mime": "text/html",
    "label": "Chart",
    "body": "<html></html>",
    "byte_size": 13,
}

LIST_RESPONSE = {
    "artifacts": [
        {
            "artifact_id": "art-1",
            "session_id": "sess-1",
            "label": "Chart",
            "byte_size": 13,
            "stored_at_utc": datetime.now(timezone.utc).isoformat(),
        },
    ],
}


@pytest.mark.asyncio
async def test_runtime_artifacts():
    transport = MockTransport(
        {
            ("POST", "/v1/runtime/artifact/fetch"): lambda *_a, **_k: FETCH_RESPONSE,
            ("POST", "/v1/runtime/artifact/list-ui"): lambda *_a, **_k: LIST_RESPONSE,
        },
    )
    client = MedousaClient("http://127.0.0.1:7419", transport=transport)

    fetched = await client.runtime().artifact_fetch(
        ArtifactFetchRequest(session_id="sess-1", artifact_id="art-1"),
    )
    assert fetched.body == "<html></html>"

    listed = await client.runtime().artifact_list_ui(ArtifactListUiRequest(session_id="sess-1"))
    assert len(listed.artifacts) == 1
