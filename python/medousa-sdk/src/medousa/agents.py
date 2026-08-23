from __future__ import annotations

from contextlib import asynccontextmanager

from medousa._decode import decode
from medousa._ops import op_path, op_path_query
from medousa.client import MedousaClient
from medousa.interactive import InteractiveStream
from medousa.types import (
    AgentPermissionRequestListResponse,
    AgentPermissionResolveRequest,
    AgentPermissionResolveResponse,
    AgentRuntimeListResponse,
    AgentSecretRequestListResponse,
    AgentSecretResolveResponse,
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
            self._client.base_url, op_path("agents.runtimes.get")
        )
        return decode(AgentRuntimeListResponse, value)

    async def create_session(
        self, request: CreateAgentSessionRequest
    ) -> CreateAgentSessionResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("agents.sessions.post"),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreateAgentSessionResponse, value)

    def stream(self, stream_url: str) -> InteractiveStream:
        """Open SSE for an existing agent session stream URL from create_session."""
        return InteractiveStream(self._client, stream_url)

    @asynccontextmanager
    async def stream_session(self, request: CreateAgentSessionRequest):
        """Create an agent session and yield an async iterator of SSE events."""
        response = await self.create_session(request)
        stream = self.stream(response.stream_url)
        async with stream:
            yield stream

    async def prompt(
        self, agent_session_id: str, request: AgentSessionPromptRequest
    ) -> AgentSessionPromptResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path(
                "agents.sessions.by_agent_session_id.prompt.post",
                agent_session_id=agent_session_id.strip(),
            ),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(AgentSessionPromptResponse, value)

    async def cancel(self, agent_session_id: str) -> CancelAgentSessionResponse:
        value = await self._client.transport.post_empty_json(
            self._client.base_url,
            op_path(
                "agents.sessions.by_agent_session_id.cancel.post",
                agent_session_id=agent_session_id.strip(),
            ),
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
        route = op_path_query("agents.permission_requests.get", query)
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
            op_path(
                "agents.permission_requests.by_request_id.approve.post",
                request_id=request_id.strip(),
            ),
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
            op_path(
                "agents.permission_requests.by_request_id.deny.post", request_id=request_id.strip()
            ),
            body,
        )
        return decode(AgentPermissionResolveResponse, value)

    async def list_secret_requests(
        self, *, status: str | None = "pending", limit: int | None = None
    ) -> AgentSecretRequestListResponse:
        query: list[tuple[str, str]] = []
        if status is not None:
            query.append(("status", status))
        if limit is not None:
            query.append(("limit", str(limit)))
        route = op_path_query("agents.secret_requests.get", query)
        value = await self._client.transport.get_json(self._client.base_url, route)
        return decode(AgentSecretRequestListResponse, value)

    async def fulfill_secret_request(
        self, request_id: str, value: str, *, resolved_by: str | None = None
    ) -> AgentSecretResolveResponse:
        """Move a write-only value directly to the request's workshop backend."""
        body = {"value": value}
        if resolved_by is not None:
            body["resolved_by"] = resolved_by
        response = await self._client.transport.post_json(
            self._client.base_url,
            op_path(
                "agents.secret_requests.by_request_id.fulfill.post",
                request_id=request_id.strip(),
            ),
            body,
        )
        return decode(AgentSecretResolveResponse, response)

    async def deny_secret_request(
        self, request_id: str, *, resolved_by: str | None = None
    ) -> AgentSecretResolveResponse:
        body = {} if resolved_by is None else {"resolved_by": resolved_by}
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path(
                "agents.secret_requests.by_request_id.deny.post",
                request_id=request_id.strip(),
            ),
            body,
        )
        return decode(AgentSecretResolveResponse, value)
