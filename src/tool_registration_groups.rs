//! Dependency-shaped tool registration groups shared by daemon compositions.

use std::sync::Arc;

use locus_core_rs::NodeStore;
use stasis::application::use_cases::identity_memory_service::IdentityMemoryService;
use stasis::prelude_ext::{MemoryContextReader, MemoryContextWriter};
use tokio::sync::RwLock;
use tokio::sync::mpsc;

use crate::bridge_tools::CapabilityWebSearchBackend;
use crate::capability_catalog::CapabilityRegistry;
use crate::events::TuiEvent;
use crate::grapheme_sttp_compaction::GraphemeCompactionModelTarget;
use crate::identity_store_ext::MedousaIdentityMemoryStore;
use crate::mcp_gateway_client::McpGatewayClient;
use crate::tools::{
    CognitionUtilityDayOfWeekTool, CognitionUtilityTimeNowTool, CognitionUtilityUuidTool,
};
use crate::typed_tools::ToolRegistration;
#[cfg(feature = "full-daemon")]
use crate::typed_tools::ToolRegistrar;
use crate::web_search_tool::CognitionWebSearchTool;
use crate::workflow::WorkflowRegistry;

/// Runtime services consumed by the tools shared across daemon deployments.
#[derive(Clone)]
pub struct SharedToolRegistrationBindings {
    pub runtime: Arc<stasis::prelude::RuntimeComposition>,
    pub event_tx: mpsc::Sender<TuiEvent>,
    pub turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
    pub workflow_registry: Arc<WorkflowRegistry>,
    pub identity_service: Arc<IdentityMemoryService>,
    pub identity_store: Arc<MedousaIdentityMemoryStore>,
    pub memory_reader: Arc<dyn MemoryContextReader>,
    pub memory_writer: Arc<dyn MemoryContextWriter>,
    pub locus_store: Arc<dyn NodeStore>,
    pub semantic_index: Arc<dyn locus_core_rs::SemanticIndexStore>,
    pub memory_operations:
        Arc<dyn stasis::ports::outbound::memory::memory_operations::MemoryOperations>,
    pub session_id: String,
    pub workshop_operator_identity: bool,
    pub compaction_target: GraphemeCompactionModelTarget,
    pub catalog_handle: crate::typed_tools::ToolCatalogHandle,
    #[cfg(feature = "full-daemon")]
    pub worker_scheduler: Arc<crate::agent_runtime::turn_worker::TurnWorkerScheduler>,
    pub capability_registry: Arc<RwLock<CapabilityRegistry>>,
    pub mcp_gateway_client: Arc<McpGatewayClient>,
}

/// Register portable foundation tools from one canonical implementation list.
pub fn register_portable_foundation_tools(
    registry: &mut impl ToolRegistration,
    bindings: &SharedToolRegistrationBindings,
) -> stasis::prelude::Result<()> {
    crate::schema_api::register_schema_tools(registry)?;
    crate::runtime_api::register_runtime_api_tools(
        registry,
        bindings.runtime.clone(),
        bindings.event_tx.clone(),
        bindings.turn_scope.clone(),
        bindings.workflow_registry.clone(),
    )?;
    crate::identity_api::register_identity_tools(
        registry,
        bindings.identity_service.clone(),
        bindings.identity_store.clone(),
        Some(bindings.memory_writer.clone()),
        crate::identity_memory::resolve_tool_identity_user_id(
            &bindings.session_id,
            bindings.workshop_operator_identity,
        ),
        crate::identity_memory::resolve_identity_persona_id(),
        crate::identity_memory::resolve_identity_channel_id(Some("interactive")),
        bindings.workshop_operator_identity,
        bindings.event_tx.clone(),
    )?;
    crate::manuscript_tools::register_manuscript_tools(registry)?;
    Ok(())
}

/// Register the shared secret-request tool between the two desktop families.
pub fn register_shared_secret_tools(
    registry: &mut impl ToolRegistration,
    bindings: &SharedToolRegistrationBindings,
) -> stasis::prelude::Result<()> {
    crate::grapheme_secret_tools::register_grapheme_secret_tools(
        registry,
        bindings.turn_scope.clone(),
    )
}

/// Register the remaining tools shared by every complete daemon composition.
#[cfg(feature = "full-daemon")]
pub fn register_shared_interactive_tools(
    registry: &mut impl ToolRegistration,
    bindings: &SharedToolRegistrationBindings,
) -> stasis::prelude::Result<()> {
    register_portable_interactive_tools(registry, bindings)?;
    crate::skill_tools::register_skill_probe_tool(
        registry,
        bindings.runtime.clone(),
        bindings.event_tx.clone(),
        bindings.turn_scope.clone(),
    )?;
    crate::workshop_api::register_workshop_tools(registry, bindings.worker_scheduler.clone())?;
    Ok(())
}

