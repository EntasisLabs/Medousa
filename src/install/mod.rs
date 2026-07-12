//! Install and release package manifest types shared by CLI, installer, and Home.

pub use medousa_install_support::manifest;
pub use medousa_install_support::packages;
pub use medousa_install_support::release_config;

pub use manifest::{
    mark_package_installed, package_installed, read_install_manifest, read_release_manifest,
    shared_bin_dir, unmark_package_installed, user_packages_dir, write_install_manifest,
    InstallManifest, PackageInstallRecord, ReleaseManifest, ReleasePackage,
};
pub use packages::{
    catalog_entry, default_install_profiles, expand_home_package_dependencies,
    expand_package_dependencies, home_packages_catalog, is_desktop_package,
    is_home_packages_package, is_model_pack, is_tarball_package, package_catalog,
    package_disk_estimate_bytes, package_short_hint, phase_label, sort_for_install,
    visible_catalog, PackageCatalogEntry, PackageCategory, PackageProfile,
};
pub use release_config::{
    host_platform_key, host_target, installer_bootstrap_url, release_base_url, release_channel,
    release_manifest_url, set_embedded_release_defaults, InstallerBootstrap,
    InstallerBootstrapPlatform,
};
pub use medousa_install_support::tarball_install::{
    fetch_release_manifest, install_tarball_package, remove_tarball_package,
    resolve_release_package,
};
