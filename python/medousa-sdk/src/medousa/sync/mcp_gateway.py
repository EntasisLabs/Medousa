from __future__ import annotations

from typing import TYPE_CHECKING

from medousa._decode import decode
from medousa.types import McpGatewayStatusResponse

if TYPE_CHECKING:
    from medousa.sync.client import MedousaClientSync


class McpGatewayApiSync:
    def __init__(self, client: MedousaClientSync) -> None:
        self._client = client

    def status(self) -> McpGatewayStatusResponse:
        return decode(
            McpGatewayStatusResponse,
            self._client._transport.get_json(self._client.base_url, "/v1/mcp/gateway/status"),
        )
