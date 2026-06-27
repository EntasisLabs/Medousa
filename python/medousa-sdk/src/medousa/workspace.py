from __future__ import annotations

from typing import Any

from medousa._decode import decode
from medousa.client import MedousaClient
from medousa.types import WorkspaceCardActionResponse, WorkspaceLinkVaultRequest


class WorkspaceApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def list_cards(self) -> dict[str, Any]:
        return await self._client.transport.get_json(
            self._client.base_url,
            "/v1/workspace/cards",
        )

    async def get_card(self, card_id: str) -> dict[str, Any]:
        return await self._client.transport.get_json(
            self._client.base_url,
            f"/v1/workspace/cards/{card_id}",
        )

    async def cancel_card(self, card_id: str) -> WorkspaceCardActionResponse:
        value = await self._client.transport.post_empty_json(
            self._client.base_url,
            f"/v1/workspace/cards/{card_id}/cancel",
        )
        return decode(WorkspaceCardActionResponse, value)

    async def archive_card(self, card_id: str) -> WorkspaceCardActionResponse:
        value = await self._client.transport.post_empty_json(
            self._client.base_url,
            f"/v1/workspace/cards/{card_id}/archive",
        )
        return decode(WorkspaceCardActionResponse, value)

    async def retry_card(self, card_id: str) -> WorkspaceCardActionResponse:
        value = await self._client.transport.post_empty_json(
            self._client.base_url,
            f"/v1/workspace/cards/{card_id}/retry",
        )
        return decode(WorkspaceCardActionResponse, value)

    async def link_vault(
        self,
        card_id: str,
        request: WorkspaceLinkVaultRequest,
    ) -> WorkspaceCardActionResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            f"/v1/workspace/cards/{card_id}/link-vault",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(WorkspaceCardActionResponse, value)

    async def feed(self) -> dict[str, Any]:
        return await self._client.transport.get_json(
            self._client.base_url,
            "/v1/workspace/feed",
        )

    async def snapshot(self) -> dict[str, Any]:
        return await self._client.transport.get_json(
            self._client.base_url,
            "/v1/workspace/snapshot",
        )

    async def stream(self) -> None:
        raise NotImplementedError("workspace.stream is planned and not yet implemented")
