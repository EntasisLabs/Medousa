//! Dependency-light protocol IR for the Medousa daemon HTTP contract.
//!
//! This crate does not assemble Axum routers or execute application init. The
//! daemon maps declared route inventories into [`OperationSpec`] values; the
//! generator emits OpenAPI, inventories, and low-level client tables from that IR.

mod diff;
mod discrepancy;
mod generate;
mod id;
mod registry;
mod spec;
mod sse;

pub use diff::{CompatibilityClass, ContractDiff, diff_contracts};
pub use discrepancy::{
    DiscrepancyKind, DiscrepancyReport, DiscrepancyRow, ListedRoute, discrepancy_report,
    parse_manifest_yaml, parse_parity_table,
};
pub use generate::{
    GeneratedArtifacts, generate_artifacts, generate_artifacts_with_catalog, generate_python_ops,
    generate_rust_ops, generate_tauri_enum, generate_typescript_ops,
};
pub use id::{const_name, encode_path_segment, expand_path, path_parameters, stable_operation_id};
pub use registry::ContractRegistry;
pub use spec::{
    Audience, ContractError, FeatureProfile, HttpMethod, OperationSpec, ParameterLocation,
    ParameterSpec, RequestBodySpec, ResponseSpec, SchemaRef, Stability, StreamSpec,
    StreamTransport,
};
pub use sse::{SseCodec, SseDecodeError, SseEvent};

pub const OPENAPI_VERSION: &str = "3.2.0";
pub const GENERATOR_ID: &str = "medousa-api-contract";
pub const ERROR_ENVELOPE_SCHEMA_VERSION: u32 = 1;

/// Provenance recorded on every generated artifact.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GeneratorProvenance {
    pub generator: &'static str,
    pub openapi: &'static str,
    pub command: &'static str,
}

pub fn provenance() -> GeneratorProvenance {
    GeneratorProvenance {
        generator: GENERATOR_ID,
        openapi: OPENAPI_VERSION,
        command: "cargo run -p medousa-api-contract -- generate",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_is_not_an_http_method() {
        let err = HttpMethod::parse("SSE").expect_err("SSE must not parse");
        assert!(err.to_string().contains("not an HTTP method"));
    }
}
