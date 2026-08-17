from __future__ import annotations

from medousa._decode import decode
from medousa._ops import op_path
from medousa.client import MedousaClient
from medousa.types import McpGatewayStatusResponse


class McpGatewayApi:
    def __init__(self, client: MedousaClient) -> None:
        self._client = client

    async def status(self) -> McpGatewayStatusResponse:
        value = await self._client.transport.get_json(
            self._client.base_url,
            op_path("mcp.gateway.status.get"),
        )
        return decode(McpGatewayStatusResponse, value)
