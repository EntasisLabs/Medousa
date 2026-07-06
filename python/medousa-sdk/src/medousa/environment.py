from __future__ import annotations

import json
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import TYPE_CHECKING

from medousa._decode import decode
from medousa.streaming import iter_sse_events
from medousa.transport import path_with_query
from medousa.types import (
    EnvironmentPendingResponse,
    EnvironmentProposeResponse,
    EnvironmentSpecPutRequest,
    EnvironmentSpecResponse,
    EnvironmentStatusResponse,
    EnvironmentStreamEvent,
    EnvironmentValidateRequest,
    EnvironmentValidateResponse,
)

if TYPE_CHECKING:
    from medousa.client import MedousaClient


def _profile_query(profile_id: str | None) -> list[tuple[str, str]]:
    return [("profile_id", profile_id)] if profile_id is not None else []


def _stream_query(
    profile_id: str | None,
    since_revision: int | None,
) -> list[tuple[str, str]]:
    query = _profile_query(profile_id)
    if since_revision is not None:
        query.append(("since_revision", str(since_revision)))
    return query


class EnvironmentSpecStream:
    """Async iterator over environment spec SSE events."""

    def __init__(self, client: MedousaClient, stream_path: str) -> None:
        self._client = client
        self._stream_path = stream_path
        self._response = None

    async def __aenter__(self) -> EnvironmentSpecStream:
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

    def __aiter__(self) -> AsyncIterator[EnvironmentStreamEvent]:
        return self._iter_events()

    async def _iter_events(self) -> AsyncIterator[EnvironmentStreamEvent]:
        if self._response is None:
            raise RuntimeError("EnvironmentSpecStream must be used as an async context manager")
        async for data in iter_sse_events(self._response):
            if not data or data == "{}":
                continue
            yield decode(EnvironmentStreamEvent, json.loads(data))


class EnvironmentApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def get_spec(self, profile_id: str | None = None) -> EnvironmentSpecResponse:
        path = path_with_query("/v1/environment/spec", _profile_query(profile_id))
        value = await self._client.transport.get_json(self._client.base_url, path)
        return decode(EnvironmentSpecResponse, value)

    async def put_spec(self, request: EnvironmentSpecPutRequest) -> EnvironmentSpecResponse:
        value = await self._client.transport.put_json(
            self._client.base_url,
            "/v1/environment/spec",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(EnvironmentSpecResponse, value)

    async def get_status(
        self,
        *,
        profile_id: str | None = None,
        surface_id: str | None = None,
        include_runtime: bool | None = None,
    ) -> EnvironmentStatusResponse:
        query = _profile_query(profile_id)
        if surface_id is not None:
            query.append(("surface_id", surface_id))
        if include_runtime is not None:
            query.append(("include_runtime", str(include_runtime).lower()))
        path = path_with_query("/v1/environment/status", query)
        value = await self._client.transport.get_json(self._client.base_url, path)
        return decode(EnvironmentStatusResponse, value)

    async def validate_spec(
        self,
        request: EnvironmentValidateRequest,
    ) -> EnvironmentValidateResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            "/v1/environment/spec/validate",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(EnvironmentValidateResponse, value)

    async def propose_spec(
        self,
        request: EnvironmentSpecPutRequest,
    ) -> EnvironmentProposeResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            "/v1/environment/spec/propose",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(EnvironmentProposeResponse, value)

    async def get_pending(self, profile_id: str | None = None) -> EnvironmentPendingResponse:
        path = path_with_query("/v1/environment/spec/pending", _profile_query(profile_id))
        value = await self._client.transport.get_json(self._client.base_url, path)
        return decode(EnvironmentPendingResponse, value)

    async def dismiss_pending(self, profile_id: str | None = None) -> None:
        path = path_with_query("/v1/environment/spec/pending", _profile_query(profile_id))
        await self._client.transport.delete_json(self._client.base_url, path)

    async def apply_pending(self, profile_id: str | None = None) -> EnvironmentSpecResponse:
        path = path_with_query("/v1/environment/spec/pending/apply", _profile_query(profile_id))
        value = await self._client.transport.post_empty_json(self._client.base_url, path)
        return decode(EnvironmentSpecResponse, value)

    def stream_spec(
        self,
        *,
        profile_id: str | None = None,
        since_revision: int | None = None,
    ) -> EnvironmentSpecStream:
        path = path_with_query(
            "/v1/environment/spec/stream",
            _stream_query(profile_id, since_revision),
        )
        return EnvironmentSpecStream(self._client, path)

    @asynccontextmanager
    async def stream_spec_ctx(
        self,
        *,
        profile_id: str | None = None,
        since_revision: int | None = None,
    ):
        stream = self.stream_spec(profile_id=profile_id, since_revision=since_revision)
        async with stream:
            yield stream
