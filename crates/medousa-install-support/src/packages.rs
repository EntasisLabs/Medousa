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

    pub fn label(self) -> &'static str {
        match self {
            Self::Core => "Core",
            Self::Adapter => "Channels",
            Self::Model => "Offline models",
            Self::Expansion => "Expansions",
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
    pub category_label: &'static str,
    pub icon: &'static str,
    pub workload_ids: &'static [&'static str],
    pub default_size_bytes: u64,
    pub optional: bool,
    pub remote_only: bool,
}

#[derive(Debug, Clone)]
pub struct PackageProfile {
    pub id: &'static str,
    pub display_name: &'static str,
    pub short_description: &'static str,
    pub icon: &'static str,
    pub section: &'static str,
    pub packages: &'static [&'static str],
}

#[allow(clippy::too_many_arguments)]
fn entry(
    id: &'static str,
    display_name: &'static str,
    depends: &'static [&'static str],
    binaries: &'static [&'static str],
    category: PackageCategory,
    icon: &'static str,
    workload_ids: &'static [&'static str],
    default_size_bytes: u64,
    optional: bool,
    remote_only: bool,
) -> PackageCatalogEntry {
    PackageCatalogEntry {
        id,
        display_name,
        depends,
        binaries,
        category,
        category_label: category.label(),
        icon,
        workload_ids,
        default_size_bytes,
        optional,
        remote_only,
    }
}

pub fn package_catalog() -> Vec<PackageCatalogEntry> {
    vec![
        entry(
            "desktop",
            "Medousa Desktop",
            &[],
            &[],
            PackageCategory::Core,
            "Monitor",
            &["express", "offline-workstation", "developer"],
            120 * 1024 * 1024,
            false,
            false,
        ),
        entry(
            "engine",
            "Medousa Engine",
            &[],
            &["medousa", "medousa_daemon", "medousa_cli", "medousa_tui"],
            PackageCategory::Core,
            "Zap",
            &["express", "offline-workstation", "developer", "headless-server"],
            70 * 1024 * 1024,
            false,
            false,
        ),
        entry(
            "local-brain",
            "Offline brain",
            &["engine"],
            &["medousa_local"],
            PackageCategory::Core,
            "Brain",
            &["offline-workstation"],
            350 * 1024 * 1024,
            true,
            false,
        ),
        entry(
            "adapter-telegram",
            "Telegram",
            &["engine"],
            &["medousa_telegram"],
            PackageCategory::Adapter,
            "MessageCircle",
            &[],
            12 * 1024 * 1024,
            true,
            false,
        ),
        entry(
            "adapter-discord",
            "Discord",
            &["engine"],
            &["medousa_discord"],
            PackageCategory::Adapter,
            "MessagesSquare",
            &[],
            18 * 1024 * 1024,
            true,
            false,
        ),
        entry(
            "adapter-slack",
            "Slack",
            &["engine"],
            &["medousa_slack"],
            PackageCategory::Adapter,
            "MessagesSquare",
            &[],
            14 * 1024 * 1024,
            true,
            false,
        ),
        entry(
            "adapter-whatsapp",
            "WhatsApp",
            &["engine"],
            &["medousa_whatsapp"],
            PackageCategory::Adapter,
            "Phone",
            &[],
            10 * 1024 * 1024,
            true,
            false,
        ),
        entry(
            "mcp-gateway",
            "MCP gateway",
            &["engine"],
            &["medousa_mcp_gateway"],
            PackageCategory::Core,
            "Plug",
            &["developer"],
            8 * 1024 * 1024,
            true,
            false,
        ),
        entry(
            "model-gemma-e2b",
            "Gemma 4 E2B (light)",
            &["local-brain"],
            &[],
            PackageCategory::Model,
            "Sparkles",
            &["offline-workstation"],
            2 * 1024 * 1024 * 1024,
            true,
            false,
        ),
        entry(
            "model-gemma-e4b",
            "Gemma 4 E4B (balanced)",
            &["local-brain"],
            &[],
            PackageCategory::Model,
            "Sparkles",
            &["offline-workstation"],
            5 * 1024 * 1024 * 1024,
            true,
            false,
        ),
        entry(
            "model-gemma-12b",
            "Gemma 4 12B (recommended)",
            &["local-brain"],
            &[],
            PackageCategory::Model,
            "Sparkles",
            &["offline-workstation"],
            10 * 1024 * 1024 * 1024,
            true,
            false,
        ),
        entry(
            "skill-hub",
            "Skill Hub",
            &["engine"],
            &[],
            PackageCategory::Expansion,
            "Sparkles",
            &[],
            50 * 1024 * 1024,
            true,
            true,
        ),
        entry(
            "grapheme-module-starter",
            "Grapheme starter pack",
            &["engine"],
            &[],
            PackageCategory::Expansion,
            "Sparkles",
            &[],
            20 * 1024 * 1024,
            true,
            true,
        ),
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
            short_description: "Desktop and engine — the fastest way to get started.",
            icon: "Zap",
            section: "Desktop & Core",
            packages: &["desktop", "engine"],
        },
        PackageProfile {
            id: "offline-workstation",
            display_name: "Offline workstation",
            short_description: "On-device AI with Gemma — works without the cloud.",
            icon: "Brain",
            section: "Offline AI",
            packages: &["desktop", "engine", "local-brain", "model-gemma-e4b"],
        },
        PackageProfile {
            id: "developer",
            display_name: "Developer",
            short_description: "Engine (includes CLI/TUI) and MCP gateway for power users.",
            icon: "Terminal",
            section: "Desktop & Core",
            packages: &["desktop", "engine", "mcp-gateway"],
        },
        PackageProfile {
            id: "headless-server",
            display_name: "Headless server",
            short_description: "Engine only — no desktop UI.",
            icon: "Server",
            section: "Desktop & Core",
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
    let mut stack: Vec<&str> = selected.to_vec();

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

/// Optional binaries Home can install from Settings → Packages (not desktop/engine/models).
pub fn is_home_packages_package(package_id: &str) -> bool {
    matches!(
        package_id,
        "local-brain"
            | "mcp-gateway"
            | "adapter-telegram"
            | "adapter-discord"
            | "adapter-slack"
            | "adapter-whatsapp"
    )
}

pub fn home_packages_catalog() -> Vec<PackageCatalogEntry> {
    package_catalog()
        .into_iter()
        .filter(|entry| is_home_packages_package(entry.id))
        .collect()
}

/// Expand deps for Home Packages — drops desktop/engine/model packs (engine ships with Home).
pub fn expand_home_package_dependencies(selected: &[&str]) -> Vec<String> {
    expand_package_dependencies(selected)
        .into_iter()
        .filter(|id| is_home_packages_package(id))
        .collect()
}

pub fn package_short_hint(package_id: &str) -> &'static str {
    match package_id {
        "engine" => "Launcher, daemon, CLI, and TUI.",
        "local-brain" => "On-device inference binary for offline Gemma.",
        "mcp-gateway" => "Connect MCP servers to Medousa.",
        "adapter-telegram" => "Telegram channel adapter.",
        "adapter-discord" => "Discord channel adapter.",
        "adapter-slack" => "Slack channel adapter.",
        "adapter-whatsapp" => "WhatsApp channel adapter.",
        _ => "Optional Medousa component.",
    }
}

/// Resolve user-facing package names/aliases to catalog package ids.
pub fn resolve_package_alias(name: &str) -> Option<&'static str> {
    match name.trim().to_ascii_lowercase().as_str() {
        "engine" | "daemon" | "cli" | "tui" | "core" => Some("engine"),
        "mcp" | "mcp_gateway" | "mcp-gateway" => Some("mcp-gateway"),
        "telegram" | "adapter-telegram" => Some("adapter-telegram"),
        "discord" | "adapter-discord" => Some("adapter-discord"),
        "slack" | "adapter-slack" => Some("adapter-slack"),
        "whatsapp" | "adapter-whatsapp" => Some("adapter-whatsapp"),
        "local-brain" | "brain" | "local_brain" => Some("local-brain"),
        "desktop" => Some("desktop"),
        other => catalog_entry(other).map(|entry| entry.id),
    }
}

