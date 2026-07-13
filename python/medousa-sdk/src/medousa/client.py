from __future__ import annotations

from typing import TYPE_CHECKING

from medousa.transport import HttpTransport, Transport

if TYPE_CHECKING:
    from medousa.budget import BudgetApi
    from medousa.capabilities import CapabilitiesApi
    from medousa.components import ComponentsApi
    from medousa.environment import EnvironmentApi
    from medousa.feeds import FeedsApi
    from medousa.health import HealthApi
    from medousa.http import HttpApi
    from medousa.ingest import IngestApi
    from medousa.interactive import InteractiveApi
    from medousa.jobs import JobsApi
    from medousa.local_models import LocalModelsApi
    from medousa.mcp_gateway import McpGatewayApi
    from medousa.recurring import RecurringApi
    from medousa.runtime import RuntimeApi
    from medousa.sessions import SessionsApi
    from medousa.vault import VaultApi
    from medousa.calendar import CalendarApi
    from medousa.workspace import WorkspaceApi


class MedousaClient:
    """Async HTTP client for the Medousa daemon API."""

    def __init__(
        self,
        base_url: str,
        *,
        transport: Transport | None = None,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.transport = transport or HttpTransport()

    def health(self) -> HealthApi:
        from medousa.health import HealthApi

        return HealthApi(self)

    def http(self) -> HttpApi:
        from medousa.http import HttpApi

        return HttpApi(self)

    def ingest(self) -> IngestApi:
        from medousa.ingest import IngestApi

        return IngestApi(self)

    def local_models(self) -> LocalModelsApi:
        from medousa.local_models import LocalModelsApi

        return LocalModelsApi(self)

    def jobs(self) -> JobsApi:
        from medousa.jobs import JobsApi

        return JobsApi(self)

    def recurring(self) -> RecurringApi:
        from medousa.recurring import RecurringApi

        return RecurringApi(self)

    def sessions(self) -> SessionsApi:
        from medousa.sessions import SessionsApi

        return SessionsApi(self)

    def interactive(self) -> InteractiveApi:
        from medousa.interactive import InteractiveApi

        return InteractiveApi(self)

    def runtime(self) -> RuntimeApi:
        from medousa.runtime import RuntimeApi

        return RuntimeApi(self)

    def capabilities(self) -> CapabilitiesApi:
        from medousa.capabilities import CapabilitiesApi

        return CapabilitiesApi(self)

    def mcp_gateway(self) -> McpGatewayApi:
        from medousa.mcp_gateway import McpGatewayApi

        return McpGatewayApi(self)

    def budget(self) -> BudgetApi:
        from medousa.budget import BudgetApi

        return BudgetApi(self)

    def vault(self) -> VaultApi:
        from medousa.vault import VaultApi

        return VaultApi(self)

    def calendar(self) -> CalendarApi:
        from medousa.calendar import CalendarApi

        return CalendarApi(self)

    def environment(self) -> EnvironmentApi:
        from medousa.environment import EnvironmentApi

        return EnvironmentApi(self)

    def components(self) -> ComponentsApi:
        from medousa.components import ComponentsApi

        return ComponentsApi(self)

    def feeds(self) -> FeedsApi:
        from medousa.feeds import FeedsApi

        return FeedsApi(self)

    def workspace(self) -> WorkspaceApi:
        from medousa.workspace import WorkspaceApi

        return WorkspaceApi(self)

    async def aclose(self) -> None:
        if isinstance(self.transport, HttpTransport):
            await self.transport.aclose()
