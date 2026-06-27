from __future__ import annotations

from typing import TYPE_CHECKING, Any

from medousa._decode import decode
from medousa._paths import encode_path_segment
from medousa.types import CapabilityListResponse, CapabilityResolveResponse

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


class CapabilitiesApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def list(self) -> CapabilityListResponse:
        return decode(
            CapabilityListResponse,
            self._client._transport.get_json(self._client.base_url, "/v1/capabilities"),
        )

    def get(self, capability_id: str) -> CapabilityResolveResponse:
        path = f"/v1/capabilities/{encode_path_segment(capability_id)}"
        return decode(
            CapabilityResolveResponse,
            self._client._transport.get_json(self._client.base_url, path),
        )

    def reindex(self) -> dict[str, Any]:
        return self._client._transport.post_empty_json(
            self._client.base_url,
            "/v1/capabilities/reindex",
        )
