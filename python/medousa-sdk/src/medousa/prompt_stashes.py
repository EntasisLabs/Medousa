from __future__ import annotations

from medousa._decode import decode
from medousa._ops import op_path
from medousa.client import MedousaClient
from medousa.types import (
    CreatePromptStashRequest,
    DeletePromptStashResponse,
    PromptStash,
    PromptStashListResponse,
)


class PromptStashesApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def list(self) -> PromptStashListResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path("prompt_stashes.get"),
        )
        return decode(PromptStashListResponse, value)

    async def create(self, request: CreatePromptStashRequest) -> PromptStash:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("prompt_stashes.post"),
            request.model_dump(mode="json"),
        )
        return decode(PromptStash, value)

    async def delete(self, stash_id: str) -> DeletePromptStashResponse:
        value = await self._client.transport.delete_json(
            self._client.base_url,
            op_path("prompt_stashes.by_stash_id.delete", stash_id=stash_id),
        )
        return decode(DeletePromptStashResponse, value)
