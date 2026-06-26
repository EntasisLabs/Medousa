use std::sync::Arc;

use crate::local::LocalModelsApi;
#[cfg(feature = "async")]
use crate::budget::BudgetApi;
#[cfg(feature = "async")]
use crate::capabilities::CapabilitiesApi;
#[cfg(feature = "async")]
use crate::health::{HealthApi, IngestApi};
#[cfg(feature = "async")]
use crate::http::HttpApi;
#[cfg(feature = "async")]
use crate::interactive::InteractiveApi;
#[cfg(feature = "async")]
use crate::jobs::{JobsApi, RecurringApi};
#[cfg(feature = "async")]
use crate::mcp_gateway::McpGatewayApi;
#[cfg(feature = "async")]
use crate::runtime::RuntimeApi;
#[cfg(feature = "async")]
use crate::sessions::SessionsApi;
use crate::transport::Transport;

pub struct MedousaClient {
    transport: Arc<dyn Transport>,
    base_url: String,
}

impl MedousaClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self::with_transport(Arc::new(crate::transport::HttpTransport::new()), base_url)
    }

    pub fn with_transport(transport: Arc<dyn Transport>, base_url: impl Into<String>) -> Self {
        Self {
            transport,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn transport(&self) -> &Arc<dyn Transport> {
        &self.transport
    }

    #[cfg(feature = "async")]
    pub fn health(&self) -> HealthApi<'_> {
        HealthApi {
            client: self,
        }
    }

    #[cfg(feature = "async")]
    pub fn http(&self) -> HttpApi<'_> {
        HttpApi { client: self }
    }

    #[cfg(feature = "async")]
    pub fn capabilities(&self) -> CapabilitiesApi<'_> {
        CapabilitiesApi { client: self }
    }

    #[cfg(feature = "async")]
    pub fn ingest(&self) -> IngestApi<'_> {
        IngestApi {
            client: self,
        }
    }

    pub fn local_models(&self) -> LocalModelsApi<'_> {
        LocalModelsApi {
            client: self,
        }
    }

    #[cfg(feature = "async")]
    pub fn jobs(&self) -> JobsApi<'_> {
        JobsApi { client: self }
    }

    #[cfg(feature = "async")]
    pub fn recurring(&self) -> RecurringApi<'_> {
        RecurringApi { client: self }
    }

    #[cfg(feature = "async")]
    pub fn sessions(&self) -> SessionsApi<'_> {
        SessionsApi { client: self }
    }

    #[cfg(feature = "async")]
    pub fn interactive(&self) -> InteractiveApi<'_> {
        InteractiveApi { client: self }
    }

    #[cfg(feature = "async")]
    pub fn runtime(&self) -> RuntimeApi<'_> {
        RuntimeApi { client: self }
    }

    #[cfg(feature = "async")]
    pub fn mcp_gateway(&self) -> McpGatewayApi<'_> {
        McpGatewayApi { client: self }
    }

    #[cfg(feature = "async")]
    pub fn budget(&self) -> BudgetApi<'_> {
        BudgetApi { client: self }
    }
}
