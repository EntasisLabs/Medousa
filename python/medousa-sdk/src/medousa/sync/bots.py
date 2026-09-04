from __future__ import annotations

from typing import TYPE_CHECKING

from medousa._decode import decode
from medousa._ops import op_path
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

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


class BotsApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def list(self) -> BotListResponse:
        value = self._client._transport.get_json(
            self._client.base_url, op_path("bots.get")
        )
        return decode(BotListResponse, value)

    def create(self, request: CreateBotRequest) -> BotOpenResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path("bots.post"),
            request.model_dump(mode="json"),
        )
        return decode(BotOpenResponse, value)

    def get(self, bot_id: str) -> BotProfile:
        value = self._client._transport.get_json(
            self._client.base_url, op_path("bots.by_bot_id.get", bot_id=bot_id)
        )
        return decode(BotProfile, value)

    def update(self, bot_id: str, request: UpdateBotRequest) -> BotProfile:
        value = self._client._transport.put_json(
            self._client.base_url,
            op_path("bots.by_bot_id.put", bot_id=bot_id),
            request.model_dump(mode="json"),
        )
        return decode(BotProfile, value)

    def set_archived(
        self, bot_id: str, request: SetBotArchivedRequest
    ) -> BotProfile:
        value = self._client._transport.put_json(
            self._client.base_url,
            op_path("bots.by_bot_id.archive.put", bot_id=bot_id),
            request.model_dump(mode="json"),
        )
        return decode(BotProfile, value)

    def duplicate(
        self, bot_id: str, request: DuplicateBotRequest
    ) -> BotOpenResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path("bots.by_bot_id.duplicate.post", bot_id=bot_id),
            request.model_dump(mode="json"),
        )
        return decode(BotOpenResponse, value)

    def open(self, bot_id: str) -> BotOpenResponse:
        value = self._client._transport.post_empty_json(
            self._client.base_url,
            op_path("bots.by_bot_id.open.post", bot_id=bot_id),
        )
        return decode(BotOpenResponse, value)

    def session(self, session_id: str) -> SessionBotResponse:
        value = self._client._transport.get_json(
            self._client.base_url,
            op_path("sessions.by_session_id.bot.get", session_id=session_id),
        )
        return decode(SessionBotResponse, value)

    def bind_session(
        self, session_id: str, request: SetSessionBotRequest
    ) -> SessionBotResponse:
        value = self._client._transport.put_json(
            self._client.base_url,
            op_path("sessions.by_session_id.bot.put", session_id=session_id),
            request.model_dump(mode="json"),
        )
        return decode(SessionBotResponse, value)

    def unbind_session(self, session_id: str) -> SessionBotResponse:
        value = self._client._transport.delete_json(
            self._client.base_url,
            op_path("sessions.by_session_id.bot.delete", session_id=session_id),
        )
        return decode(SessionBotResponse, value)
