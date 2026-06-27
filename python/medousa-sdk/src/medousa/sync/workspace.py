from __future__ import annotations

from typing import TYPE_CHECKING, Any

from medousa._decode import decode
from medousa.types import WorkspaceCardActionResponse, WorkspaceLinkVaultRequest

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


class WorkspaceApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def list_cards(self) -> dict[str, Any]:
        return self._client._transport.get_json(self._client.base_url, "/v1/workspace/cards")

    def get_card(self, card_id: str) -> dict[str, Any]:
        return self._client._transport.get_json(
            self._client.base_url,
            f"/v1/workspace/cards/{card_id}",
        )

    def cancel_card(self, card_id: str) -> WorkspaceCardActionResponse:
        value = self._client._transport.post_empty_json(
            self._client.base_url,
            f"/v1/workspace/cards/{card_id}/cancel",
        )
        return decode(WorkspaceCardActionResponse, value)

    def archive_card(self, card_id: str) -> WorkspaceCardActionResponse:
        value = self._client._transport.post_empty_json(
            self._client.base_url,
            f"/v1/workspace/cards/{card_id}/archive",
        )
        return decode(WorkspaceCardActionResponse, value)

    def retry_card(self, card_id: str) -> WorkspaceCardActionResponse:
        value = self._client._transport.post_empty_json(
            self._client.base_url,
            f"/v1/workspace/cards/{card_id}/retry",
        )
        return decode(WorkspaceCardActionResponse, value)

    def link_vault(
        self,
        card_id: str,
        request: WorkspaceLinkVaultRequest,
    ) -> WorkspaceCardActionResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            f"/v1/workspace/cards/{card_id}/link-vault",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(WorkspaceCardActionResponse, value)

    def feed(self) -> dict[str, Any]:
        return self._client._transport.get_json(self._client.base_url, "/v1/workspace/feed")

    def snapshot(self) -> dict[str, Any]:
        return self._client._transport.get_json(self._client.base_url, "/v1/workspace/snapshot")

    def stream(self) -> None:
        raise NotImplementedError("workspace.stream is planned and not yet implemented")
