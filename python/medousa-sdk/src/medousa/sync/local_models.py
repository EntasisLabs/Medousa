from __future__ import annotations

from typing import TYPE_CHECKING

from medousa._decode import decode
from medousa.types import (
    LocalCatalogResponse,
    LocalEngineStatus,
    LocalHardwareResponse,
    LocalModelDownloadRequest,
    LocalModelDownloadResponse,
    LocalModelsResponse,
    ModelDownloadProgress,
)

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


class LocalModelsApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def hardware(self) -> LocalHardwareResponse:
        return decode(
            LocalHardwareResponse,
            self._client._transport.get_json(self._client.base_url, "/v1/local/hardware"),
        )

    def catalog(self) -> LocalCatalogResponse:
        return decode(
            LocalCatalogResponse,
            self._client._transport.get_json(self._client.base_url, "/v1/local/catalog"),
        )

    def list(self) -> LocalModelsResponse:
        return decode(
            LocalModelsResponse,
            self._client._transport.get_json(self._client.base_url, "/v1/local/models"),
        )

    def engine_status(self) -> LocalEngineStatus:
        return decode(
            LocalEngineStatus,
            self._client._transport.get_json(self._client.base_url, "/v1/local/engine/status"),
        )

    def start_download(self, model_id: str) -> LocalModelDownloadResponse:
        body = LocalModelDownloadRequest(modelId=model_id)
        value = self._client._transport.post_json(
            self._client.base_url,
            "/v1/local/models/download",
            body.model_dump(mode="json", exclude_none=True),
        )
        return decode(LocalModelDownloadResponse, value)

    def download_status(self, job_id: str) -> ModelDownloadProgress:
        value = self._client._transport.get_json(
            self._client.base_url,
            f"/v1/local/models/download/{job_id}",
        )
        return decode(ModelDownloadProgress, value)

    def remove_model(self, model_id: str) -> dict:
        return self._client._transport.delete_json(
            self._client.base_url,
            f"/v1/local/models/{model_id}",
        )
