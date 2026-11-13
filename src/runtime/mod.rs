pub mod agent_platform;
pub(crate) mod agent_services;
pub(crate) mod event_delivery;
pub mod locus_semantic_index_store;
pub mod locus_surreal_client;
pub mod memory_bundle;
pub mod persistent_locus;
pub mod platform;
pub mod settings;
pub mod stasis_otel;
pub mod stasis_surreal_schema;
pub mod stasis_wire;
pub mod surreal_startup;
#[cfg(feature = "tui")]
pub mod tui_platform;
pub mod vault_surreal_schema;

pub use platform::{
    MedousaPlatformRuntime, PlatformBuildConfig, build_daemon_platform, build_medousa_platform,
};
#[cfg(feature = "tui")]
pub use tui_platform::{
    TuiPlatformBuildConfig, TuiPlatformMode, build_tui_platform, is_daemon_bind_reachable,
    resolve_tui_platform_mode,
};
