from __future__ import annotations

from medousa._decode import decode
from medousa.client import MedousaClient
from medousa.types import (
    SessionAppendTurnRequest,
    SessionAppendTurnResponse,
    SessionHistoryListResponse,
    SessionHistoryResponse,
    SessionSetDisplayNameRequest,
    SessionSetDisplayNameResponse,
)


class SessionsApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def list(self, limit: int = 50) -> SessionHistoryListResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            f"/v1/sessions?limit={limit}",
        )
        return decode(SessionHistoryListResponse, value)

    async def history(self, session_id: str) -> SessionHistoryResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            f"/v1/sessions/{session_id}/history",
        )
        return decode(SessionHistoryResponse, value)

    async def set_display_name(
        self,
        session_id: str,
        display_name: str,
    ) -> SessionSetDisplayNameResponse:
        body = SessionSetDisplayNameRequest(display_name=display_name)
        value = await self._client.transport.put_json(
            self._client.base_url,
            f"/v1/sessions/{session_id}/name",
            body.model_dump(mode="json"),
        )
        return decode(SessionSetDisplayNameResponse, value)

    async def append_turn(
        self,
        session_id: str,
        request: SessionAppendTurnRequest,
    ) -> SessionAppendTurnResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            f"/v1/sessions/{session_id}/turns",
            request.model_dump(mode="json"),
        )
        return decode(SessionAppendTurnResponse, value)
