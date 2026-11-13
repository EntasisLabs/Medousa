//! Compatibility facade; shared runtime assembly is owned by `runtime`.
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use crate::runtime::agent_services::build_agent_runtime_services_for_contract_baseline as build_tui_runtime_services_for_contract_baseline;
#[allow(unused_imports)]
pub(crate) use crate::runtime::agent_services::{
    assemble_agent_runtime as assemble_tui_runtime,
    build_agent_runtime_services as build_tui_runtime_services,
    build_tool_loop_pipeline_for_target,
};
