from __future__ import annotations

from typing import Any

from medousa._decode import decode
from medousa._ops import op_path
from medousa.client import MedousaClient
from medousa.types import WorkspaceCardActionResponse, WorkspaceLinkVaultRequest


class WorkspaceApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def list_cards(self) -> dict[str, Any]:
        return await self._client.transport.get_json(
            self._client.base_url,
            op_path("workspace.cards.get"),
        )

    async def get_card(self, card_id: str) -> dict[str, Any]:
        return await self._client.transport.get_json(
            self._client.base_url,
            op_path("workspace.cards.by_card_id.get", card_id=card_id),
        )

    async def cancel_card(self, card_id: str) -> WorkspaceCardActionResponse:
        value = await self._client.transport.post_empty_json(
            self._client.base_url,
            op_path("workspace.cards.by_card_id.cancel.post", card_id=card_id),
        )
        return decode(WorkspaceCardActionResponse, value)

    async def archive_card(self, card_id: str) -> WorkspaceCardActionResponse:
        value = await self._client.transport.post_empty_json(
            self._client.base_url,
            op_path("workspace.cards.by_card_id.archive.post", card_id=card_id),
        )
        return decode(WorkspaceCardActionResponse, value)

    async def retry_card(self, card_id: str) -> WorkspaceCardActionResponse:
        value = await self._client.transport.post_empty_json(
            self._client.base_url,
            op_path("workspace.cards.by_card_id.retry.post", card_id=card_id),
        )
        return decode(WorkspaceCardActionResponse, value)

    async def link_vault(
        self,
        card_id: str,
        request: WorkspaceLinkVaultRequest,
    ) -> WorkspaceCardActionResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("workspace.cards.by_card_id.link_vault.post", card_id=card_id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(WorkspaceCardActionResponse, value)

    async def feed(self) -> dict[str, Any]:
        return await self._client.transport.get_json(
            self._client.base_url,
            op_path("workspace.feed.get"),
        )

    async def snapshot(self) -> dict[str, Any]:
        return await self._client.transport.get_json(
            self._client.base_url,
            op_path("workspace.snapshot.get"),
        )

    async def stream(self) -> None:
        raise NotImplementedError("workspace.stream is planned and not yet implemented")
