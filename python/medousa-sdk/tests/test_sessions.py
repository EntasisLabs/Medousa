from __future__ import annotations

from datetime import datetime, timezone

import pytest
from mock_transport import MockTransport

from medousa import MedousaClient


@pytest.mark.asyncio
async def test_search_transcripts_encodes_query_and_decodes_hits():
    timestamp = datetime.now(timezone.utc)
    response = {
        "query": "phoenix project",
        "hits": [
            {
                "session_id": "sess-1",
                "display_name": "Launch notes",
                "role": "assistant",
                "timestamp": timestamp.isoformat(),
                "excerpt": "The Phoenix project ships tomorrow.",
            },
        ],
    }
    transport = MockTransport(
        {
            (
                "GET",
                "/v1/sessions/search?q=phoenix+project&limit=12",
            ): lambda *_a, **_k: response,
        },
    )
    client = MedousaClient("http://127.0.0.1:7419", transport=transport)

    result = await client.sessions().search_transcripts("phoenix project", 12)

    assert result.query == "phoenix project"
    assert result.hits[0].session_id == "sess-1"
    assert result.hits[0].timestamp == timestamp
