from __future__ import annotations

from medousa._decode import decode
from medousa.client import MedousaClient
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


class RuntimeApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def artifact_command(
        self,
        request: ArtifactCommandRequest,
    ) -> ArtifactCommandResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            "/v1/runtime/artifact/command",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(ArtifactCommandResponse, value)

    async def artifact_fetch(self, request: ArtifactFetchRequest) -> ArtifactFetchResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            "/v1/runtime/artifact/fetch",
            request.model_dump(mode="json"),
        )
        return decode(ArtifactFetchResponse, value)

    async def artifact_write(self, request: ArtifactWriteRequest) -> ArtifactWriteResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            "/v1/runtime/artifact/write",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(ArtifactWriteResponse, value)

    async def artifact_delete(self, request: ArtifactDeleteRequest) -> ArtifactDeleteResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            "/v1/runtime/artifact/delete",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(ArtifactDeleteResponse, value)

    async def artifact_list_ui(self, request: ArtifactListUiRequest) -> ArtifactListUiResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            "/v1/runtime/artifact/list-ui",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(ArtifactListUiResponse, value)

    async def config_command(
        self,
        request: RuntimeConfigCommandRequest,
    ) -> RuntimeConfigCommandResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            "/v1/runtime/config/command",
            request.model_dump(mode="json"),
        )
        return decode(RuntimeConfigCommandResponse, value)

    async def stage_route_command(
        self,
        request: StageRouteCommandRequest,
    ) -> StageRouteCommandResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            "/v1/runtime/stage-route/command",
            request.model_dump(mode="json"),
        )
        return decode(StageRouteCommandResponse, value)
