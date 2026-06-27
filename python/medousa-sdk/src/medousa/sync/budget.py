from __future__ import annotations

from typing import TYPE_CHECKING

from medousa._decode import decode
from medousa.types import (
    TurnBudgetApproveRequest,
    TurnBudgetDenyRequest,
    TurnBudgetRequestListResponse,
    TurnBudgetRequestResponse,
)

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


class BudgetApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def list(self, pending_only: bool = False) -> TurnBudgetRequestListResponse:
        path = (
            "/v1/turns/budget-requests?status=pending&limit=20"
            if pending_only
            else "/v1/turns/budget-requests?limit=20"
        )
        return decode(
            TurnBudgetRequestListResponse,
            self._client._transport.get_json(self._client.base_url, path),
        )

    def get(self, request_id: str) -> TurnBudgetRequestResponse:
        value = self._client._transport.get_json(
            self._client.base_url,
            f"/v1/turns/budget-requests/{request_id.strip()}",
        )
        return decode(TurnBudgetRequestResponse, value)

    def approve(
        self,
        request_id: str,
        body: TurnBudgetApproveRequest,
    ) -> TurnBudgetRequestResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            f"/v1/turns/budget-requests/{request_id.strip()}/approve",
            body.model_dump(mode="json", exclude_none=True),
        )
        return decode(TurnBudgetRequestResponse, value)

    def deny(
        self,
        request_id: str,
        body: TurnBudgetDenyRequest,
    ) -> TurnBudgetRequestResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            f"/v1/turns/budget-requests/{request_id.strip()}/deny",
            body.model_dump(mode="json", exclude_none=True),
        )
        return decode(TurnBudgetRequestResponse, value)
