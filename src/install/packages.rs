use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageCategory {
    Core,
    Adapter,
    Model,
    Expansion,
}

impl PackageCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Adapter => "adapter",
            Self::Model => "model",
            Self::Expansion => "expansion",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PackageCatalogEntry {
    pub id: &'static str,
    pub display_name: &'static str,
    pub depends: &'static [&'static str],
    pub binaries: &'static [&'static str],
    pub category: PackageCategory,
    pub workload_ids: &'static [&'static str],
    pub default_size_bytes: u64,
    pub optional: bool,
    /// Hidden from catalog until published in release-manifest (expansions).
    pub remote_only: bool,
}

#[derive(Debug, Clone)]
pub struct PackageProfile {
    pub id: &'static str,
    pub display_name: &'static str,
    pub packages: &'static [&'static str],
}

pub fn package_catalog() -> Vec<PackageCatalogEntry> {
    vec![
        PackageCatalogEntry {
            id: "desktop",
            display_name: "Medousa Desktop",
            depends: &[],
            binaries: &[],
            category: PackageCategory::Core,
            workload_ids: &["express", "offline-workstation", "developer"],
            default_size_bytes: 120 * 1024 * 1024,
            optional: false,
            remote_only: false,
        },
        PackageCatalogEntry {
            id: "engine",
            display_name: "Medousa Engine",
            depends: &[],
            binaries: &["medousa", "medousa_daemon"],
            category: PackageCategory::Core,
            workload_ids: &["express", "offline-workstation", "developer", "headless-server"],
            default_size_bytes: 45 * 1024 * 1024,
            optional: false,
            remote_only: false,
        },
        PackageCatalogEntry {
            id: "local-brain",
            display_name: "Offline brain (local inference)",
            depends: &["engine"],
            binaries: &["medousa_local"],
            category: PackageCategory::Core,
            workload_ids: &["offline-workstation"],
            default_size_bytes: 350 * 1024 * 1024,
            optional: true,
            remote_only: false,
        },
        PackageCatalogEntry {
            id: "cli",
            display_name: "Command-line tools",
            depends: &["engine"],
            binaries: &["medousa_cli", "medousa_tui"],
            category: PackageCategory::Core,
            workload_ids: &["developer"],
            default_size_bytes: 25 * 1024 * 1024,
            optional: true,
            remote_only: false,
        },
        PackageCatalogEntry {
            id: "adapter-telegram",
            display_name: "Telegram adapter",
            depends: &["engine"],
            binaries: &["medousa_telegram"],
            category: PackageCategory::Adapter,
            workload_ids: &[],
            default_size_bytes: 12 * 1024 * 1024,
            optional: true,
            remote_only: false,
        },
        PackageCatalogEntry {
            id: "adapter-discord",
            display_name: "Discord adapter",
            depends: &["engine"],
            binaries: &["medousa_discord"],
            category: PackageCategory::Adapter,
            workload_ids: &[],
            default_size_bytes: 18 * 1024 * 1024,
            optional: true,
            remote_only: false,
        },
        PackageCatalogEntry {
            id: "adapter-slack",
            display_name: "Slack adapter",
            depends: &["engine"],
            binaries: &["medousa_slack"],
            category: PackageCategory::Adapter,
            workload_ids: &[],
            default_size_bytes: 14 * 1024 * 1024,
            optional: true,
            remote_only: false,
        },
        PackageCatalogEntry {
            id: "adapter-whatsapp",
            display_name: "WhatsApp adapter",
            depends: &["engine"],
            binaries: &["medousa_whatsapp"],
            category: PackageCategory::Adapter,
            workload_ids: &[],
            default_size_bytes: 10 * 1024 * 1024,
            optional: true,
            remote_only: false,
        },
        PackageCatalogEntry {
            id: "mcp-gateway",
            display_name: "MCP gateway",
            depends: &["engine"],
            binaries: &["medousa_mcp_gateway"],
            category: PackageCategory::Core,
            workload_ids: &["developer"],
            default_size_bytes: 8 * 1024 * 1024,
            optional: true,
            remote_only: false,
        },
        PackageCatalogEntry {
            id: "model-gemma-e2b",
            display_name: "Gemma 4 E2B (light)",
            depends: &["local-brain"],
            binaries: &[],
            category: PackageCategory::Model,
            workload_ids: &["offline-workstation"],
            default_size_bytes: 2 * 1024 * 1024 * 1024,
            optional: true,
            remote_only: false,
        },
        PackageCatalogEntry {
            id: "model-gemma-e4b",
            display_name: "Gemma 4 E4B (balanced)",
            depends: &["local-brain"],
            binaries: &[],
            category: PackageCategory::Model,
            workload_ids: &["offline-workstation"],
            default_size_bytes: 5 * 1024 * 1024 * 1024,
            optional: true,
            remote_only: false,
        },
        PackageCatalogEntry {
            id: "model-gemma-12b",
            display_name: "Gemma 4 12B (recommended)",
            depends: &["local-brain"],
            binaries: &[],
            category: PackageCategory::Model,
            workload_ids: &["offline-workstation"],
            default_size_bytes: 10 * 1024 * 1024 * 1024,
            optional: true,
            remote_only: false,
        },
        // Expansion stubs — shown only when release-manifest publishes them.
        PackageCatalogEntry {
            id: "skill-hub",
            display_name: "Skill Hub",
            depends: &["engine"],
            binaries: &[],
            category: PackageCategory::Expansion,
            workload_ids: &[],
            default_size_bytes: 50 * 1024 * 1024,
            optional: true,
            remote_only: true,
        },
        PackageCatalogEntry {
            id: "grapheme-module-starter",
            display_name: "Grapheme module starter pack",
            depends: &["engine"],
            binaries: &[],
            category: PackageCategory::Expansion,
            workload_ids: &[],
            default_size_bytes: 20 * 1024 * 1024,
            optional: true,
            remote_only: true,
        },
    ]
}

