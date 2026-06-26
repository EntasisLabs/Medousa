from __future__ import annotations

from medousa._decode import decode
from medousa.client import MedousaClient
from medousa.types import RegisterRecurringPromptRequest, RegisterRecurringResponse


class RecurringApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def register_prompt(
        self,
        request: RegisterRecurringPromptRequest,
    ) -> RegisterRecurringResponse:
        value = await self._client.transport.post_json(
            self._client.base_url,
            "/v1/recurring/prompt",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(RegisterRecurringResponse, value)
