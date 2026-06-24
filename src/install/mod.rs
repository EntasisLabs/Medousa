//! Install and release package manifest types shared by CLI, installer, and Home.

pub mod manifest;
pub mod packages;

pub use manifest::{
    read_install_manifest, read_release_manifest, write_install_manifest, InstallManifest,
    PackageInstallRecord, ReleaseManifest, ReleasePackage,
};
pub use packages::{
    default_install_profiles, package_catalog, package_disk_estimate_bytes, PackageCatalogEntry,
    PackageProfile,
};
