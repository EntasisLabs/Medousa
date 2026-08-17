from __future__ import annotations

from typing import TYPE_CHECKING, Any

from medousa._decode import decode
from medousa._ops import op_path
from medousa.types import CapabilityListResponse, CapabilityResolveResponse

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


class CapabilitiesApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def list(self) -> CapabilityListResponse:
        return decode(
            CapabilityListResponse,
            self._client._transport.get_json(self._client.base_url, op_path("capabilities.get")),
        )

    def get(self, capability_id: str) -> CapabilityResolveResponse:
        return decode(
            CapabilityResolveResponse,
            self._client._transport.get_json(
                self._client.base_url,
                op_path("capabilities.by_capability_id.get", capability_id=capability_id),
            ),
        )

    def reindex(self) -> dict[str, Any]:
        return self._client._transport.post_empty_json(
            self._client.base_url,
            op_path("capabilities.reindex.post"),
        )
