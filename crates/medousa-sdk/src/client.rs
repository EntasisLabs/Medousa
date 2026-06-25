use std::sync::Arc;

use crate::local::LocalModelsApi;
#[cfg(feature = "async")]
use crate::health::{HealthApi, IngestApi};
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
}