pub fn visible_catalog(remote_package_ids: &[String]) -> Vec<PackageCatalogEntry> {
    let remote: HashSet<_> = remote_package_ids.iter().cloned().collect();
    package_catalog()
        .into_iter()
        .filter(|entry| !entry.remote_only || remote.contains(entry.id))
        .collect()
}

pub fn catalog_entry(package_id: &str) -> Option<PackageCatalogEntry> {
    package_catalog()
        .into_iter()
        .find(|entry| entry.id == package_id)
}

pub fn default_install_profiles() -> Vec<PackageProfile> {
    vec![
        PackageProfile {
            id: "express",
            display_name: "Express",
            packages: &["desktop", "engine"],
        },
        PackageProfile {
            id: "offline-workstation",
            display_name: "Offline workstation",
            packages: &["desktop", "engine", "local-brain", "model-gemma-e4b"],
        },
        PackageProfile {
            id: "developer",
            display_name: "Developer",
            packages: &["desktop", "engine", "cli", "mcp-gateway"],
        },
        PackageProfile {
            id: "headless-server",
            display_name: "Headless server",
            packages: &["engine"],
        },
    ]
}

pub fn resolve_profile_packages(profile_id: &str) -> Option<Vec<&'static str>> {
    default_install_profiles()
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .map(|profile| profile.packages.to_vec())
}

pub fn expand_package_dependencies(selected: &[&str]) -> Vec<String> {
    let catalog: HashMap<_, _> = package_catalog()
        .into_iter()
        .map(|entry| (entry.id, entry))
        .collect();
    let mut resolved = Vec::new();
    let mut seen = HashSet::new();
    let mut stack: Vec<&str> = selected.iter().copied().collect();

    while let Some(id) = stack.pop() {
        if !seen.insert(id.to_string()) {
            continue;
        }
        if let Some(entry) = catalog.get(id) {
            for dep in entry.depends {
                if !seen.contains(*dep) {
                    stack.push(dep);
                }
            }
        }
        resolved.push(id.to_string());
    }

    resolved.reverse();
    resolved
}

pub fn package_disk_estimate_bytes(package_ids: &[String]) -> u64 {
    let catalog: HashMap<_, _> = package_catalog()
        .into_iter()
        .map(|entry| (entry.id, entry.default_size_bytes))
        .collect();
    package_ids
        .iter()
        .filter_map(|id| catalog.get(id.as_str()).copied())
        .sum()
}

pub fn is_model_pack(package_id: &str) -> bool {
    package_id.starts_with("model-")
}

pub fn is_desktop_package(package_id: &str) -> bool {
    package_id == "desktop"
}

pub fn is_tarball_package(package_id: &str) -> bool {
    !is_model_pack(package_id) && package_id != "desktop" && package_id != "installer"
}

pub fn install_order_key(package_id: &str) -> u8 {
    match package_id {
        "desktop" => 0,
        "engine" => 1,
        id if id.starts_with("adapter-") || id == "cli" || id == "mcp-gateway" => 2,
        "local-brain" => 3,
        id if is_model_pack(id) => 4,
        _ => 5,
    }
}

pub fn sort_for_install(package_ids: &mut [String]) {
    package_ids.sort_by_key(|id| install_order_key(id));
}
