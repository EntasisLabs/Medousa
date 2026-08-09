//! Typed first-party tool contracts and their external runtime adapters.
//!
//! Product policy such as mode exposure, authority, and placement deliberately
//! lives outside this module.

mod catalog;
mod compat;
mod contract;
mod mode;
mod stasis_adapter;

pub use catalog::{
    RegisteredToolContract, RegisteredToolKind, ToolCapabilityId, ToolCapabilityRequirement,
    ToolCatalog, ToolCatalogEntry, ToolCatalogError, ToolCatalogHandle, ToolDomainId, ToolEffect,
    ToolExposureQualifier, ToolExposureRef, ToolId, ToolIdError, ToolIdSource, ToolModeId,
    ToolPlacement, ToolPlacementIndex, ToolPolicyId, ToolRegistrar, ToolRegistration,
    ToolSurfaceId, resolve_tool_id,
};
pub use compat::{deserialize_lenient_optional_string, deserialize_lenient_optional_usize};
pub use contract::{
    ContractError, ExternalJson, OpaqueToolPayload, SchemaNormalizationError, ToolContract,
    TypedTool, build_contract, normalize_input_schema, normalize_output_schema,
};
pub use medousa_tool_macros::medousa_tool;
pub use mode::{EmptyCallMetadata, ModeInputProjection, ModeToolAdapter, ModeToolAdapterError};
pub use stasis_adapter::{deserialize_input, serialize_output};

#[doc(hidden)]
pub mod __private {
    pub use async_trait;
    pub use serde_json::Value;
    pub use stasis::application::orchestration::tool_registry::StasisTool;
    pub use stasis::domain::errors::Result as StasisResult;
    pub use std::sync::OnceLock;
}

#[cfg(test)]
mod tests;
