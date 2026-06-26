from __future__ import annotations

from medousa._decode import decode
from medousa.client import MedousaClient
from medousa.types import EnqueueAskRequest, EnqueueResponse


class JobsApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def enqueue_ask(self, request: EnqueueAskRequest) -> EnqueueResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            "/v1/jobs/ask",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(EnqueueResponse, value)
