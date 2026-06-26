"""Synchronous Medousa client (scripts and notebooks)."""

from __future__ import annotations

import httpx

from medousa._decode import decode
from medousa.error import SdkError
from medousa.transport import normalize_path
from medousa.types import (
    ArtifactCommandRequest,
    ArtifactCommandResponse,
    ArtifactFetchRequest,
    ArtifactFetchResponse,
    ArtifactListUiRequest,
    ArtifactListUiResponse,
    CapabilityListResponse,
    CapabilityResolveResponse,
    EnqueueAskRequest,
    EnqueueResponse,
    HealthResponse,
    IngestRequest,
    IngestResponse,
    InteractiveTurnRequest,
    InteractiveTurnResponse,
    McpGatewayStatusResponse,
    RegisterRecurringPromptRequest,
    RegisterRecurringResponse,
    RuntimeConfigCommandRequest,
    RuntimeConfigCommandResponse,
    SessionAppendTurnRequest,
    SessionAppendTurnResponse,
    SessionHistoryListResponse,
    SessionHistoryResponse,
    SessionSetDisplayNameRequest,
    SessionSetDisplayNameResponse,
    StageRouteCommandRequest,
    StageRouteCommandResponse,
    TurnBudgetApproveRequest,
    TurnBudgetDenyRequest,
    TurnBudgetRequestListResponse,
    TurnBudgetRequestResponse,
)
from medousa.types.local import (
    LocalCatalogResponse,
    LocalEngineStatus,
    LocalHardwareResponse,
    LocalModelDownloadResponse,
    LocalModelsResponse,
    ModelDownloadProgress,
)


class _SyncTransport:
    def __init__(self, bearer_token: str | None = None) -> None:
        headers = {}
        if bearer_token:
            headers["Authorization"] = f"Bearer {bearer_token}"
        self._client = httpx.Client(timeout=httpx.Timeout(120.0, connect=10.0), headers=headers)

    def close(self) -> None:
        self._client.close()

    def _url(self, base_url: str, path: str) -> str:
        return f"{base_url.rstrip('/')}{normalize_path(path)}"

    def _raise(self, response: httpx.Response) -> None:
        if response.is_success:
            return
        try:
            body = response.json()
        except Exception:
            body = response.text
        raise SdkError(
            f"HTTP request failed: {response.request.method} {response.request.url}",
            status_code=response.status_code,
            body=body,
        )

    def get_json(self, base_url: str, path: str):
        response = self._client.get(self._url(base_url, path))
        self._raise(response)
        return response.json()

    def post_json(self, base_url: str, path: str, body):
        response = self._client.post(self._url(base_url, path), json=body)
        self._raise(response)
        return response.json() if response.content else {}

    def put_json(self, base_url: str, path: str, body):
        response = self._client.put(self._url(base_url, path), json=body)
        self._raise(response)
        return response.json()

    def post_empty_json(self, base_url: str, path: str):
        response = self._client.post(self._url(base_url, path))
        self._raise(response)
        return response.json() if response.content else {}

    def delete_json(self, base_url: str, path: str):
        response = self._client.delete(self._url(base_url, path))
        self._raise(response)
        return response.json() if response.content else {}


