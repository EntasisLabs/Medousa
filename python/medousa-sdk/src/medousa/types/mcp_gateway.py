from __future__ import annotations

from medousa.types.daemon_api import MedousaModel


class McpGatewayHealthSnapshot(MedousaModel):
    status: str
    invokes_enabled: bool
    registered_servers: int
    connected_servers: int
    catalog_entries: int


class McpGatewayServerRuntime(MedousaModel):
    server_id: str
    title: str
    enabled: bool
    connected: bool
    tool_count: int
    allowed_lanes: list[str]


class McpGatewayStatusResponse(MedousaModel):
    gateway_url: str
    reachable: bool
    message: str
    health: McpGatewayHealthSnapshot | None = None
    servers: list[McpGatewayServerRuntime] = []
