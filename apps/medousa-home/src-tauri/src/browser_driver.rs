//! Identity and advertised mechanics for this Home process's browser surface.
//!
//! The descriptor is an availability claim only. The workshop daemon still
//! grants capabilities and admits every governed action.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use medousa_world::{
    WorldDriverCapability, WorldDriverId, WorldDriverKind, WorldDriverRegistration,
    WorldDriverTransport, WorldOwnership, WorldSurfaceKind,
};

static DRIVER_ID: LazyLock<WorldDriverId> = LazyLock::new(|| {
    let platform = if cfg!(target_os = "ios") {
        "ios"
    } else if cfg!(target_os = "android") {
        "android"
    } else {
        "desktop"
    };
    WorldDriverId::new(format!(
        "driver:home-{platform}:{}",
        uuid::Uuid::new_v4().simple()
    ))
});

pub fn id() -> &'static WorldDriverId {
    &DRIVER_ID
}

pub fn client_id(channel_surface: &str) -> String {
    format!("home-{}-{}", channel_surface.trim(), id())
}

pub fn registration() -> WorldDriverRegistration {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let (kind, transport, capabilities) = (
        WorldDriverKind::MobileBrowser,
        WorldDriverTransport::ClientQueue,
        [
            WorldDriverCapability::SemanticObservation,
            WorldDriverCapability::Navigation,
            WorldDriverCapability::Interaction,
            WorldDriverCapability::HumanTakeover,
        ]
        .into_iter()
        .collect::<BTreeSet<_>>(),
    );
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let (kind, transport, capabilities) = (
        WorldDriverKind::EmbeddedBrowser,
        WorldDriverTransport::LoopbackHttp,
        [
            WorldDriverCapability::SemanticObservation,
            WorldDriverCapability::PixelObservation,
            WorldDriverCapability::Navigation,
            WorldDriverCapability::Interaction,
            WorldDriverCapability::GuardedBatch,
            WorldDriverCapability::HumanTakeover,
        ]
        .into_iter()
        .collect::<BTreeSet<_>>(),
    );

    WorldDriverRegistration {
        driver_id: id().clone(),
        kind,
        surface: WorldSurfaceKind::Browser,
        ownership: WorldOwnership::Managed,
        transport,
        capabilities,
        display_name: Some("Medousa browser".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_driver_identity_is_stable_for_the_process() {
        assert_eq!(id(), id());
        assert!(!id().is_empty());
        assert_eq!(registration().driver_id, id().clone());
    }
}
