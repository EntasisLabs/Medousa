from __future__ import annotations

from typing import TYPE_CHECKING

from medousa._decode import decode
from medousa._ops import op_path
from medousa.types import (
    CreatePromptStashRequest,
    DeletePromptStashResponse,
    PromptStash,
    PromptStashListResponse,
)

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


class PromptStashesApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def list(self) -> PromptStashListResponse:
        value = self._client._transport.get_json(
            self._client.base_url,
            op_path("prompt_stashes.get"),
        )
        return decode(PromptStashListResponse, value)

    def create(self, request: CreatePromptStashRequest) -> PromptStash:
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path("prompt_stashes.post"),
            request.model_dump(mode="json"),
        )
        return decode(PromptStash, value)

    def delete(self, stash_id: str) -> DeletePromptStashResponse:
        value = self._client._transport.delete_json(
            self._client.base_url,
            op_path("prompt_stashes.by_stash_id.delete", stash_id=stash_id),
        )
        return decode(DeletePromptStashResponse, value)
