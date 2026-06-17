//! Medousa Home workspace — work board projections + activity feed (Phase W1).

pub mod actions;
pub mod ask_job_finalize;
pub mod ask_job_store;
pub mod card;
pub mod domain_event;
pub mod event;
pub mod feed;
pub mod incremental;
pub mod persist;
pub mod projector;
pub mod retention;
pub mod service;
pub mod store;

pub use actions::replay_runtime_job;
pub use domain_event::{
    notify_workspace_event, notify_workspace_invalidate, rebuild_workspace_full, WorkspaceDomainEvent,
};
pub use persist::{flush_persist_writer, init_persist_writer};
pub use projector::{init_workspace_hub, workspace_hub, WorkspaceHub, WorkspaceReadSnapshot};
pub use service::WorkspaceService;
