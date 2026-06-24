use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct PackageCatalogEntry {
    pub id: &'static str,
    pub display_name: &'static str,
    pub depends: &'static [&'static str],
    pub default_size_bytes: u64,
    pub optional: bool,
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
            default_size_bytes: 120 * 1024 * 1024,
            optional: false,
        },
        PackageCatalogEntry {
            id: "engine",
            display_name: "Medousa Engine",
            depends: &[],
            default_size_bytes: 45 * 1024 * 1024,
            optional: false,
        },
        PackageCatalogEntry {
            id: "local-brain",
            display_name: "Offline brain (local inference)",
            depends: &["engine"],
            default_size_bytes: 350 * 1024 * 1024,
            optional: true,
        },
        PackageCatalogEntry {
            id: "cli",
            display_name: "Command-line tools",
            depends: &["engine"],
            default_size_bytes: 25 * 1024 * 1024,
            optional: true,
        },
        PackageCatalogEntry {
            id: "adapter-telegram",
            display_name: "Telegram adapter",
            depends: &["engine"],
            default_size_bytes: 12 * 1024 * 1024,
            optional: true,
        },
        PackageCatalogEntry {
            id: "adapter-discord",
            display_name: "Discord adapter",
            depends: &["engine"],
            default_size_bytes: 18 * 1024 * 1024,
            optional: true,
        },
        PackageCatalogEntry {
            id: "adapter-slack",
            display_name: "Slack adapter",
            depends: &["engine"],
            default_size_bytes: 14 * 1024 * 1024,
            optional: true,
        },
        PackageCatalogEntry {
            id: "adapter-whatsapp",
            display_name: "WhatsApp adapter",
            depends: &["engine"],
            default_size_bytes: 10 * 1024 * 1024,
            optional: true,
        },
        PackageCatalogEntry {
            id: "mcp-gateway",
            display_name: "MCP gateway",
            depends: &["engine"],
            default_size_bytes: 8 * 1024 * 1024,
            optional: true,
        },
        PackageCatalogEntry {
            id: "model-gemma-e2b",
            display_name: "Gemma 4 E2B (light)",
            depends: &["local-brain"],
            default_size_bytes: 2 * 1024 * 1024 * 1024,
            optional: true,
        },
        PackageCatalogEntry {
            id: "model-gemma-e4b",
            display_name: "Gemma 4 E4B (balanced)",
            depends: &["local-brain"],
            default_size_bytes: 5 * 1024 * 1024 * 1024,
            optional: true,
        },
        PackageCatalogEntry {
            id: "model-gemma-12b",
            display_name: "Gemma 4 12B (recommended)",
            depends: &["local-brain"],
            default_size_bytes: 10 * 1024 * 1024 * 1024,
            optional: true,
        },
    ]
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
