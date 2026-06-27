//! Unified tab group model for collaborative agent/user browser sessions.

mod manager;
mod model;

pub use manager::TabGroupManager;
pub use model::{
    BrowserControl, BrowserSnapshot, BrowserTab, TabGroup, TabGroupState, TabOpenedBy,
};
