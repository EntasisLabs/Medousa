from __future__ import annotations

import json
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import TYPE_CHECKING

from medousa._decode import decode
from medousa.streaming import iter_sse_events
from medousa.transport import path_with_query
from medousa.types import (
    FeedListResponse,
    FeedReadRequest,
    FeedStreamEvent,
    FeedTailQuery,
    FeedTailResponse,
)

if TYPE_CHECKING:
    from medousa.client import MedousaClient


def _profile_query(profile_id: str | None) -> list[tuple[str, str]]:
    return [("profile_id", profile_id)] if profile_id is not None else []


def _tail_query(query: FeedTailQuery) -> list[tuple[str, str]]:
    params = _profile_query(query.profile_id)
    if query.limit is not None:
        params.append(("limit", str(query.limit)))
    return params


class FeedsStream:
    """Async iterator over feed bus SSE events."""

    def __init__(self, client: MedousaClient, stream_path: str) -> None:
        self._client = client
        self._stream_path = stream_path
        self._response = None

    async def __aenter__(self) -> FeedsStream:
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

    def __aiter__(self) -> AsyncIterator[FeedStreamEvent]:
        return self._iter_events()

    async def _iter_events(self) -> AsyncIterator[FeedStreamEvent]:
        if self._response is None:
            raise RuntimeError("FeedsStream must be used as an async context manager")
        async for data in iter_sse_events(self._response):
            if not data or data == "[DONE]":
                continue
            yield decode(FeedStreamEvent, json.loads(data))


class FeedsApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def list(self, profile_id: str | None = None) -> FeedListResponse:
        path = path_with_query("/v1/feeds", _profile_query(profile_id))
        value = await self._client.transport.get_json(self._client.base_url, path)
        return decode(FeedListResponse, value)

    async def tail(self, feed_id: str, query: FeedTailQuery) -> FeedTailResponse:
        path = path_with_query(f"/v1/feeds/{feed_id.strip()}/tail", _tail_query(query))
        value = await self._client.transport.get_json(self._client.base_url, path)
        return decode(FeedTailResponse, value)

    async def mark_read(self, feed_id: str, request: FeedReadRequest) -> None:
        await self._client.transport.post_json(
            self._client.base_url,
            f"/v1/feeds/{feed_id.strip()}/read",
            request.model_dump(mode="json", exclude_none=True),
        )

    def stream(self, profile_id: str | None = None) -> FeedsStream:
        path = path_with_query("/v1/feeds/stream", _profile_query(profile_id))
        return FeedsStream(self._client, path)

    @asynccontextmanager
    async def stream_ctx(self, profile_id: str | None = None):
        stream = self.stream(profile_id)
        async with stream:
            yield stream
