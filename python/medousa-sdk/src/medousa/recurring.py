from __future__ import annotations

from medousa._decode import decode
from medousa._ops import op_path
from medousa.client import MedousaClient
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


class RecurringApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def register_prompt(
        self,
        request: RegisterRecurringPromptRequest,
    ) -> RegisterRecurringResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            op_path("recurring.prompt.post"),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(RegisterRecurringResponse, value)

    async def list(self) -> RecurringListResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path("recurring.get"),
        )
        return decode(RecurringListResponse, value)

    async def update(
        self,
        recurring_id: str,
        request: UpdateRecurringRequest,
    ) -> UpdateRecurringResponse:
        value = await self._client.transport.patch_json(
            self._client.base_url,
            op_path("recurring.by_recurring_id.patch", recurring_id=recurring_id),
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(UpdateRecurringResponse, value)

    async def delete(self, recurring_id: str) -> DeleteRecurringResponse:
        value = await self._client.transport.delete_json(
            self._client.base_url,
            op_path("recurring.by_recurring_id.delete", recurring_id=recurring_id),
        )
        return decode(DeleteRecurringResponse, value)

    async def runs(self, recurring_id: str) -> RecurringRunsResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path("recurring.by_recurring_id.runs.get", recurring_id=recurring_id),
        )
        return decode(RecurringRunsResponse, value)

    async def delivery_status(self, recurring_id: str) -> RecurringDeliveryResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path("recurring.by_recurring_id.delivery.get", recurring_id=recurring_id),
        )
        return decode(RecurringDeliveryResponse, value)
