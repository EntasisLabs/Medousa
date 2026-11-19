//! Desktop host policy shared by Tauri and RNDR, with no renderer dependency.
pub mod connection;
pub mod defaults;
pub mod paths;
pub mod workshop_registry;
pub mod workshop_runtime;

#[cfg(test)]
mod connection_tests;

pub mod mesh_envelope;
pub mod pairing_client;
pub mod peers;
pub mod workshop_transport;
