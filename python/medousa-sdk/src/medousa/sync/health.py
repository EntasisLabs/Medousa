from __future__ import annotations

from typing import TYPE_CHECKING

from medousa._decode import decode
from medousa._generated.ops import by_id
from medousa.types import HealthResponse

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


class HealthApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def get(self) -> HealthResponse:
        value = self._client._transport.get_json(
            self._client.base_url, by_id("health.get").path
        )
        return decode(HealthResponse, value)
