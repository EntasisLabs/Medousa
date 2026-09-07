//! Provider-neutral authority kernel for daemon-governed interactive worlds.
//!
//! This crate deliberately performs no I/O. Workshop daemons compile policy
//! into grants, admit work synchronously, hand bounded permits to colocated
//! drivers, and persist or publish the resulting events outside this kernel.

mod authority;
mod model;

#[cfg(test)]
mod evaluation_tests;

pub use authority::{WorldAuthority, WorldAuthorityError};
pub use model::*;
