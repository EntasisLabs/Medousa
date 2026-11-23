//! Desktop host policy shared by Tauri and RNDR, with no renderer dependency.
pub mod connection;
pub mod defaults;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod packages;
pub mod paths;
pub mod workshop_registry;
pub mod workshop_runtime;

#[cfg(test)]
mod connection_tests;

#[cfg(feature = "legacy-client-network")]
pub mod mesh_envelope;
#[cfg(feature = "legacy-client-network")]
pub mod pairing_client;
pub mod peer_lan;
pub mod peer_management;
pub mod peers;
#[cfg(feature = "legacy-client-network")]
pub mod workshop_transport;
