from __future__ import annotations

from medousa._decode import decode
from medousa._ops import op_path, op_path_query
from medousa.client import MedousaClient
from medousa.types import (
    TurnBudgetApproveRequest,
    TurnBudgetDenyRequest,
    TurnBudgetRequestListResponse,
    TurnBudgetRequestResponse,
)


class BudgetApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def list(self, pending_only: bool = False) -> TurnBudgetRequestListResponse:
        path = (
            op_path_query("turns.budget_requests.get", [("status", "pending"), ("limit", "20")])
            if pending_only
            else op_path_query("turns.budget_requests.get", [("limit", "20")])
        )
        value = await self._client.transport.get_json(self._client.base_url, path)
        return decode(TurnBudgetRequestListResponse, value)

    async def get(self, request_id: str) -> TurnBudgetRequestResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path("turns.budget_requests.by_request_id.get", request_id=request_id.strip()),
        )
        return decode(TurnBudgetRequestResponse, value)

    async def approve(
        self,
        request_id: str,
        body: TurnBudgetApproveRequest,
    ) -> TurnBudgetRequestResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path(
                "turns.budget_requests.by_request_id.approve.post", request_id=request_id.strip()
            ),
            body.model_dump(mode="json", exclude_none=True),
        )
        return decode(TurnBudgetRequestResponse, value)

    async def deny(
        self,
        request_id: str,
        body: TurnBudgetDenyRequest,
    ) -> TurnBudgetRequestResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("turns.budget_requests.by_request_id.deny.post", request_id=request_id.strip()),
            body.model_dump(mode="json", exclude_none=True),
        )
        return decode(TurnBudgetRequestResponse, value)
