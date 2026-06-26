from __future__ import annotations

import json
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import TYPE_CHECKING

from medousa._decode import decode
from medousa.streaming import iter_sse_events
from medousa.types import (
    InteractiveTurnRequest,
    InteractiveTurnResponse,
    InteractiveTurnStreamEvent,
)

if TYPE_CHECKING:
    from medousa.client import MedousaClient


class InteractiveStream:
    """Async iterator over SSE events for an interactive turn."""

    def __init__(self, client: MedousaClient, stream_path: str) -> None:
        self._client = client
        self._stream_path = stream_path
        self._response = None

    async def __aenter__(self) -> InteractiveStream:
        self._response = await self._client.transport.stream_sse(
            self._client.base_url,
            self._stream_path,
        )
        return self

    async def __aexit__(self, *args: object) -> None:
        if self._response is not None:
            aclose = getattr(self._response, "aclose", None)
            if aclose is not None:
                await aclose()
            self._response = None

    def __aiter__(self) -> AsyncIterator[InteractiveTurnStreamEvent]:
        return self._iter_events()

    async def _iter_events(self) -> AsyncIterator[InteractiveTurnStreamEvent]:
        if self._response is None:
            raise RuntimeError("InteractiveStream must be used as an async context manager")
        async for data in iter_sse_events(self._response):
            if not data or data == "[DONE]":
                continue
            yield decode(InteractiveTurnStreamEvent, json.loads(data))


class InteractiveApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def start_turn(self, request: InteractiveTurnRequest) -> InteractiveTurnResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            "/v1/interactive/turn",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(InteractiveTurnResponse, value)

    @asynccontextmanager
    async def stream_turn(self, request: InteractiveTurnRequest):
        """Start a turn and yield an async iterator of SSE events."""
        response = await self.start_turn(request)
        stream = InteractiveStream(self._client, response.stream_url)
        async with stream:
            yield stream

    def stream(self, stream_url: str) -> InteractiveStream:
        """Open SSE for an existing stream URL from start_turn."""
        return InteractiveStream(self._client, stream_url)

    async def cancel(self, session_id: str) -> dict:
        return await self._client.transport.post_empty_json(
            self._client.base_url,
            f"/v1/sessions/{session_id}/active-turn",
        )