/// Register canonical tools whose dependencies are portable across daemon hosts.
pub fn register_portable_interactive_tools(
    registry: &mut impl ToolRegistration,
    bindings: &SharedToolRegistrationBindings,
) -> stasis::prelude::Result<()> {
    crate::ui_present_tools::register_ui_present_tools(registry, bindings.turn_scope.clone())?;
    crate::skill_tools::register_portable_skill_tools(registry)?;
    crate::environment_tools::register_environment_tools(registry, bindings.turn_scope.clone())?;
    crate::layout_tools::register_layout_tools(registry)?;
    crate::feed_tools::register_feed_tools(
        registry,
        bindings.capability_registry.clone(),
        bindings.turn_scope.clone(),
    )?;
    crate::custom_view_tools::register_custom_view_tools(
        registry,
        bindings.runtime.clone(),
        bindings.event_tx.clone(),
        bindings.turn_scope.clone(),
    )?;
    crate::context_pointer_tools::register_context_pointer_tools(
        registry,
        bindings.turn_scope.clone(),
    )?;
    crate::chat_history_tools::register_chat_history_tools(registry, bindings.turn_scope.clone())?;
    crate::tool_history_tools::register_tool_history_tools(registry, bindings.turn_scope.clone())?;
    #[cfg(feature = "full-daemon")]
    crate::turn_api::register_turn_tools(
        registry,
        bindings.worker_scheduler.clone(),
        bindings.session_id.clone(),
        bindings.turn_scope.clone(),
    )?;
    #[cfg(not(feature = "full-daemon"))]
    crate::turn_api::register_turn_tools(
        registry,
        bindings.session_id.clone(),
        bindings.turn_scope.clone(),
    )?;
    crate::manuscript_overlay_tools::register_manuscript_overlay_tools(registry)?;
    crate::store_tools::register_store_tools(
        registry,
        bindings.event_tx.clone(),
        bindings.turn_scope.clone(),
        bindings.session_id.clone(),
    )?;
    crate::artifact_tools::register_artifact_tools(
        registry,
        bindings.event_tx.clone(),
        bindings.turn_scope.clone(),
    )?;
    crate::vault_tools::register_vault_tools(
        registry,
        bindings.event_tx.clone(),
        bindings.turn_scope.clone(),
        bindings.session_id.clone(),
    )?;
    crate::calendar_api::register_calendar_tools(registry, bindings.event_tx.clone())?;
    crate::ui_scene_tools::register_ui_scene_tools(registry, bindings.turn_scope.clone())?;
    crate::ui_build_tools::register_ui_build_tools(registry, bindings.turn_scope.clone())?;
    crate::grapheme_script_tools::register_grapheme_script_tools(
        registry,
        bindings.event_tx.clone(),
    )?;
    crate::tool_bootstrap_tools::register_tool_bootstrap_tools(
        registry,
        bindings.turn_scope.clone(),
        bindings.catalog_handle.clone(),
    )?;
    crate::memory_api::register_memory_tools(
        registry,
        bindings.locus_store.clone(),
        bindings.memory_reader.clone(),
        bindings.memory_writer.clone(),
        bindings.semantic_index.clone(),
        bindings.memory_operations.clone(),
        bindings.session_id.clone(),
        bindings.workshop_operator_identity,
        bindings.turn_scope.clone(),
        bindings.event_tx.clone(),
    )?;
    registry.register_typed_tool(CognitionUtilityTimeNowTool)?;
    registry.register_typed_tool(CognitionUtilityDayOfWeekTool)?;
    registry.register_typed_tool(CognitionUtilityUuidTool)?;
    crate::capability_tools::register_capability_tools(
        registry,
        bindings.runtime.clone(),
        bindings.event_tx.clone(),
        bindings.session_id.clone(),
        bindings.turn_scope.clone(),
        bindings.compaction_target.clone(),
        bindings.capability_registry.clone(),
        bindings.mcp_gateway_client.clone(),
    )?;
    registry.register_typed_tool(CognitionWebSearchTool::new(Arc::new(
        CapabilityWebSearchBackend::new(
            bindings.capability_registry.clone(),
            bindings.runtime.clone(),
            bindings.mcp_gateway_client.clone(),
            bindings.session_id.clone(),
            bindings.turn_scope.clone(),
            bindings.event_tx.clone(),
        ),
    )))?;
    crate::browser_fetch_tools::register_browser_fetch_tool(
        registry,
        bindings.turn_scope.clone(),
        bindings.event_tx.clone(),
    )?;
    crate::browser_snapshot_tools::register_browser_snapshot_tool(
        registry,
        bindings.turn_scope.clone(),
        bindings.event_tx.clone(),
    )?;
    crate::browser_act_tools::register_browser_act_tool(
        registry,
        bindings.turn_scope.clone(),
        bindings.event_tx.clone(),
    )?;
    Ok(())
}

/// Register the OpenShell tools that require the desktop sidecar.
#[cfg(feature = "full-daemon")]
pub fn register_desktop_openshell_tools(
    registry: &mut impl ToolRegistration,
    runtime: Arc<stasis::prelude::RuntimeComposition>,
    event_tx: mpsc::Sender<TuiEvent>,
    turn_scope: crate::agent_runtime::execution_context::TurnScopeAccess,
) -> stasis::prelude::Result<()> {
    crate::openshell_tools::register_openshell_tools(registry, runtime, event_tx, turn_scope)
}

/// Register desktop execution tools backed by shell, PTY, Forge, or Detamu.
#[cfg(feature = "full-daemon")]
pub fn register_desktop_coding_tools(
    registry: &mut impl ToolRegistration,
    runtime: Arc<stasis::prelude::RuntimeComposition>,
) -> stasis::prelude::Result<()> {
    crate::shell_tools::register_shell_tools(registry, runtime)?;
    crate::code_intelligence_tools::register_code_intelligence_tools(registry)?;
    crate::coding_tools::register_coding_tools(registry)?;
    crate::detamu_tools::register_detamu_tools(registry)?;
    Ok(())
}

/// Register catalog-only adapters used by Forge-fenced Coder turns.
#[cfg(feature = "full-daemon")]
pub fn register_desktop_coder_catalog_adapters(
    registry: &mut ToolRegistrar,
) -> Result<(), crate::typed_tools::ToolCatalogError> {
    crate::agent_runtime::coder_tools::register_catalog_runtime_adapters(registry)
}
