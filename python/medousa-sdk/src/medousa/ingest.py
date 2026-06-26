from __future__ import annotations

from medousa._decode import decode
from medousa.client import MedousaClient
from medousa.types import IngestRequest, IngestResponse


class IngestApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def post(self, request: IngestRequest) -> IngestResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            "/v1/ingest",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(IngestResponse, value)
