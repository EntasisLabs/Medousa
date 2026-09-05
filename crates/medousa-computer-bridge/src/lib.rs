//! Platform-neutral contract between a workshop daemon and a computer driver.
//!
//! Drivers report mechanical capabilities and untrusted observations. They do
//! not mint authority: the daemon admits every observation or action through
//! `medousa-world` before dispatch.

mod model;

pub use model::*;
