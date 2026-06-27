from __future__ import annotations

import pytest
from mock_transport import MockTransport

from medousa import MedousaClient
from medousa.types import IngestRequest

INGEST_RESPONSE = {
    "session_id": "sess-1",
    "job_id": "job-1",
    "reply": "hello",
    "is_new_session": True,
    "stream_ready": False,
}


@pytest.mark.asyncio
async def test_ingest_post():
    def handler(_base, _path, body):
        assert body["channel"] == "test"
        return INGEST_RESPONSE

    transport = MockTransport({("POST", "/v1/ingest"): handler})
    client = MedousaClient("http://127.0.0.1:7419", transport=transport)

    response = await client.ingest().post(
        IngestRequest(channel="test", user_id="u1", channel_id="c1", text="hi"),
    )

    assert response.session_id == "sess-1"
    assert response.reply == "hello"
