//! Agent-facing public library: always-callable primitives.
//!
//! Modes switch advertised ambient, not whether these names exist. Backend
//! physics (lease, roots, sandbox, client capabilities) still decide success.

use std::collections::HashSet;

pub const COGNITION_STORE_READ: &str = "cognition_store_read";
pub const COGNITION_STORE_WRITE: &str = "cognition_store_write";
pub const COGNITION_CAPABILITY: &str = "cognition_capability";
pub const COGNITION_SCHEMA: &str = "cognition_schema";
pub const COGNITION_RUNTIME_QUERY: &str = "cognition_runtime_query";
pub const COGNITION_RUNTIME_MUTATE: &str = "cognition_runtime_mutate";

/// Primitives that every mode may call. Grows as families collapse.
pub const PUBLIC_API_TOOLS: &[&str] = &[
    COGNITION_STORE_READ,
    COGNITION_STORE_WRITE,
    COGNITION_CAPABILITY,
    COGNITION_SCHEMA,
    COGNITION_RUNTIME_QUERY,
    COGNITION_RUNTIME_MUTATE,
];

pub fn is_public_api_tool(name: &str) -> bool {
    PUBLIC_API_TOOLS.contains(&name)
}

pub fn ensure_public_api(names: &mut HashSet<String>) {
    for name in PUBLIC_API_TOOLS {
        names.insert((*name).to_string());
    }
}
