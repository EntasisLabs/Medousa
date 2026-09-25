//! Trust role shared by full-daemon pairing and embedded execution policy.

use serde::{Deserialize, Serialize};

/// How a paired surface relates to the workshop.
/// - `portal`: full client of this brain (phone / workshop switcher)
/// - `peer`: inbox + share only
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum PairingRole {
    #[default]
    Portal,
    Peer,
}

impl PairingRole {
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("peer") => Self::Peer,
            _ => Self::Portal,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Portal => "portal",
            Self::Peer => "peer",
        }
    }

    pub fn allows_peer_surface(self) -> bool {
        matches!(self, Self::Peer | Self::Portal)
    }

    pub fn allows_full_portal(self) -> bool {
        matches!(self, Self::Portal)
    }
}
