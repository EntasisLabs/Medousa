from __future__ import annotations

from typing import TYPE_CHECKING, Any

from medousa._decode import decode
from medousa._ops import op_path
from medousa.types import WorkspaceCardActionResponse, WorkspaceLinkVaultRequest

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


class WorkspaceApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def list_cards(self) -> dict[str, Any]:
        return self._client._transport.get_json(
            self._client.base_url, op_path("workspace.cards.get")
        )

    def get_card(self, card_id: str) -> dict[str, Any]:
        return self._client._transport.get_json(
            self._client.base_url,
            op_path("workspace.cards.by_card_id.get", card_id=card_id),
        )

    def cancel_card(self, card_id: str) -> WorkspaceCardActionResponse:
        value = self._client._transport.post_empty_json(
            self._client.base_url,
            op_path("workspace.cards.by_card_id.cancel.post", card_id=card_id),
        )
        return decode(WorkspaceCardActionResponse, value)

    def archive_card(self, card_id: str) -> WorkspaceCardActionResponse:
        value = self._client._transport.post_empty_json(
            self._client.base_url,
            op_path("workspace.cards.by_card_id.archive.post", card_id=card_id),
        )
        return decode(WorkspaceCardActionResponse, value)

    def retry_card(self, card_id: str) -> WorkspaceCardActionResponse:
        value = self._client._transport.post_empty_json(
            self._client.base_url,
            op_path("workspace.cards.by_card_id.retry.post", card_id=card_id),
        )
        return decode(WorkspaceCardActionResponse, value)

    def link_vault(
        self,
        card_id: str,
        request: WorkspaceLinkVaultRequest,
    ) -> WorkspaceCardActionResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path("workspace.cards.by_card_id.link_vault.post", card_id=card_id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(WorkspaceCardActionResponse, value)

    def feed(self) -> dict[str, Any]:
        return self._client._transport.get_json(
            self._client.base_url, op_path("workspace.feed.get")
        )

    def snapshot(self) -> dict[str, Any]:
        return self._client._transport.get_json(
            self._client.base_url, op_path("workspace.snapshot.get")
        )

    def stream(self) -> None:
        raise NotImplementedError("workspace.stream is planned and not yet implemented")
