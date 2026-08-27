//! Personal-mobile deployment composition.
//!
//! Tool behavior lives in the canonical modules. This recipe only supplies
//! portable services and selects the mobile-compatible registration group.

use std::sync::Arc;

use stasis::domain::errors::Result as StasisResult;
use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
use tokio::sync::{RwLock, mpsc};

use crate::capability_catalog::CapabilityRegistry;
use crate::embedded_daemon::{
    EmbeddedToolRegistryAssembly, EmbeddedToolRegistryBindings, EmbeddedToolRegistryRecipe,
};
use crate::events::TuiEvent;
use crate::grapheme_sttp_compaction::GraphemeCompactionModelTarget;
use crate::tool_registration_groups::SharedToolRegistrationBindings;
use crate::typed_tools::ToolCatalogHandle;

pub const PERSONAL_MOBILE_TOOL_NAMES: &[&str] = &[
    "cognition_tools_discover",
    "cognition_web_search",
    "cognition_capability",
    "cognition_schema",
    "cognition_runtime_query",
    "cognition_runtime_mutate",
    "cognition_identity_query",
    "cognition_identity_mutate",
    "cognition_manuscript_list",
    "cognition_manuscript_resolve",
    "cognition_skill_discover",
    "cognition_skill_propose",
    "cognition_store_read",
    "cognition_store_write",
    "cognition_memory_query",
    "cognition_memory_mutate",
    "cognition_calendar_query",
    "cognition_calendar_mutate",
    "cognition_grapheme_request_secret",
    "cognition_ui_build",
    "cognition_ui_present",
    "cognition_ui_scene",
    "cognition_environment_get",
    "cognition_environment_apply",
    "cognition_environment_propose",
    "cognition_environment_wiki",
    "cognition_layout_get",
    "cognition_layout_apply",
    "cognition_layout_reset",
    "cognition_intent_resolve",
    "cognition_feed_subscribe",
    "cognition_feed_publish",
    "cognition_custom_view_compose",
    "cognition_context_follow_pointer",
    "cognition_context_list_pointers",
    "cognition_chat_history_search",
    "cognition_chat_history_read",
    "cognition_tool_history_summary",
    "cognition_tool_history_detail",
    "cognition_manuscript_overlay_propose",
    "cognition_manuscript_overlay_list",
    "cognition_browser_fetch",
    "cognition_browser_snapshot",
    "cognition_browser_act",
    "cognition_utility_time_now",
    "cognition_utility_day_of_week",
    "cognition_utility_uuid",
    medousa_runtime::turn_control::COGNITION_TURN,
];

#[derive(Debug, Default)]
pub struct PersonalMobileToolRegistryRecipe;

impl EmbeddedToolRegistryRecipe for PersonalMobileToolRegistryRecipe {
    fn assemble(
        &self,
        bindings: EmbeddedToolRegistryBindings,
    ) -> StasisResult<EmbeddedToolRegistryAssembly> {
        let catalog_handle = ToolCatalogHandle::default();
        let mut assembly =
            EmbeddedToolRegistryAssembly::new(crate::tool_catalog::first_party_placement_index());
        let (event_tx, event_rx) = mpsc::channel::<TuiEvent>(64);
        drop(event_rx);
        let shared = SharedToolRegistrationBindings {
            runtime: bindings.runtime,
            event_tx,
            turn_scope: crate::execution_context::TurnScopeAccess::default(),
            workflow_registry: crate::workflow::shared_workflow_registry(),
            identity_service: Arc::new(
                stasis::application::use_cases::identity_memory_service::IdentityMemoryService::new(
                    bindings.identity_store.clone() as Arc<dyn IdentityMemoryStore>,
                ),
            ),
            identity_store: bindings.identity_store,
            memory_reader: bindings.memory_reader,
            memory_writer: bindings.memory_writer,
            locus_store: bindings.locus_store,
            semantic_index: bindings.semantic_index,
            memory_operations: bindings.memory_operations,
            session_id: "embedded".to_string(),
            workshop_operator_identity: true,
            compaction_target: GraphemeCompactionModelTarget {
                provider: bindings.provider,
                model: bindings.model,
                base_url: None,
                chat_client: Some(bindings.chat_client),
            },
            catalog_handle: catalog_handle.clone(),
            capability_registry: Arc::new(RwLock::new(CapabilityRegistry::with_loaded_manifest())),
            mcp_gateway_client: bindings.mcp_gateway_client,
        };
        crate::tool_registration_groups::register_portable_foundation_tools(
            assembly.registrar(),
            &shared,
        )?;
        crate::tool_registration_groups::register_shared_secret_tools(
            assembly.registrar(),
            &shared,
        )?;
        crate::tool_registration_groups::register_portable_interactive_tools(
            assembly.registrar(),
            &shared,
        )?;
        assembly.initialize_handle_after_finish(catalog_handle);
        Ok(assembly)
    }
}
