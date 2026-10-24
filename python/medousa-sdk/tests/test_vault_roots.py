from urllib.parse import parse_qs, urlsplit

import pytest
from medousa import MedousaClient, MedousaClientSync


def response():
    return {"query": "日記 & notes", "root_id": "vault:work & ideas", "hits": []}


def check_route(path):
    parts = urlsplit(path)
    assert parts.path == "/v1/vault/search"
    assert parse_qs(parts.query) == {
        "root_id": ["vault:work & ideas"], "q": ["日記 & notes"], "limit": ["24"]
    }


@pytest.mark.asyncio
async def test_async_search_preserves_root_identity_and_encodes_query():
    class Transport:
        async def get_json(self, base_url, path):
            check_route(path)
            return response()

    client = MedousaClient("http://workshop", transport=Transport())
    result = await client.vault().search(root_id="vault:work & ideas", q="日記 & notes", limit=24)
    assert result.root_id == "vault:work & ideas"


def test_sync_search_preserves_root_identity_and_encodes_query(monkeypatch):
    class Transport:
        def get_json(self, base_url, path):
            check_route(path)
            return response()

    client = MedousaClientSync("http://workshop")
    monkeypatch.setattr(client, "_transport", Transport())
    result = client.vault().search(root_id="vault:work & ideas", q="日記 & notes", limit=24)
    assert result.root_id == "vault:work & ideas"
