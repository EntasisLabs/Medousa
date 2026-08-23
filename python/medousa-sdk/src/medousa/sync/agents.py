from __future__ import annotations

from medousa._decode import decode
from medousa._ops import op_path, op_path_query
from medousa.sync.client import MedousaClientSync
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


class AgentsApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def list_runtimes(self) -> AgentRuntimeListResponse:
        value = self._client._transport.get_json(
            self._client.base_url, op_path("agents.runtimes.get")
        )
        return decode(AgentRuntimeListResponse, value)

    def create_session(self, request: CreateAgentSessionRequest) -> CreateAgentSessionResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path("agents.sessions.post"),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(CreateAgentSessionResponse, value)

    def prompt(
        self, agent_session_id: str, request: AgentSessionPromptRequest
    ) -> AgentSessionPromptResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path(
                "agents.sessions.by_agent_session_id.prompt.post",
                agent_session_id=agent_session_id.strip(),
            ),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(AgentSessionPromptResponse, value)

    def cancel(self, agent_session_id: str) -> CancelAgentSessionResponse:
        value = self._client._transport.post_empty_json(
            self._client.base_url,
            op_path(
                "agents.sessions.by_agent_session_id.cancel.post",
                agent_session_id=agent_session_id.strip(),
            ),
        )
        return decode(CancelAgentSessionResponse, value)

    def list_permission_requests(
        self, *, status: str | None = "pending", limit: int | None = None
    ) -> AgentPermissionRequestListResponse:
        query: list[tuple[str, str]] = []
        if status is not None:
            query.append(("status", status))
        if limit is not None:
            query.append(("limit", str(limit)))
        route = op_path_query("agents.permission_requests.get", query)
        value = self._client._transport.get_json(self._client.base_url, route)
        return decode(AgentPermissionRequestListResponse, value)

    def approve_permission(
        self, request_id: str, request: AgentPermissionResolveRequest | None = None
    ) -> AgentPermissionResolveResponse:
        body = (request or AgentPermissionResolveRequest()).model_dump(
            mode="json", exclude_none=True
        )
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path(
                "agents.permission_requests.by_request_id.approve.post",
                request_id=request_id.strip(),
            ),
            body,
        )
        return decode(AgentPermissionResolveResponse, value)

    def deny_permission(
        self, request_id: str, request: AgentPermissionResolveRequest | None = None
    ) -> AgentPermissionResolveResponse:
        body = (request or AgentPermissionResolveRequest()).model_dump(
            mode="json", exclude_none=True
        )
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path(
                "agents.permission_requests.by_request_id.deny.post", request_id=request_id.strip()
            ),
            body,
        )
        return decode(AgentPermissionResolveResponse, value)

    def list_secret_requests(
        self, *, status: str | None = "pending", limit: int | None = None
    ) -> AgentSecretRequestListResponse:
        query: list[tuple[str, str]] = []
        if status is not None:
            query.append(("status", status))
        if limit is not None:
            query.append(("limit", str(limit)))
        route = op_path_query("agents.secret_requests.get", query)
        value = self._client._transport.get_json(self._client.base_url, route)
        return decode(AgentSecretRequestListResponse, value)

    def fulfill_secret_request(
        self, request_id: str, value: str, *, resolved_by: str | None = None
    ) -> AgentSecretResolveResponse:
        """Move a write-only value directly to the request's workshop backend."""
        body = {"value": value}
        if resolved_by is not None:
            body["resolved_by"] = resolved_by
        response = self._client._transport.post_json(
            self._client.base_url,
            op_path(
                "agents.secret_requests.by_request_id.fulfill.post",
                request_id=request_id.strip(),
            ),
            body,
        )
        return decode(AgentSecretResolveResponse, response)

    def deny_secret_request(
        self, request_id: str, *, resolved_by: str | None = None
    ) -> AgentSecretResolveResponse:
        body = {} if resolved_by is None else {"resolved_by": resolved_by}
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path(
                "agents.secret_requests.by_request_id.deny.post",
                request_id=request_id.strip(),
            ),
            body,
        )
        return decode(AgentSecretResolveResponse, value)