pub fn install_order_key(package_id: &str) -> u8 {
    match package_id {
        "desktop" => 0,
        "engine" => 1,
        id if id.starts_with("adapter-") || id == "mcp-gateway" => 2,
        "local-brain" => 3,
        id if is_model_pack(id) => 4,
        _ => 5,
    }
}

pub fn sort_for_install(package_ids: &mut [String]) {
    package_ids.sort_by_key(|id| install_order_key(id));
}

pub fn phase_label(phase: &str) -> &'static str {
    match phase {
        "downloading" => "Downloading",
        "verifying" => "Verifying",
        "extracting" | "installing" => "Installing",
        "removing" => "Removing",
        "ready" => "Complete",
        "failed" => "Failed",
        "removed" => "Removed",
        _ => "Working",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_package_includes_cli_and_tui() {
        let engine = catalog_entry("engine").expect("engine catalog entry");
        assert!(engine.binaries.contains(&"medousa_cli"));
        assert!(engine.binaries.contains(&"medousa_tui"));
        assert!(catalog_entry("cli").is_none());
    }

    #[test]
    fn package_aliases_resolve() {
        assert_eq!(resolve_package_alias("daemon"), Some("engine"));
        assert_eq!(resolve_package_alias("cli"), Some("engine"));
        assert_eq!(resolve_package_alias("mcp"), Some("mcp-gateway"));
        assert_eq!(resolve_package_alias("telegram"), Some("adapter-telegram"));
        assert_eq!(resolve_package_alias("brain"), Some("local-brain"));
    }

    #[test]
    fn developer_profile_no_longer_lists_cli() {
        let packages = resolve_profile_packages("developer").expect("developer profile");
        assert!(packages.contains(&"engine"));
        assert!(packages.contains(&"mcp-gateway"));
        assert!(!packages.contains(&"cli"));
    }

    #[test]
    fn home_packages_excludes_cli() {
        assert!(!is_home_packages_package("cli"));
        assert!(is_home_packages_package("mcp-gateway"));
    }
}
