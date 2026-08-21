from __future__ import annotations

from datetime import datetime, timezone

import pytest
from mock_transport import MockTransport

from medousa import MedousaClient
from medousa.types import CreatePromptStashRequest


@pytest.mark.asyncio
async def test_prompt_stash_lifecycle_uses_typed_routes():
    created_at = datetime.now(timezone.utc).isoformat()
    stash = {
        "stash_id": f"pst_{'a' * 32}",
        "label": "Follow up",
        "draft": {"text": "ask this next", "mode": "general"},
        "created_by": "user:local",
        "created_at": created_at,
        "updated_at": created_at,
    }
    transport = MockTransport(
        {
            ("GET", "/v1/prompt-stashes"): lambda *_a, **_k: {"stashes": [stash]},
            ("POST", "/v1/prompt-stashes"): lambda *_a, **_k: stash,
            ("DELETE", f"/v1/prompt-stashes/pst_{'a' * 32}"): lambda *_a, **_k: {
                "stash_id": f"pst_{'a' * 32}",
                "deleted": True,
            },
        },
    )
    client = MedousaClient("http://127.0.0.1:7419", transport=transport)
    request = CreatePromptStashRequest(
        label="Follow up",
        draft={"text": "ask this next", "mode": "general"},
    )

    listed = await client.prompt_stashes().list()
    created = await client.prompt_stashes().create(request)
    deleted = await client.prompt_stashes().delete(str(created.stash_id.root))

    assert listed.stashes[0].draft.text == "ask this next"
    assert created.label == "Follow up"
    assert deleted.deleted is True
    assert [call[:2] for call in transport.calls] == [
        ("GET", "/v1/prompt-stashes"),
        ("POST", "/v1/prompt-stashes"),
        ("DELETE", f"/v1/prompt-stashes/pst_{'a' * 32}"),
    ]
