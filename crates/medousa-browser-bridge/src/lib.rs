//! Unified tab group model for collaborative agent/user browser sessions.

mod manager;
mod model;

pub use manager::TabGroupManager;
pub use model::{
    BrowserControl, BrowserObservation, BrowserObservationBounds, BrowserObservationCapture,
    BrowserObservationState, BrowserObservationViewport, BrowserScreenshotCapture,
    BrowserSemanticNode, BrowserSnapshot, BrowserTab, TabGroup, TabGroupState, TabOpenedBy,
    BROWSER_OBSERVATION_SCHEMA_VERSION, BROWSER_SCREENSHOT_SCHEMA_VERSION,
};
