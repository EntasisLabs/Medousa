from __future__ import annotations

from medousa._decode import decode
from medousa._ops import op_path
from medousa.client import MedousaClient
from medousa.types import (
    BotListResponse,
    BotOpenResponse,
    BotProfile,
    CreateBotRequest,
    DuplicateBotRequest,
    SessionBotResponse,
    SetBotArchivedRequest,
    SetSessionBotRequest,
    UpdateBotRequest,
)


class BotsApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def list(self) -> BotListResponse:
        value = await self._client.transport.get_json(
            self._client.base_url, op_path("bots.get")
        )
        return decode(BotListResponse, value)

    async def create(self, request: CreateBotRequest) -> BotOpenResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("bots.post"),
            request.model_dump(mode="json"),
        )
        return decode(BotOpenResponse, value)

    async def get(self, bot_id: str) -> BotProfile:
        value = await self._client.transport.get_json(
            self._client.base_url, op_path("bots.by_bot_id.get", bot_id=bot_id)
        )
        return decode(BotProfile, value)

    async def update(self, bot_id: str, request: UpdateBotRequest) -> BotProfile:
        value = await self._client.transport.put_json(
            self._client.base_url,
            op_path("bots.by_bot_id.put", bot_id=bot_id),
            request.model_dump(mode="json"),
        )
        return decode(BotProfile, value)

    async def set_archived(
        self, bot_id: str, request: SetBotArchivedRequest
    ) -> BotProfile:
        value = await self._client.transport.put_json(
            self._client.base_url,
            op_path("bots.by_bot_id.archive.put", bot_id=bot_id),
            request.model_dump(mode="json"),
        )
        return decode(BotProfile, value)

    async def duplicate(
        self, bot_id: str, request: DuplicateBotRequest
    ) -> BotOpenResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("bots.by_bot_id.duplicate.post", bot_id=bot_id),
            request.model_dump(mode="json"),
        )
        return decode(BotOpenResponse, value)

    async def open(self, bot_id: str) -> BotOpenResponse:
        value = await self._client.transport.post_empty_json(
            self._client.base_url,
            op_path("bots.by_bot_id.open.post", bot_id=bot_id),
        )
        return decode(BotOpenResponse, value)

    async def session(self, session_id: str) -> SessionBotResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path("sessions.by_session_id.bot.get", session_id=session_id),
        )
        return decode(SessionBotResponse, value)

    async def bind_session(
        self, session_id: str, request: SetSessionBotRequest
    ) -> SessionBotResponse:
        value = await self._client.transport.put_json(
            self._client.base_url,
            op_path("sessions.by_session_id.bot.put", session_id=session_id),
            request.model_dump(mode="json"),
        )
        return decode(SessionBotResponse, value)

    async def unbind_session(self, session_id: str) -> SessionBotResponse:
        value = await self._client.transport.delete_json(
            self._client.base_url,
            op_path("sessions.by_session_id.bot.delete", session_id=session_id),
        )
        return decode(SessionBotResponse, value)
