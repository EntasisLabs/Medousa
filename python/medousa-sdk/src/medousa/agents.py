from __future__ import annotations

from medousa._decode import decode
from medousa.client import MedousaClient
from medousa.transport import path_with_query
from medousa.types import (
    AgentPermissionRequestListResponse,
    AgentPermissionResolveRequest,
    AgentPermissionResolveResponse,
    AgentRuntimeListResponse,
    AgentSessionPromptRequest,
    AgentSessionPromptResponse,
    CancelAgentSessionResponse,
    CreateAgentSessionRequest,
    CreateAgentSessionResponse,
)


class AgentsApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def list_runtimes(self) -> AgentRuntimeListResponse:
        value = await self._client.transport.get_json(
            self._client.base_url, "/v1/agents/runtimes"
        )
        return decode(AgentRuntimeListResponse, value)

    async def create_session(
        self, request: CreateAgentSessionRequest
    ) -> CreateAgentSessionResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            "/v1/agents/sessions",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreateAgentSessionResponse, value)

    async def prompt(
        self, agent_session_id: str, request: AgentSessionPromptRequest
    ) -> AgentSessionPromptResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            f"/v1/agents/sessions/{agent_session_id.strip()}/prompt",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(AgentSessionPromptResponse, value)

    async def cancel(self, agent_session_id: str) -> CancelAgentSessionResponse:
        value = await self._client.transport.post_empty_json(
            self._client.base_url,
            f"/v1/agents/sessions/{agent_session_id.strip()}/cancel",
        )
        return decode(CancelAgentSessionResponse, value)

    async def list_permission_requests(
        self, *, status: str | None = "pending", limit: int | None = None
    ) -> AgentPermissionRequestListResponse:
        query: list[tuple[str, str]] = []
        if status is not None:
            query.append(("status", status))
        if limit is not None:
            query.append(("limit", str(limit)))
        route = path_with_query("/v1/agents/permission-requests", query)
        value = await self._client.transport.get_json(self._client.base_url, route)
        return decode(AgentPermissionRequestListResponse, value)

    async def approve_permission(
        self, request_id: str, request: AgentPermissionResolveRequest | None = None
    ) -> AgentPermissionResolveResponse:
        body = (request or AgentPermissionResolveRequest()).model_dump(
            mode="json", exclude_none=True
        )
        value = await self._client.transport.post_json(
            self._client.base_url,
            f"/v1/agents/permission-requests/{request_id.strip()}/approve",
            body,
        )
        return decode(AgentPermissionResolveResponse, value)

    async def deny_permission(
        self, request_id: str, request: AgentPermissionResolveRequest | None = None
    ) -> AgentPermissionResolveResponse:
        body = (request or AgentPermissionResolveRequest()).model_dump(
            mode="json", exclude_none=True
        )
        value = await self._client.transport.post_json(
            self._client.base_url,
            f"/v1/agents/permission-requests/{request_id.strip()}/deny",
            body,
        )
        return decode(AgentPermissionResolveResponse, value)
