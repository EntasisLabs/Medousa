//! Slim install/bootstrap support for Medousa Installer — no engine, DB, or channel deps.

pub mod manifest;
pub mod model_catalog;
pub mod model_download;
pub mod packages;
pub mod paths;
pub mod release_config;
pub mod tarball_install;

pub use manifest::{
    InstallManifest, PackageInstallRecord, ReleaseManifest, ReleasePackage,
    desktop_artifact_url_matches_host, installed_package_dir, mark_package_installed, package_dir,
    package_installed, read_install_manifest, read_release_manifest, release_package_matches_host,
    resolve_release_package, shared_bin_dir, unmark_package_installed, user_packages_dir,
    write_install_manifest,
};
pub use model_catalog::{CatalogFile, CatalogModelEntry, builtin_catalog};
pub use model_download::{
    DownloadPhase, InstalledModelRecord, MODEL_STORE, ModelDownloadProgress, ModelStore,
    include_hf_file, local_repo_if_installed,
};
pub use packages::{
    PackageCatalogEntry, PackageCategory, PackageComposition, PackageProfile, catalog_entry,
    default_install_profiles, expand_home_package_dependencies, expand_package_dependencies,
    home_packages_catalog, is_desktop_package, is_home_packages_package, is_model_pack,
    is_tarball_package, package_catalog, package_composition, package_disk_estimate_bytes,
    package_short_hint, phase_label, resolve_package_alias, sort_for_install, visible_catalog,
};
pub use release_config::{
    InstallerBootstrap, InstallerBootstrapPlatform, host_platform_key, host_target,
    installer_bootstrap_url, release_base_url, release_channel, release_manifest_url,
    set_embedded_release_defaults,
};
pub use tarball_install::{
    download_url, fetch_release_manifest, install_tarball_package, remove_tarball_package,
    verify_sha256,
};
