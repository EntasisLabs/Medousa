from __future__ import annotations

from medousa._decode import decode
from medousa.client import MedousaClient
from medousa.types.local import (
    LocalCatalogResponse,
    LocalEngineStatus,
    LocalHardwareResponse,
    LocalModelDownloadResponse,
    LocalModelsResponse,
    ModelDownloadProgress,
)


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
        value = await self._client.transport.post_json(
            self._client.base_url,
            "/v1/local/models/download",
            {"modelId": model_id},
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