class MedousaClientSync:
    """Blocking HTTP client mirroring MedousaClient accessors."""

    def __init__(self, base_url: str, *, bearer_token: str | None = None) -> None:
        self.base_url = base_url.rstrip("/")
        self._transport = _SyncTransport(bearer_token=bearer_token)

    def close(self) -> None:
        self._transport.close()

    def __enter__(self) -> MedousaClientSync:
        return self

    def __exit__(self, *args: object) -> None:
        self.close()

    def health_get(self) -> HealthResponse:
        return decode(HealthResponse, self._transport.get_json(self.base_url, "/health"))

    def ingest_post(self, request: IngestRequest) -> IngestResponse:
        value = self._transport.post_json(
            self.base_url,
            "/v1/ingest",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(IngestResponse, value)

    def jobs_enqueue_ask(self, request: EnqueueAskRequest) -> EnqueueResponse:
        value = self._transport.post_json(
            self.base_url,
            "/v1/jobs/ask",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(EnqueueResponse, value)

    def recurring_register_prompt(
        self,
        request: RegisterRecurringPromptRequest,
    ) -> RegisterRecurringResponse:
        value = self._transport.post_json(
            self.base_url,
            "/v1/recurring/prompt",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(RegisterRecurringResponse, value)

    def sessions_list(self, limit: int = 50) -> SessionHistoryListResponse:
        value = self._transport.get_json(self.base_url, f"/v1/sessions?limit={limit}")
        return decode(SessionHistoryListResponse, value)

    def sessions_history(self, session_id: str) -> SessionHistoryResponse:
        value = self._transport.get_json(self.base_url, f"/v1/sessions/{session_id}/history")
        return decode(SessionHistoryResponse, value)

    def sessions_set_display_name(
        self,
        session_id: str,
        display_name: str,
    ) -> SessionSetDisplayNameResponse:
        body = SessionSetDisplayNameRequest(display_name=display_name)
        value = self._transport.put_json(
            self.base_url,
            f"/v1/sessions/{session_id}/name",
            body.model_dump(mode="json"),
        )
        return decode(SessionSetDisplayNameResponse, value)

    def sessions_append_turn(
        self,
        session_id: str,
        request: SessionAppendTurnRequest,
    ) -> SessionAppendTurnResponse:
        value = self._transport.post_json(
            self.base_url,
            f"/v1/sessions/{session_id}/turns",
            request.model_dump(mode="json"),
        )
        return decode(SessionAppendTurnResponse, value)

    def local_models_hardware(self) -> LocalHardwareResponse:
        return decode(
            LocalHardwareResponse,
            self._transport.get_json(self.base_url, "/v1/local/hardware"),
        )

    def local_models_catalog(self) -> LocalCatalogResponse:
        return decode(
            LocalCatalogResponse,
            self._transport.get_json(self.base_url, "/v1/local/catalog"),
        )

    def local_models_list(self) -> LocalModelsResponse:
        return decode(
            LocalModelsResponse,
            self._transport.get_json(self.base_url, "/v1/local/models"),
        )

    def local_models_engine_status(self) -> LocalEngineStatus:
        return decode(
            LocalEngineStatus,
            self._transport.get_json(self.base_url, "/v1/local/engine/status"),
        )

    def local_models_start_download(self, model_id: str) -> LocalModelDownloadResponse:
        value = self._transport.post_json(
            self.base_url,
            "/v1/local/models/download",
            {"modelId": model_id},
        )
        return decode(LocalModelDownloadResponse, value)

    def local_models_download_status(self, job_id: str) -> ModelDownloadProgress:
        value = self._transport.get_json(
            self.base_url,
            f"/v1/local/models/download/{job_id}",
        )
        return decode(ModelDownloadProgress, value)

    def local_models_remove_model(self, model_id: str) -> dict:
        return self._transport.delete_json(self.base_url, f"/v1/local/models/{model_id}")

    def interactive_start_turn(self, request: InteractiveTurnRequest) -> InteractiveTurnResponse:
        value = self._transport.post_json(
            self.base_url,
            "/v1/interactive/turn",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(InteractiveTurnResponse, value)

    def runtime_artifact_command(
        self,
        request: ArtifactCommandRequest,
    ) -> ArtifactCommandResponse:
        value = self._transport.post_json(
            self.base_url,
            "/v1/runtime/artifact/command",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(ArtifactCommandResponse, value)

    def runtime_artifact_fetch(self, request: ArtifactFetchRequest) -> ArtifactFetchResponse:
        value = self._transport.post_json(
            self.base_url,
            "/v1/runtime/artifact/fetch",
            request.model_dump(mode="json"),
        )
        return decode(ArtifactFetchResponse, value)

    def runtime_artifact_list_ui(self, request: ArtifactListUiRequest) -> ArtifactListUiResponse:
        value = self._transport.post_json(
            self.base_url,
            "/v1/runtime/artifact/list-ui",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(ArtifactListUiResponse, value)

    def runtime_config_command(
        self,
        request: RuntimeConfigCommandRequest,
    ) -> RuntimeConfigCommandResponse:
        value = self._transport.post_json(
            self.base_url,
            "/v1/runtime/config/command",
            request.model_dump(mode="json"),
        )
        return decode(RuntimeConfigCommandResponse, value)

    def runtime_stage_route_command(
        self,
        request: StageRouteCommandRequest,
    ) -> StageRouteCommandResponse:
        value = self._transport.post_json(
            self.base_url,
            "/v1/runtime/stage-route/command",
            request.model_dump(mode="json"),
        )
        return decode(StageRouteCommandResponse, value)

    def capabilities_list(self) -> CapabilityListResponse:
        return decode(
            CapabilityListResponse,
            self._transport.get_json(self.base_url, "/v1/capabilities"),
        )

    def capabilities_get(self, capability_id: str) -> CapabilityResolveResponse:
        from urllib.parse import quote

        path = f"/v1/capabilities/{quote(capability_id, safe='')}"
        return decode(CapabilityResolveResponse, self._transport.get_json(self.base_url, path))

    def mcp_gateway_status(self) -> McpGatewayStatusResponse:
        return decode(
            McpGatewayStatusResponse,
            self._transport.get_json(self.base_url, "/v1/mcp/gateway/status"),
        )

    def budget_list(self, pending_only: bool = False) -> TurnBudgetRequestListResponse:
        path = (
            "/v1/turns/budget-requests?status=pending&limit=20"
            if pending_only
            else "/v1/turns/budget-requests?limit=20"
        )
        return decode(
            TurnBudgetRequestListResponse,
            self._transport.get_json(self.base_url, path),
        )

    def budget_approve(
        self,
        request_id: str,
        body: TurnBudgetApproveRequest,
    ) -> TurnBudgetRequestResponse:
        value = self._transport.post_json(
            self.base_url,
            f"/v1/turns/budget-requests/{request_id.strip()}/approve",
            body.model_dump(mode="json", exclude_none=True),
        )
        return decode(TurnBudgetRequestResponse, value)

    def budget_deny(
        self,
        request_id: str,
        body: TurnBudgetDenyRequest,
    ) -> TurnBudgetRequestResponse:
        value = self._transport.post_json(
            self.base_url,
            f"/v1/turns/budget-requests/{request_id.strip()}/deny",
            body.model_dump(mode="json", exclude_none=True),
        )
        return decode(TurnBudgetRequestResponse, value)
