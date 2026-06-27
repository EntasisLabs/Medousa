from __future__ import annotations

import json
from collections.abc import AsyncIterator

from medousa._decode import decode
from medousa.client import MedousaClient
from medousa.streaming import iter_sse_events
from medousa.types import (
    LocalCatalogResponse,
    LocalEngineStatus,
    LocalHardwareResponse,
    LocalModelDownloadRequest,
    LocalModelDownloadResponse,
    LocalModelsResponse,
    ModelDownloadProgress,
)


class DownloadEventsStream:
    """Async iterator over SSE download progress events."""

    def __init__(self, client: MedousaClient, stream_path: str) -> None:
        self._client = client
        self._stream_path = stream_path
        self._response = None

    async def __aenter__(self) -> DownloadEventsStream:
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

    def __aiter__(self) -> AsyncIterator[ModelDownloadProgress]:
        return self._iter_events()

    async def _iter_events(self) -> AsyncIterator[ModelDownloadProgress]:
        if self._response is None:
            raise RuntimeError("DownloadEventsStream must be used as an async context manager")
        async for data in iter_sse_events(self._response):
            if not data or data == "[DONE]":
                continue
            yield decode(ModelDownloadProgress, json.loads(data))


class LocalModelsApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def hardware(self) -> LocalHardwareResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            "/v1/local/hardware",
        )
        return decode(LocalHardwareResponse, value)

    async def catalog(self) -> LocalCatalogResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            "/v1/local/catalog",
        )
        return decode(LocalCatalogResponse, value)

    async def list(self) -> LocalModelsResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            "/v1/local/models",
        )
        return decode(LocalModelsResponse, value)

    async def engine_status(self) -> LocalEngineStatus:
        value = await self._client.transport.get_json(
            self._client.base_url,
            "/v1/local/engine/status",
        )
        return decode(LocalEngineStatus, value)

    async def start_download(self, model_id: str) -> LocalModelDownloadResponse:
        body = LocalModelDownloadRequest(modelId=model_id)
        value = await self._client.transport.post_json(
            self._client.base_url,
            "/v1/local/models/download",
            body.model_dump(mode="json", exclude_none=True),
        )
        return decode(LocalModelDownloadResponse, value)

    async def remove_model(self, model_id: str) -> dict:
        return await self._client.transport.delete_json(
            self._client.base_url,
            f"/v1/local/models/{model_id}",
        )

    async def download_status(self, job_id: str) -> ModelDownloadProgress:
        value = await self._client.transport.get_json(
            self._client.base_url,
            f"/v1/local/models/download/{job_id}",
        )
        return decode(ModelDownloadProgress, value)

    def download_events(self, job_id: str) -> DownloadEventsStream:
        return DownloadEventsStream(
            self._client,
            f"/v1/local/models/download/{job_id}/events",
        )
