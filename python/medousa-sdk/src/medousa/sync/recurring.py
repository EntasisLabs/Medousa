from __future__ import annotations

from typing import TYPE_CHECKING

from medousa._decode import decode
from medousa._ops import op_path
from medousa.types import (
    DeleteRecurringResponse,
    RecurringDeliveryResponse,
    RecurringListResponse,
    RecurringRunsResponse,
    RegisterRecurringPromptRequest,
    RegisterRecurringResponse,
    UpdateRecurringRequest,
    UpdateRecurringResponse,
)

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


class RecurringApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def register_prompt(
        self,
        request: RegisterRecurringPromptRequest,
    ) -> RegisterRecurringResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            op_path("recurring.prompt.post"),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(RegisterRecurringResponse, value)

    def list(self) -> RecurringListResponse:
        value = self._client._transport.get_json(self._client.base_url, op_path("recurring.get"))
        return decode(RecurringListResponse, value)

    def update(
        self,
        recurring_id: str,
        request: UpdateRecurringRequest,
    ) -> UpdateRecurringResponse:
        value = self._client._transport.patch_json(
            self._client.base_url,
            op_path("recurring.by_recurring_id.patch", recurring_id=recurring_id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(UpdateRecurringResponse, value)

    def delete(self, recurring_id: str) -> DeleteRecurringResponse:
        value = self._client._transport.delete_json(
            self._client.base_url,
            op_path("recurring.by_recurring_id.delete", recurring_id=recurring_id),
        )
        return decode(DeleteRecurringResponse, value)

    def runs(self, recurring_id: str) -> RecurringRunsResponse:
        value = self._client._transport.get_json(
            self._client.base_url,
            op_path("recurring.by_recurring_id.runs.get", recurring_id=recurring_id),
        )
        return decode(RecurringRunsResponse, value)

    def delivery_status(self, recurring_id: str) -> RecurringDeliveryResponse:
        value = self._client._transport.get_json(
            self._client.base_url,
            op_path("recurring.by_recurring_id.delivery.get", recurring_id=recurring_id),
        )
        return decode(RecurringDeliveryResponse, value)
