from __future__ import annotations

from typing import TYPE_CHECKING, Any

from medousa._decode import decode
from medousa.types import InteractiveTurnRequest, InteractiveTurnResponse

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


class InteractiveApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def start_turn(self, request: InteractiveTurnRequest) -> InteractiveTurnResponse:
        value = self._client._transport.post_json(
            self._client.base_url,
            "/v1/interactive/turn",
            request.model_dump(mode="json", exclude_none=True),
        )
        return decode(InteractiveTurnResponse, value)

    def cancel(self, session_id: str) -> dict[str, Any]:
        return self._client.sessions().cancel_active_turn(session_id)
