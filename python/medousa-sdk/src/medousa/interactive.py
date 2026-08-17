from __future__ import annotations

import json
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import TYPE_CHECKING, Any, Generic, TypeVar, cast

from pydantic import BaseModel

from medousa._decode import decode
from medousa._ops import op_path
from medousa.reconnect import (
    TURN_STREAM_V2_MEDIA_TYPE,
    ReconnectingInteractiveStream,
    ReconnectingInteractiveStreamV2,
    ReconnectPolicy,
)
from medousa.streaming import iter_sse_events
from medousa.types import (
    InteractiveTurnRequest,
    InteractiveTurnResponse,
    InteractiveTurnStreamEvent,
    TurnStreamEnvelopeV2,
)

if TYPE_CHECKING:
    from medousa.client import MedousaClient


TEvent = TypeVar("TEvent", bound=BaseModel)


class InteractiveStream(Generic[TEvent]):
    """Async iterator over SSE events for an interactive turn."""

    def __init__(
        self,
        client: MedousaClient,
        stream_path: str,
        *,
        event_model: type[TEvent] | None = None,
        accept: str = "text/event-stream",
    ) -> None:
        self._client = client
        self._stream_path = stream_path
        self._event_model = event_model or cast(type[TEvent], InteractiveTurnStreamEvent)
        self._accept = accept
        self._response = None

    async def __aenter__(self) -> InteractiveStream:
        if self._accept == "text/event-stream":
            self._response = await self._client.transport.stream_sse(
                self._client.base_url,
                self._stream_path,
            )
        else:
            self._response = await self._client.transport.stream_sse_with_accept(
                self._client.base_url,
                self._stream_path,
                self._accept,
            )
        return self

    async def __aexit__(self, *args: object) -> None:
        if self._response is not None:
            aclose = getattr(self._response, "aclose", None)
            if aclose is not None:
                await aclose()
            self._response = None

    def __aiter__(self) -> AsyncIterator[TEvent]:
        return self._iter_events()

    async def _iter_events(self) -> AsyncIterator[TEvent]:
        if self._response is None:
            raise RuntimeError("InteractiveStream must be used as an async context manager")
        async for data in iter_sse_events(self._response):
            if not data or data == "[DONE]":
                continue
            yield decode(self._event_model, json.loads(data))


class InteractiveApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def start_turn(self, request: InteractiveTurnRequest) -> InteractiveTurnResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("interactive.turn.post"),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(InteractiveTurnResponse, value)

    @asynccontextmanager
    async def stream_turn(self, request: InteractiveTurnRequest):
        """Start a turn and yield an async iterator of SSE events."""
        response = await self.start_turn(request)
        stream = InteractiveStream[InteractiveTurnStreamEvent](self._client, response.stream_url)
        async with stream:
            yield stream

    def stream(self, stream_url: str) -> InteractiveStream:
        """Open SSE for an existing stream URL from start_turn."""
        return InteractiveStream(self._client, stream_url)

    def stream_v2(self, stream_url: str) -> InteractiveStream[TurnStreamEnvelopeV2]:
        """Open a one-shot typed v2 stream for an existing turn URL."""
        return InteractiveStream(
            self._client,
            stream_url,
            event_model=TurnStreamEnvelopeV2,
            accept=TURN_STREAM_V2_MEDIA_TYPE,
        )

    def stream_reconnecting(
        self,
        stream_url: str,
        *,
        policy: ReconnectPolicy | None = None,
    ) -> ReconnectingInteractiveStream:
        from medousa.reconnect import ReconnectingInteractiveStream

        return ReconnectingInteractiveStream(
            self._client,
            stream_url,
            policy=policy or ReconnectPolicy(),
        )

    def stream_reconnecting_v2(
        self,
        stream_url: str,
        *,
        policy: ReconnectPolicy | None = None,
    ) -> ReconnectingInteractiveStreamV2:
        """Open the recommended typed v2 stream with spine-backed reconnect."""
        return ReconnectingInteractiveStreamV2(
            self._client,
            stream_url,
            policy=policy or ReconnectPolicy(),
        )

    @asynccontextmanager
    async def stream_turn_reconnecting(
        self,
        request: InteractiveTurnRequest,
        *,
        policy: ReconnectPolicy | None = None,
    ):
        """Start a turn and yield a reconnecting SSE iterator."""
        response = await self.start_turn(request)
        yield self.stream_reconnecting(
            response.stream_url,
            policy=policy or ReconnectPolicy(),
        )

    @asynccontextmanager
    async def stream_turn_reconnecting_v2(
        self,
        request: InteractiveTurnRequest,
        *,
        policy: ReconnectPolicy | None = None,
    ):
        """Start a turn and yield its typed v2 reconnecting stream."""
        response = await self.start_turn(request)
        yield self.stream_reconnecting_v2(
            response.stream_url,
            policy=policy or ReconnectPolicy(),
        )

    async def cancel(self, session_id: str) -> dict[str, Any]:
        return await self._client.sessions().cancel_active_turn(session_id)
