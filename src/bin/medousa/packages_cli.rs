//! `medousa pull` / `update` / `packages` — CDN package install via medousa-install-support.

use anyhow::{Context, Result, anyhow};
use medousa_install_support::{
    fetch_release_manifest, install_tarball_package, is_tarball_package, package_installed,
    resolve_package_alias, resolve_release_package, PackageCatalogEntry,
};
use serde::Serialize;
use std::path::Path;

pub fn run_pull(args: &[String]) -> Result<()> {
    let json = has_flag(args, "--json");
    let name = positional_arg(args).ok_or_else(|| {
        anyhow!("usage: medousa pull <name> [--json]\nNames: engine, mcp-gateway, telegram, discord, slack, whatsapp, local-brain, …")
    })?;
    let package_id = resolve_package_alias(&name)
        .ok_or_else(|| anyhow!("unknown package '{}'", name))?;
    if !is_tarball_package(package_id) {
        bail_or_json(
            json,
            &format!("package '{package_id}' is not installable via pull (use the Installer for desktop)"),
        )?;
        return Ok(());
    }

    let data_dir = medousa::paths::medousa_data_dir();
    let rt = tokio::runtime::Runtime::new().context("start tokio runtime")?;
    let package = rt.block_on(async {
        install_tarball_package(&data_dir, package_id, None, |pct, phase| {
            if !json {
                eprintln!("[{pct:5.1}%] {phase}");
            }
        })
        .await
    })
    .map_err(|err| anyhow!("{err}"))?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "action": "pull",
                "packageId": package.id,
                "version": package.version,
                "target": package.target,
                "binDir": data_dir.join("bin").display().to_string(),
            }))?
        );
    } else {
        println!(
            "Installed {} v{} → {}",
            package.id,
            package.version,
            data_dir.join("bin").display()
        );
    }
    Ok(())
}

pub fn run_update(args: &[String]) -> Result<()> {
    let json = has_flag(args, "--json");
    let name = positional_arg(args);
    let data_dir = medousa::paths::medousa_data_dir();
    let rt = tokio::runtime::Runtime::new().context("start tokio runtime")?;

    let results = rt.block_on(async {
        let manifest = fetch_release_manifest().await.map_err(|e| anyhow!("{e}"))?;
        let targets: Vec<String> = if let Some(name) = name {
            let id = resolve_package_alias(&name)
                .ok_or_else(|| anyhow!("unknown package '{}'", name))?;
            vec![id.to_string()]
        } else {
            installed_tarball_packages(&data_dir)
        };

        if targets.is_empty() {
            return Ok::<Vec<UpdateResult>, anyhow::Error>(vec![]);
        }

        let mut out = Vec::new();
        for package_id in targets {
            if !is_tarball_package(&package_id) {
                out.push(UpdateResult {
                    package_id: package_id.clone(),
                    action: "skipped".into(),
                    local_version: None,
                    remote_version: None,
                    message: Some("not a tarball package".into()),
                });
                continue;
            }
            let remote = match resolve_release_package(&manifest, &package_id) {
                Ok(pkg) => pkg.clone(),
                Err(err) => {
                    out.push(UpdateResult {
                        package_id,
                        action: "error".into(),
                        local_version: None,
                        remote_version: None,
                        message: Some(err),
                    });
                    continue;
                }
            };
            let local_version = local_package_version(&data_dir, &package_id);
            let needs = match &local_version {
                Some(local) => version_newer(&remote.version, local),
                None => true,
            };
            if !needs {
                out.push(UpdateResult {
                    package_id,
                    action: "current".into(),
                    local_version,
                    remote_version: Some(remote.version),
                    message: None,
                });
                continue;
            }
            match install_tarball_package(&data_dir, &package_id, None, |pct, phase| {
                if !json {
                    eprintln!("[{package_id} {pct:5.1}%] {phase}");
                }
            })
            .await
            {
                Ok(pkg) => out.push(UpdateResult {
                    package_id,
                    action: "updated".into(),
                    local_version,
                    remote_version: Some(pkg.version),
                    message: None,
                }),
                Err(err) => out.push(UpdateResult {
                    package_id,
                    action: "error".into(),
                    local_version,
                    remote_version: Some(remote.version),
                    message: Some(err),
                }),
            }
        }
        Ok(out)
    })?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": results.iter().all(|r| r.action != "error"),
                "action": "update",
                "results": results,
            }))?
        );
    } else if results.is_empty() {
        println!("No installed packages to update. Use `medousa pull <name>` first.");
    } else {
        for r in &results {
            match r.action.as_str() {
                "updated" => println!(
                    "Updated {} → v{}",
                    r.package_id,
                    r.remote_version.as_deref().unwrap_or("?")
                ),
                "current" => println!(
                    "{} is current (v{})",
                    r.package_id,
                    r.remote_version.as_deref().unwrap_or("?")
                ),
                "error" => eprintln!(
                    "Failed {}: {}",
                    r.package_id,
                    r.message.as_deref().unwrap_or("unknown error")
                ),
                other => println!("{}: {other}", r.package_id),
            }
        }
        if results.iter().any(|r| r.action == "error") {
            return Err(anyhow!("one or more package updates failed"));
        }
    }
    Ok(())
}

