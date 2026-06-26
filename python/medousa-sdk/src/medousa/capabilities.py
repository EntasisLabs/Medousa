from __future__ import annotations

from typing import Any

from medousa._decode import decode
from medousa.client import MedousaClient
from medousa.types import CapabilityListResponse, CapabilityResolveResponse


class CapabilitiesApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def list(self) -> CapabilityListResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            "/v1/capabilities",
        )
        return decode(CapabilityListResponse, value)

    async def get(self, capability_id: str) -> CapabilityResolveResponse:
        from urllib.parse import quote

        path = f"/v1/capabilities/{quote(capability_id, safe='')}"
        value = await self._client.transport.get_json(self._client.base_url, path)
        return decode(CapabilityResolveResponse, value)

    async def reindex(self) -> dict[str, Any]:
        return await self._client.transport.post_empty_json(
            self._client.base_url,
            "/v1/capabilities/reindex",
        )
