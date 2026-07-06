from __future__ import annotations

from typing import TYPE_CHECKING

from medousa._decode import decode
from medousa.types import (
    ArtifactCommandRequest,
    ArtifactCommandResponse,
    ArtifactDeleteRequest,
    ArtifactDeleteResponse,
    ArtifactFetchRequest,
    ArtifactFetchResponse,
    ArtifactListUiRequest,
    ArtifactListUiResponse,
    ArtifactWriteRequest,
    ArtifactWriteResponse,
    RuntimeConfigCommandRequest,
    RuntimeConfigCommandResponse,
    StageRouteCommandRequest,
    StageRouteCommandResponse,
)

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


class RuntimeApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def artifact_command(
        self,
        request: ArtifactCommandRequest,
    ) -> ArtifactCommandResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            "/v1/runtime/artifact/command",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(ArtifactCommandResponse, value)

    def artifact_fetch(self, request: ArtifactFetchRequest) -> ArtifactFetchResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            "/v1/runtime/artifact/fetch",
            request.model_dump(mode="json"),
        )
        return decode(ArtifactFetchResponse, value)

    def artifact_write(self, request: ArtifactWriteRequest) -> ArtifactWriteResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            "/v1/runtime/artifact/write",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(ArtifactWriteResponse, value)

    def artifact_delete(self, request: ArtifactDeleteRequest) -> ArtifactDeleteResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            "/v1/runtime/artifact/delete",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(ArtifactDeleteResponse, value)

    def artifact_list_ui(self, request: ArtifactListUiRequest) -> ArtifactListUiResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            "/v1/runtime/artifact/list-ui",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(ArtifactListUiResponse, value)

    def config_command(
        self,
        request: RuntimeConfigCommandRequest,
    ) -> RuntimeConfigCommandResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            "/v1/runtime/config/command",
            request.model_dump(mode="json"),
        )
        return decode(RuntimeConfigCommandResponse, value)

    def stage_route_command(
        self,
        request: StageRouteCommandRequest,
    ) -> StageRouteCommandResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            "/v1/runtime/stage-route/command",
            request.model_dump(mode="json"),
        )
        return decode(StageRouteCommandResponse, value)
