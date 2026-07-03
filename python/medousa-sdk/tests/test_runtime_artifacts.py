from __future__ import annotations

from datetime import datetime, timezone

import pytest
from mock_transport import MockTransport

from medousa import MedousaClient
from medousa.types import (
    ArtifactDeleteRequest,
    ArtifactFetchRequest,
    ArtifactListUiRequest,
    ArtifactWriteRequest,
)

FETCH_RESPONSE = {
    "artifact_id": "art-1",
    "mime": "text/html",
    "label": "Chart",
    "body": "<html></html>",
    "byte_size": 13,
}

WRITE_RESPONSE = {
    "artifact_id": "art-1",
    "mime": "text/html",
    "label": "Chart",
    "byte_size": 13,
}

DELETE_RESPONSE = {
    "artifact_id": "art-1",
    "deleted": True,
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
            ("POST", "/v1/runtime/artifact/write"): lambda *_a, **_k: WRITE_RESPONSE,
            ("POST", "/v1/runtime/artifact/delete"): lambda *_a, **_k: DELETE_RESPONSE,
            ("POST", "/v1/runtime/artifact/list-ui"): lambda *_a, **_k: LIST_RESPONSE,
        },
    )
    client = MedousaClient("http://127.0.0.1:7419", transport=transport)

    fetched = await client.runtime().artifact_fetch(
        ArtifactFetchRequest(session_id="sess-1", artifact_id="art-1"),
    )
    assert fetched.body == "<html></html>"

    written = await client.runtime().artifact_write(
        ArtifactWriteRequest(
            session_id="sess-1",
            artifact_id="art-1",
            mime="text/html",
            label="Chart",
            body="<html></html>",
        ),
    )
    assert written.artifact_id == "art-1"

    deleted = await client.runtime().artifact_delete(
        ArtifactDeleteRequest(session_id="sess-1", artifact_id="art-1"),
    )
    assert deleted.deleted is True

    listed = await client.runtime().artifact_list_ui(ArtifactListUiRequest(session_id="sess-1"))
    assert len(listed.artifacts) == 1
