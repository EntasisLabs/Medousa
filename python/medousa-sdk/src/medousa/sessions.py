from __future__ import annotations

from typing import Any

from medousa._decode import decode
from medousa._ops import op_path, op_path_query
from medousa.client import MedousaClient
from medousa.types import (
    SessionAppendTurnRequest,
    SessionAppendTurnResponse,
    SessionDeleteResponse,
    SessionHistoryListResponse,
    SessionHistoryResponse,
    SessionSetDisplayNameRequest,
    SessionSetDisplayNameResponse,
    SessionTranscriptSearchResponse,
)


class SessionsApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def list(self, limit: int = 50) -> SessionHistoryListResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path_query("sessions.get", [("limit", str(limit))]),
        )
        return decode(SessionHistoryListResponse, value)

    async def search_transcripts(
        self,
        query: str,
        limit: int = 20,
    ) -> SessionTranscriptSearchResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path_query(
                "sessions.search.get",
                [("q", query), ("limit", str(limit))],
            ),
        )
        return decode(SessionTranscriptSearchResponse, value)

    async def history(self, session_id: str) -> SessionHistoryResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path("sessions.by_session_id.history.get", session_id=session_id),
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
            op_path("sessions.by_session_id.name.put", session_id=session_id),
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
            op_path("sessions.by_session_id.turns.post", session_id=session_id),
            request.model_dump(mode="json"),
        )
        return decode(SessionAppendTurnResponse, value)

    async def delete(self, session_id: str) -> SessionDeleteResponse:
        value = await self._client.transport.delete_json(
            self._client.base_url,
            op_path("sessions.by_session_id.delete", session_id=session_id),
        )
        return decode(SessionDeleteResponse, value)

    async def list_turns(self, session_id: str) -> SessionHistoryResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path("sessions.by_session_id.turns.get", session_id=session_id),
        )
        return decode(SessionHistoryResponse, value)

    async def active_turn(self, session_id: str) -> dict[str, Any]:
        return await self._client.transport.get_json(
            self._client.base_url,
            op_path("sessions.by_session_id.active_turn.get", session_id=session_id),
        )

    async def cancel_active_turn(self, session_id: str) -> dict[str, Any]:
        return await self._client.transport.post_empty_json(
            self._client.base_url,
            op_path("sessions.by_session_id.active_turn.post", session_id=session_id),
        )
