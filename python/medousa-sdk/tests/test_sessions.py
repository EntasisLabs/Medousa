from __future__ import annotations

from datetime import datetime, timezone

import pytest
from mock_transport import MockTransport

from medousa import MedousaClient
from medousa.types import DeriveSessionRequest


@pytest.mark.asyncio
async def test_history_page_reuses_history_route_with_cursor_query():
    authority = f"auth_{'a' * 64}"
    response = {
        "authority_id": authority,
        "session_id": "session-1",
        "turns": [],
        "next_cursor": "1",
    }
    transport = MockTransport(
        {
            (
                "GET",
                "/v1/sessions/session-1/history?limit=24&cursor=25",
            ): lambda *_a, **_k: response,
        },
    )
    client = MedousaClient("http://127.0.0.1:7419", transport=transport)

    result = await client.sessions().history_page("session-1", 24, "25")

    assert result.next_cursor == "1"


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


@pytest.mark.asyncio
async def test_derive_session_sends_idempotency_header_and_decodes_provenance():
    authority = f"auth_{'a' * 64}"
    source_id = "source-session"
    target_id = "target-session"
    created_at = datetime.now(timezone.utc).isoformat()
    response = {
        "authority_id": authority,
        "session_id": target_id,
        "catalog": "single",
        "display_name": "Branch",
        "reused": False,
        "derivation": {
            "derivation_id": f"drv_{'d' * 32}",
            "target_session": {
                "authority_id": authority,
                "session_id": target_id,
            },
            "manifest": {
                "manifest_id": f"ctx_{'c' * 32}",
                "sources": [
                    {
                        "selection": {
                            "session": {
                                "authority_id": authority,
                                "session_id": source_id,
                            },
                            "through_entry_seq": 3,
                        },
                        "selection_digest": "sha256:selection",
                    },
                ],
                "created_by": "profile:user:test",
                "created_at": created_at,
            },
            "intent": "fork",
            "created_by": "profile:user:test",
            "created_at": created_at,
        },
    }
    transport = MockTransport(
        {("POST", "/v1/sessions/derive"): lambda *_a, **_k: response},
    )
    client = MedousaClient("http://127.0.0.1:7419", transport=transport)
    request = DeriveSessionRequest(
        sources=[
            {
                "session": {"authority_id": authority, "session_id": source_id},
                "through_entry_seq": 3,
            },
        ],
        intent="fork",
        target={"catalog": "single", "display_name": "Branch"},
    )

    result = await client.sessions().derive(request, "derive-test-1")

    assert result.session_id == target_id
    assert result.derivation.manifest.sources[0].selection.through_entry_seq == 3
    assert transport.calls[0][2]["headers"] == {"Idempotency-Key": "derive-test-1"}
