//! Medousa vault — portable markdown corpus (Phase V0).

pub mod note;
pub mod path;
pub mod search;
pub mod service;
pub mod store;

pub use service::VaultService;
pub use store::vault_store;
