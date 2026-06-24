//! Medousa vault — portable markdown corpus (Phase V0).

pub mod job_footer;
pub mod links;
pub mod note;
pub mod path;
pub mod search;
pub mod semantic_tags;
pub mod service;
pub mod store;

pub use service::VaultService;
pub use store::vault_store;
