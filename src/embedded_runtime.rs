//! Portable runtime adapters shared by the embedded deployment.

#[path = "runtime/memory_bundle.rs"]
pub mod memory_bundle;

pub mod persistent_locus {
    pub use crate::persistent_locus::*;
}

pub mod surreal_startup {
    pub use crate::surreal_startup::*;
}

