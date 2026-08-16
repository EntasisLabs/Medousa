//! Dependency-light store primitives shared by the daemon and `medousa-forge`.
//!
//! H06.0.5 placement decision: extract H04 receipts, fault points, file
//! transactions, and the `StoreRoot` capability so Forge can reuse them
//! without depending upward on the `medousa` crate.

pub mod persistence;
pub mod store_root;

pub use persistence::{
    CommitReceipt, DurabilityLevel, FileTransaction, NoTransactionFaults, PersistenceError,
    PersistenceErrorKind, StoreKind, TransactionFaultPoint, TransactionFaults,
};
pub use store_root::{
    ConfinementReason, StoreDirectoryEntry, StoreEntry, StoreEntryKind, StoreMetadata, StorePath,
    StoreRoot, StoreRootError, StoreRootPath,
};
