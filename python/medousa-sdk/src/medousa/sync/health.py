from __future__ import annotations

from typing import TYPE_CHECKING

from medousa._decode import decode
from medousa.types import HealthResponse

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


class HealthApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def get(self) -> HealthResponse:
        value = self._client._transport.get_json(self._client.base_url, "/health")
        return decode(HealthResponse, value)
