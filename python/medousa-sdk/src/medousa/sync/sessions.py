from __future__ import annotations

from typing import TYPE_CHECKING, Any

from medousa._decode import decode
from medousa.types import (
    SessionAppendTurnRequest,
    SessionAppendTurnResponse,
    SessionDeleteResponse,
    SessionHistoryListResponse,
    SessionHistoryResponse,
    SessionSetDisplayNameRequest,
    SessionSetDisplayNameResponse,
)

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


class SessionsApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def list(self, limit: int = 50) -> SessionHistoryListResponse:
        value = self._client._transport.get_json(
            self._client.base_url,
            f"/v1/sessions?limit={limit}",
        )
        return decode(SessionHistoryListResponse, value)

    def history(self, session_id: str) -> SessionHistoryResponse:
        value = self._client._transport.get_json(
            self._client.base_url,
            f"/v1/sessions/{session_id}/history",
        )
        return decode(SessionHistoryResponse, value)

    def set_display_name(
        self,
        session_id: str,
        display_name: str,
    ) -> SessionSetDisplayNameResponse:
        body = SessionSetDisplayNameRequest(display_name=display_name)
        value = self._client._transport.put_json(
            self._client.base_url,
            f"/v1/sessions/{session_id}/name",
            body.model_dump(mode="json"),
        )
        return decode(SessionSetDisplayNameResponse, value)

    def append_turn(
        self,
        session_id: str,
        request: SessionAppendTurnRequest,
    ) -> SessionAppendTurnResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            f"/v1/sessions/{session_id}/turns",
            request.model_dump(mode="json"),
        )
        return decode(SessionAppendTurnResponse, value)

    def delete(self, session_id: str) -> SessionDeleteResponse:
        value = self._client._transport.delete_json(
            self._client.base_url,
            f"/v1/sessions/{session_id}",
        )
        return decode(SessionDeleteResponse, value)

    def list_turns(self, session_id: str) -> SessionHistoryResponse:
        value = self._client._transport.get_json(
            self._client.base_url,
            f"/v1/sessions/{session_id}/turns",
        )
        return decode(SessionHistoryResponse, value)

    def active_turn(self, session_id: str) -> dict[str, Any]:
        return self._client._transport.get_json(
            self._client.base_url,
            f"/v1/sessions/{session_id}/active-turn",
        )

    def cancel_active_turn(self, session_id: str) -> dict[str, Any]:
        return self._client._transport.post_empty_json(
            self._client.base_url,
            f"/v1/sessions/{session_id}/active-turn",
        )
