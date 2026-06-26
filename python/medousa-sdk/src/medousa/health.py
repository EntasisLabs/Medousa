from __future__ import annotations

from medousa._decode import decode
from medousa.client import MedousaClient
from medousa.types import HealthResponse


class HealthApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def get(self) -> HealthResponse:
        value = await self._client.transport.get_json(self._client.base_url, "/health")
        return decode(HealthResponse, value)
