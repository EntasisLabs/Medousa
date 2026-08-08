//! medousa-forge — Medousa's version-controlled work lifecycle engine.
//!
//! Forge owns the durable lifecycle of a user-owned piece of work: governed
//! git environments, lease-fenced executor attempts, sealed evidence, human
//! review, and recoverable dispositions. Executors (script adapters now, ACP
//! adapters later) are replaceable callers of the lease API — Forge never runs
//! them and never resumes providers itself.
//!
//! Governance is audit, not sandbox. Forge metadata lives outside the user's
//! worktree, always.

pub mod adapter;
pub mod error;
pub mod events;
pub mod forge;
pub mod git;
pub mod model;
pub mod policy;
pub mod reconcile;
pub mod slug;
pub mod store;

pub use error::{ForgeError, Result};
