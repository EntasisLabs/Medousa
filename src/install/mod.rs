//! Install and release package manifest types shared by CLI, installer, and Home.

pub mod manifest;
pub mod packages;
pub mod release_config;

pub use manifest::{
    mark_package_installed, package_installed, read_install_manifest, read_release_manifest,
    shared_bin_dir, unmark_package_installed, user_packages_dir, write_install_manifest,
    InstallManifest, PackageInstallRecord, ReleaseManifest, ReleasePackage,
};
pub use packages::{
    catalog_entry, default_install_profiles, expand_package_dependencies, is_desktop_package,
    is_model_pack, is_tarball_package, package_catalog, package_disk_estimate_bytes,
    sort_for_install, visible_catalog, PackageCatalogEntry, PackageCategory, PackageProfile,
};
pub use release_config::{
    host_platform_key, host_target, installer_bootstrap_url, release_base_url, release_channel,
    release_manifest_url, InstallerBootstrap, InstallerBootstrapPlatform,
};