pub fn run_packages(args: &[String]) -> Result<()> {
    let json = has_flag(args, "--json");
    let sub = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map(String::as_str)
        .unwrap_or("list");

    match sub {
        "list" | "status" => {
            let data_dir = medousa::paths::medousa_data_dir();
            let rt = tokio::runtime::Runtime::new().context("start tokio runtime")?;
            let rows = rt.block_on(async {
                let remote = fetch_release_manifest().await.ok();
                let catalog = medousa_install_support::package_catalog();
                let mut rows = Vec::new();
                for entry in catalog {
                    if !is_tarball_package(entry.id) {
                        continue;
                    }
                    let installed = package_installed(&data_dir, entry.id)
                        || binaries_present(&data_dir, &entry);
                    let local_version = local_package_version(&data_dir, entry.id);
                    let remote_version = remote.as_ref().and_then(|m| {
                        resolve_release_package(m, entry.id)
                            .ok()
                            .map(|p| p.version.clone())
                    });
                    let update_available = match (&local_version, &remote_version) {
                        (Some(local), Some(remote_v)) => version_newer(remote_v, local),
                        (None, Some(_)) if installed => true,
                        _ => false,
                    };
                    rows.push(PackageStatusRow {
                        id: entry.id.to_string(),
                        display_name: entry.display_name.to_string(),
                        installed,
                        local_version,
                        remote_version,
                        update_available,
                    });
                }
                rows
            });

            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "ok": true,
                        "packages": rows,
                    }))?
                );
            } else {
                println!(
                    "{:<22} {:<10} {:<12} {:<12} {}",
                    "PACKAGE", "STATUS", "LOCAL", "REMOTE", "UPDATE"
                );
                for row in rows {
                    println!(
                        "{:<22} {:<10} {:<12} {:<12} {}",
                        row.id,
                        if row.installed { "installed" } else { "—" },
                        row.local_version.as_deref().unwrap_or("—"),
                        row.remote_version.as_deref().unwrap_or("—"),
                        if row.update_available { "yes" } else { "—" }
                    );
                }
            }
            Ok(())
        }
        "help" | "--help" | "-h" => {
            println!("medousa packages list|status [--json]");
            println!("medousa pull <name> [--json]");
            println!("medousa update [<name>] [--json]");
            Ok(())
        }
        other => Err(anyhow!(
            "unknown packages subcommand '{}'. try list|status",
            other
        )),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateResult {
    package_id: String,
    action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remote_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackageStatusRow {
    id: String,
    display_name: String,
    installed: bool,
    local_version: Option<String>,
    remote_version: Option<String>,
    update_available: bool,
}

fn installed_tarball_packages(data_dir: &Path) -> Vec<String> {
    let mut ids = Vec::new();
    for entry in medousa_install_support::package_catalog() {
        if !is_tarball_package(entry.id) {
            continue;
        }
        if package_installed(data_dir, entry.id) || binaries_present(data_dir, &entry) {
            ids.push(entry.id.to_string());
        }
    }
    ids
}

fn binaries_present(data_dir: &Path, entry: &PackageCatalogEntry) -> bool {
    let bin_dir = data_dir.join("bin");
    entry.binaries.iter().any(|bin| {
        let name = if cfg!(windows) {
            format!("{bin}.exe")
        } else {
            bin.to_string()
        };
        bin_dir.join(name).is_file()
    })
}

fn local_package_version(data_dir: &Path, package_id: &str) -> Option<String> {
    let marker = data_dir
        .join("packages")
        .join(package_id)
        .join(".installed");
    if marker.is_file() {
        // Prefer version from nested install-manifest if present.
        let packages_dir = data_dir.join("packages").join(package_id);
        if let Some(version) = read_nested_package_version(&packages_dir) {
            return Some(version);
        }
    }
    None
}

fn read_nested_package_version(packages_dir: &Path) -> Option<String> {
    let candidates = [
        packages_dir.join("install-manifest.json"),
        packages_dir.join("bin").join("..").join("install-manifest.json"),
    ];
    for path in candidates {
        if let Ok(raw) = std::fs::read_to_string(&path)
            && let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw)
            && let Some(v) = value.get("version").and_then(|v| v.as_str())
        {
            return Some(v.to_string());
        }
    }
    // Walk one level for extracted archive root.
    if let Ok(entries) = std::fs::read_dir(packages_dir) {
        for entry in entries.flatten() {
            let manifest = entry.path().join("install-manifest.json");
            if let Ok(raw) = std::fs::read_to_string(&manifest)
                && let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw)
                && let Some(v) = value.get("version").and_then(|v| v.as_str())
            {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// True when `remote` is strictly newer than `local` (simple dotted numeric compare).
fn version_newer(remote: &str, local: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.trim()
            .trim_start_matches('v')
            .split('.')
            .map(|part| {
                part.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0)
            })
            .collect()
    };
    let a = parse(remote);
    let b = parse(local);
    let len = a.len().max(b.len());
    for i in 0..len {
        let ai = a.get(i).copied().unwrap_or(0);
        let bi = b.get(i).copied().unwrap_or(0);
        if ai != bi {
            return ai > bi;
        }
    }
    false
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn positional_arg(args: &[String]) -> Option<String> {
    args.iter()
        .find(|a| !a.starts_with('-'))
        .cloned()
}

fn bail_or_json(json: bool, message: &str) -> Result<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": false,
                "error": message,
            }))?
        );
        Ok(())
    } else {
        Err(anyhow!("{message}"))
    }
}
