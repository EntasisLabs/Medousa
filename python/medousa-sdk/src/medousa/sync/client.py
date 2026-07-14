from __future__ import annotations

from typing import TYPE_CHECKING

from medousa.sync.transport import SyncTransport

if TYPE_CHECKING:
    from medousa.sync.budget import BudgetApiSync
    from medousa.sync.capabilities import CapabilitiesApiSync
    from medousa.sync.components import ComponentsApiSync
    from medousa.sync.environment import EnvironmentApiSync
    from medousa.sync.feeds import FeedsApiSync
    from medousa.sync.health import HealthApiSync
    from medousa.sync.ingest import IngestApiSync
    from medousa.sync.interactive import InteractiveApiSync
    from medousa.sync.jobs import JobsApiSync
    from medousa.sync.local_models import LocalModelsApiSync
    from medousa.sync.mcp_gateway import McpGatewayApiSync
    from medousa.sync.recurring import RecurringApiSync
    from medousa.sync.runtime import RuntimeApiSync
    from medousa.sync.sessions import SessionsApiSync
    from medousa.sync.vault import VaultApiSync
    from medousa.sync.calendar import CalendarApiSync
    from medousa.sync.workspace import WorkspaceApiSync


class MedousaClientSync:
    """Blocking HTTP client mirroring MedousaClient accessors."""

    def __init__(self, base_url: str, *, bearer_token: str | None = None) -> None:
        self.base_url = base_url.rstrip("/")
        self._transport = SyncTransport(bearer_token=bearer_token)

    def close(self) -> None:
        self._transport.close()

    def __enter__(self) -> MedousaClientSync:
        return self

    def __exit__(self, *args: object) -> None:
        self.close()

    def health(self) -> HealthApiSync:
        from medousa.sync.health import HealthApiSync

        return HealthApiSync(self)

    def ingest(self) -> IngestApiSync:
        from medousa.sync.ingest import IngestApiSync

        return IngestApiSync(self)

    def local_models(self) -> LocalModelsApiSync:
        from medousa.sync.local_models import LocalModelsApiSync

        return LocalModelsApiSync(self)

    def jobs(self) -> JobsApiSync:
        from medousa.sync.jobs import JobsApiSync

        return JobsApiSync(self)

    def recurring(self) -> RecurringApiSync:
        from medousa.sync.recurring import RecurringApiSync

        return RecurringApiSync(self)

    def sessions(self) -> SessionsApiSync:
        from medousa.sync.sessions import SessionsApiSync

        return SessionsApiSync(self)

    def interactive(self) -> InteractiveApiSync:
        from medousa.sync.interactive import InteractiveApiSync

        return InteractiveApiSync(self)

    def runtime(self) -> RuntimeApiSync:
        from medousa.sync.runtime import RuntimeApiSync

        return RuntimeApiSync(self)

    def capabilities(self) -> CapabilitiesApiSync:
        from medousa.sync.capabilities import CapabilitiesApiSync

        return CapabilitiesApiSync(self)

    def mcp_gateway(self) -> McpGatewayApiSync:
        from medousa.sync.mcp_gateway import McpGatewayApiSync

        return McpGatewayApiSync(self)

    def budget(self) -> BudgetApiSync:
        from medousa.sync.budget import BudgetApiSync

        return BudgetApiSync(self)

    def vault(self) -> VaultApiSync:
        from medousa.sync.vault import VaultApiSync

        return VaultApiSync(self)

    def calendar(self) -> CalendarApiSync:
        from medousa.sync.calendar import CalendarApiSync

        return CalendarApiSync(self)

    def environment(self) -> EnvironmentApiSync:
        from medousa.sync.environment import EnvironmentApiSync

        return EnvironmentApiSync(self)

    def components(self) -> ComponentsApiSync:
        from medousa.sync.components import ComponentsApiSync

        return ComponentsApiSync(self)

    def feeds(self) -> FeedsApiSync:
        from medousa.sync.feeds import FeedsApiSync

        return FeedsApiSync(self)

    def workspace(self) -> WorkspaceApiSync:
        from medousa.sync.workspace import WorkspaceApiSync

        return WorkspaceApiSync(self)
