from __future__ import annotations

from typing import Any

from medousa._decode import decode
from medousa._ops import op_path
from medousa.client import MedousaClient
from medousa.types import CapabilityListResponse, CapabilityResolveResponse


class CapabilitiesApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def list(self) -> CapabilityListResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path("capabilities.get"),
        )
        return decode(CapabilityListResponse, value)

    async def get(self, capability_id: str) -> CapabilityResolveResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path("capabilities.by_capability_id.get", capability_id=capability_id),
        )
        return decode(CapabilityResolveResponse, value)

    async def reindex(self) -> dict[str, Any]:
        return await self._client.transport.post_empty_json(
            self._client.base_url,
            op_path("capabilities.reindex.post"),
        )
