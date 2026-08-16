//! Medousa vault — portable markdown corpus (Phase V0).

pub mod admission;
pub mod baseline;
pub mod contracts;
pub mod fixtures;
pub mod h07_verify;
pub mod job_footer;
pub mod lanes;
pub mod links;
pub mod mutation;
pub mod note;
pub mod owner;
pub mod path;
pub mod projection;
pub mod relocate;
pub mod roots;
pub mod search;
pub mod search_index;
pub mod semantic_tags;
pub mod service;
pub mod store;

pub use service::VaultService;
pub use store::vault_store;
