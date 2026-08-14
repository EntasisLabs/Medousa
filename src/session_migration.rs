//! H02 legacy session-storage inventory and restartable migration.
//!
//! Inventory is no-follow and bounded. Applying a plan copies ordinary legacy
//! data to its opaque storage key, verifies the copy, and retains the source as
//! rollback material. Malformed, ambiguous, link-backed, and colliding entries
//! are reported but never followed or removed.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::session_storage::{SessionId, StorageKey};
use crate::store_root::{StoreEntryKind, StorePath, StoreRoot};

const MIGRATION_RECORD: &str = "h02-v1.json";
const MAX_ENTRIES: usize = 100_000;
const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationEntryStatus {
    Current,
    Planned,
    Migrated,
    Quarantined,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMigrationEntry {
    pub surface: String,
    pub name_digest: String,
    pub entry_kind: String,
    pub size: u64,
    pub status: MigrationEntryStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collision_group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_class: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMigrationReport {
    pub layout_version: u8,
    pub dry_run: bool,
    pub generated_at_utc: chrono::DateTime<Utc>,
    pub entries_scanned: usize,
    pub bytes_scanned: u64,
    pub current: usize,
    pub planned: usize,
    pub migrated: usize,
    pub quarantined: usize,
    pub entries: Vec<SessionMigrationEntry>,
}

#[derive(Clone, Copy)]
enum SurfaceShape {
    File(&'static str),
    Directory,
}

#[derive(Clone, Copy)]
struct SurfaceSpec {
    name: &'static str,
    directory: &'static str,
    shape: SurfaceShape,
    ignored_root_entry: Option<&'static str>,
}

const SURFACES: &[SurfaceSpec] = &[
    SurfaceSpec {
        name: "transcript",
        directory: "history",
        shape: SurfaceShape::File("jsonl"),
        ignored_root_entry: None,
    },
    SurfaceSpec {
        name: "catalog",
        directory: "catalog",
        shape: SurfaceShape::File("json"),
        ignored_root_entry: None,
    },
    SurfaceSpec {
        name: "shared_catalog",
        directory: "shared_catalog",
        shape: SurfaceShape::File("json"),
        ignored_root_entry: None,
    },
    SurfaceSpec {
        name: "tool_surface",
        directory: "session_surfaces",
        shape: SurfaceShape::File("json"),
        ignored_root_entry: None,
    },
    SurfaceSpec {
        name: "turn_ledger",
        directory: "turn_ledger",
        shape: SurfaceShape::File("jsonl"),
        ignored_root_entry: None,
    },
    SurfaceSpec {
        name: "artifacts",
        directory: "artifacts",
        shape: SurfaceShape::Directory,
        ignored_root_entry: Some("index.jsonl"),
    },
    SurfaceSpec {
        name: "media",
        directory: "media",
        shape: SurfaceShape::Directory,
        ignored_root_entry: Some("index.jsonl"),
    },
    SurfaceSpec {
        name: "extractions",
        directory: "extractions",
        shape: SurfaceShape::Directory,
        ignored_root_entry: Some("index.jsonl"),
    },
    SurfaceSpec {
        name: "verifications",
        directory: "verifications",
        shape: SurfaceShape::Directory,
        ignored_root_entry: Some("index.jsonl"),
    },
    SurfaceSpec {
        name: "context_packs",
        directory: "context_packs",
        shape: SurfaceShape::Directory,
        ignored_root_entry: Some("index.jsonl"),
    },
    SurfaceSpec {
        name: "coder_turn_checkpoints",
        directory: "coder_turn_checkpoints",
        shape: SurfaceShape::Directory,
        ignored_root_entry: None,
    },
];

pub fn inventory(data_root: &Path, apply: bool) -> Result<SessionMigrationReport, String> {
    let resumable = load_report(data_root)?
        .into_iter()
        .flat_map(|report| report.entries)
        .filter(|entry| entry.status == MigrationEntryStatus::Planned)
        .filter_map(|entry| {
            Some((
                entry.surface,
                entry.candidate_session_id?,
                entry.storage_key?,
            ))
        })
        .collect::<HashSet<_>>();
    let mut report = SessionMigrationReport {
        layout_version: 1,
        dry_run: !apply,
        generated_at_utc: Utc::now(),
        entries_scanned: 0,
        bytes_scanned: 0,
        current: 0,
        planned: 0,
        migrated: 0,
        quarantined: 0,
        entries: Vec::new(),
    };

    for spec in SURFACES {
        inventory_surface(data_root, *spec, apply, &resumable, &mut report)?;
    }
    recount(&mut report);
    persist_report(data_root, &report)?;
    Ok(report)
}

pub fn load_report(data_root: &Path) -> Result<Option<SessionMigrationReport>, String> {
    let root = StoreRoot::open_or_create(&data_root.join("session_migrations"))
        .map_err(|error| error.to_string())?;
    let path = StorePath::parse(MIGRATION_RECORD).expect("static migration record path");
    match root.read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| "migration record is corrupt".to_string()),
        Err(error) if error.is_not_found() => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn inventory_surface(
    data_root: &Path,
    spec: SurfaceSpec,
    apply: bool,
    resumable: &HashSet<(String, String, String)>,
    report: &mut SessionMigrationReport,
) -> Result<(), String> {
    let root = StoreRoot::open_or_create(&data_root.join(spec.directory))
        .map_err(|error| format!("open {}: {error}", spec.name))?;
    for entry in root.list_root().map_err(|error| error.to_string())? {
        if report.entries_scanned >= MAX_ENTRIES {
            return Err("migration inventory entry limit exceeded".to_string());
        }
        report.entries_scanned += 1;
        let mut entry_size = entry.size;
        report.bytes_scanned = report.bytes_scanned.saturating_add(entry.size);
        if report.bytes_scanned > MAX_TOTAL_BYTES {
            return Err("migration inventory byte limit exceeded".to_string());
        }
        let name = entry.path.file_name();
        if spec.ignored_root_entry == Some(name) {
            continue;
        }
        let name_digest = digest_name(name);
        let entry_kind = kind_name(entry.kind).to_string();
        let expected_kind = match spec.shape {
            SurfaceShape::File(_) => StoreEntryKind::File,
            SurfaceShape::Directory => StoreEntryKind::Directory,
        };
        if entry.kind != expected_kind {
            report.entries.push(quarantined(
                spec,
                name_digest,
                entry_kind,
                entry.size,
                "unsupported_entry_type",
            ));
            continue;
        }
        if matches!(spec.shape, SurfaceShape::Directory) {
            let Some(size) = inspect_directory(&root, &entry.path, 0, report)? else {
                report.entries.push(quarantined(
                    spec,
                    name_digest,
                    entry_kind,
                    entry_size,
                    "unsupported_nested_entry_type",
                ));
                continue;
            };
            entry_size = size;
        }

        let Some(stem) = candidate_stem(name, spec.shape) else {
            report.entries.push(quarantined(
                spec,
                name_digest,
                entry_kind,
                entry_size,
                "unexpected_name",
            ));
            continue;
        };
        if StorageKey::is_encoded(stem) {
            report.entries.push(SessionMigrationEntry {
                surface: spec.name.to_string(),
                name_digest,
                entry_kind,
                size: entry_size,
                status: MigrationEntryStatus::Current,
                candidate_session_id: None,
                storage_key: Some(stem.to_string()),
                collision_group: None,
                reason_class: None,
            });
            continue;
        }
        let Ok(session_id) = SessionId::parse(stem) else {
            report.entries.push(quarantined(
                spec,
                name_digest,
                entry_kind,
                entry_size,
                "invalid_identifier",
            ));
            continue;
        };
        let storage_key = StorageKey::for_session(&session_id);
        let destination = destination_path(&storage_key, spec.shape)?;
        let destination_exists = match spec.shape {
            SurfaceShape::File(_) => root.is_file(&destination),
            SurfaceShape::Directory => root.is_dir(&destination),
        }
        .map_err(|error| error.to_string())?;
        if destination_exists {
            let mut identical =
                match entries_identical(&root, &entry.path, &destination, spec.shape) {
                    Ok(identical) => identical,
                    Err(_) => {
                        report.entries.push(SessionMigrationEntry {
                            surface: spec.name.to_string(),
                            name_digest,
                            entry_kind,
                            size: entry_size,
                            status: MigrationEntryStatus::Quarantined,
                            candidate_session_id: Some(session_id.to_string()),
                            storage_key: Some(storage_key.as_str().to_string()),
                            collision_group: Some(storage_key.as_str().to_string()),
                            reason_class: Some("destination_unsupported".to_string()),
                        });
                        continue;
                    }
                };
            let resume_key = (
                spec.name.to_string(),
                session_id.to_string(),
                storage_key.as_str().to_string(),
            );
            if apply && !identical && resumable.contains(&resume_key) {
                copy_entry(&root, &entry.path, &destination, spec.shape)?;
                identical = entries_identical(&root, &entry.path, &destination, spec.shape)?;
            }
            report.entries.push(SessionMigrationEntry {
                surface: spec.name.to_string(),
                name_digest,
                entry_kind,
                size: entry_size,
                status: if identical {
                    MigrationEntryStatus::Migrated
                } else {
                    MigrationEntryStatus::Quarantined
                },
                candidate_session_id: Some(session_id.to_string()),
                storage_key: Some(storage_key.as_str().to_string()),
                collision_group: (!identical).then(|| storage_key.as_str().to_string()),
                reason_class: (!identical).then(|| "destination_collision".to_string()),
            });
            continue;
        }

        let mut planned = SessionMigrationEntry {
            surface: spec.name.to_string(),
            name_digest,
            entry_kind,
            size: entry_size,
            status: MigrationEntryStatus::Planned,
            candidate_session_id: Some(session_id.to_string()),
            storage_key: Some(storage_key.as_str().to_string()),
            collision_group: None,
            reason_class: None,
        };
        report.entries.push(planned.clone());
        if apply {
            recount(report);
            persist_report(data_root, report)?;
            copy_entry(&root, &entry.path, &destination, spec.shape)?;
            if !entries_identical(&root, &entry.path, &destination, spec.shape)? {
                return Err(format!("{} migration verification failed", spec.name));
            }
            planned.status = MigrationEntryStatus::Migrated;
            if let Some(last) = report.entries.last_mut() {
                *last = planned;
            }
            recount(report);
            persist_report(data_root, report)?;
        }
    }
    Ok(())
}

fn inspect_directory(
    root: &StoreRoot,
    path: &StorePath,
    depth: usize,
    report: &mut SessionMigrationReport,
) -> Result<Option<u64>, String> {
    if depth >= 64 {
        return Ok(None);
    }
    let mut bytes = 0u64;
    for entry in root
        .list_directory(path)
        .map_err(|error| error.to_string())?
    {
        report.entries_scanned = report.entries_scanned.saturating_add(1);
        if report.entries_scanned > MAX_ENTRIES {
            return Err("migration inventory entry limit exceeded".to_string());
        }
        match entry.kind {
            StoreEntryKind::File => {
                if entry.size > MAX_FILE_BYTES {
                    return Ok(None);
                }
                bytes = bytes.saturating_add(entry.size);
                report.bytes_scanned = report.bytes_scanned.saturating_add(entry.size);
            }
            StoreEntryKind::Directory => {
                let child_path = path.join(&entry.path).map_err(|error| error.to_string())?;
                let Some(child_bytes) = inspect_directory(root, &child_path, depth + 1, report)?
                else {
                    return Ok(None);
                };
                bytes = bytes.saturating_add(child_bytes);
            }
            StoreEntryKind::Link | StoreEntryKind::Other => return Ok(None),
        }
        if report.bytes_scanned > MAX_TOTAL_BYTES || bytes > MAX_TOTAL_BYTES {
            return Err("migration inventory byte limit exceeded".to_string());
        }
    }
    Ok(Some(bytes))
}

fn copy_entry(
    root: &StoreRoot,
    source: &StorePath,
    destination: &StorePath,
    shape: SurfaceShape,
) -> Result<(), String> {
    match shape {
        SurfaceShape::File(_) => {
            let bytes = root
                .read_limited(source, MAX_FILE_BYTES)
                .map_err(|error| error.to_string())?;
            root.atomic_write(destination, &bytes)
                .map_err(|error| error.to_string())
        }
        SurfaceShape::Directory => {
            root.create_dir_all(destination)
                .map_err(|error| error.to_string())?;
            copy_directory(root, source, destination, 0)
        }
    }
}

fn copy_directory(
    root: &StoreRoot,
    source: &StorePath,
    destination: &StorePath,
    depth: usize,
) -> Result<(), String> {
    if depth >= 64 {
        return Err("migration directory depth limit exceeded".to_string());
    }
    for entry in root
        .list_directory(source)
        .map_err(|error| error.to_string())?
    {
        let child = entry.path.clone();
        let source_child = source.join(&child).map_err(|error| error.to_string())?;
        let destination_child = destination
            .join(&child)
            .map_err(|error| error.to_string())?;
        match entry.kind {
            StoreEntryKind::File => {
                let bytes = root
                    .read_limited(&source_child, MAX_FILE_BYTES)
                    .map_err(|error| error.to_string())?;
                root.atomic_write(&destination_child, &bytes)
                    .map_err(|error| error.to_string())?;
            }
            StoreEntryKind::Directory => {
                root.create_dir_all(&destination_child)
                    .map_err(|error| error.to_string())?;
                copy_directory(root, &source_child, &destination_child, depth + 1)?;
            }
            StoreEntryKind::Link | StoreEntryKind::Other => {
                return Err("migration source contains unsupported entry type".to_string());
            }
        }
    }
    Ok(())
}

fn entries_identical(
    root: &StoreRoot,
    left: &StorePath,
    right: &StorePath,
    shape: SurfaceShape,
) -> Result<bool, String> {
    match shape {
        SurfaceShape::File(_) => Ok(root
            .read_limited(left, MAX_FILE_BYTES)
            .map_err(|error| error.to_string())?
            == root
                .read_limited(right, MAX_FILE_BYTES)
                .map_err(|error| error.to_string())?),
        SurfaceShape::Directory => directory_digest(root, left, 0)
            .and_then(|left| directory_digest(root, right, 0).map(|right| left == right)),
    }
}

fn directory_digest(root: &StoreRoot, path: &StorePath, depth: usize) -> Result<[u8; 32], String> {
    if depth >= 64 {
        return Err("migration directory depth limit exceeded".to_string());
    }
    let mut entries = root
        .list_directory(path)
        .map_err(|error| error.to_string())?;
    entries.sort_by(|left, right| left.path.file_name().cmp(right.path.file_name()));
    let mut hasher = Sha256::new();
    for entry in entries {
        hasher.update(entry.path.file_name().as_bytes());
        let child_path = path.join(&entry.path).map_err(|error| error.to_string())?;
        match entry.kind {
            StoreEntryKind::File => {
                hasher.update([0]);
                hasher.update(
                    root.read_limited(&child_path, MAX_FILE_BYTES)
                        .map_err(|error| error.to_string())?,
                );
            }
            StoreEntryKind::Directory => {
                hasher.update([1]);
                hasher.update(directory_digest(root, &child_path, depth + 1)?);
            }
            StoreEntryKind::Link | StoreEntryKind::Other => {
                return Err("migration source contains unsupported entry type".to_string());
            }
        }
    }
    Ok(hasher.finalize().into())
}

fn destination_path(key: &StorageKey, shape: SurfaceShape) -> Result<StorePath, String> {
    let name = match shape {
        SurfaceShape::File(extension) => format!("{}.{}", key.as_str(), extension),
        SurfaceShape::Directory => key.as_str().to_string(),
    };
    StorePath::parse(&name).map_err(|error| error.to_string())
}

fn candidate_stem(name: &str, shape: SurfaceShape) -> Option<&str> {
    match shape {
        SurfaceShape::File(extension) => name.strip_suffix(&format!(".{extension}")),
        SurfaceShape::Directory => Some(name),
    }
}

fn quarantined(
    spec: SurfaceSpec,
    name_digest: String,
    entry_kind: String,
    size: u64,
    reason: &str,
) -> SessionMigrationEntry {
    SessionMigrationEntry {
        surface: spec.name.to_string(),
        name_digest,
        entry_kind,
        size,
        status: MigrationEntryStatus::Quarantined,
        candidate_session_id: None,
        storage_key: None,
        collision_group: None,
        reason_class: Some(reason.to_string()),
    }
}

fn digest_name(name: &str) -> String {
    format!("n1-{:x}", Sha256::digest(name.as_bytes()))
}

fn kind_name(kind: StoreEntryKind) -> &'static str {
    match kind {
        StoreEntryKind::File => "file",
        StoreEntryKind::Directory => "directory",
        StoreEntryKind::Link => "link",
        StoreEntryKind::Other => "other",
    }
}

fn recount(report: &mut SessionMigrationReport) {
    report.current = report
        .entries
        .iter()
        .filter(|entry| entry.status == MigrationEntryStatus::Current)
        .count();
    report.planned = report
        .entries
        .iter()
        .filter(|entry| entry.status == MigrationEntryStatus::Planned)
        .count();
    report.migrated = report
        .entries
        .iter()
        .filter(|entry| entry.status == MigrationEntryStatus::Migrated)
        .count();
    report.quarantined = report
        .entries
        .iter()
        .filter(|entry| entry.status == MigrationEntryStatus::Quarantined)
        .count();
}

fn persist_report(data_root: &Path, report: &SessionMigrationReport) -> Result<(), String> {
    let root = StoreRoot::open_or_create(&data_root.join("session_migrations"))
        .map_err(|error| error.to_string())?;
    let path = StorePath::parse(MIGRATION_RECORD).expect("static migration record path");
    let bytes = serde_json::to_vec_pretty(report).map_err(|error| error.to_string())?;
    root.atomic_write(&path, &bytes)
        .map_err(|error| error.to_string())
}

pub fn record_path(data_root: &Path) -> PathBuf {
    data_root.join("session_migrations").join(MIGRATION_RECORD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_reports_without_mutating_and_redacts_invalid_names() {
        let temp = tempfile::tempdir().unwrap();
        let history = temp.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        std::fs::write(history.join("session-a.jsonl"), b"turn\n").unwrap();
        std::fs::write(history.join("bad.name.jsonl"), b"hostile\n").unwrap();

        let report = inventory(temp.path(), false).unwrap();
        assert_eq!(report.planned, 1);
        assert_eq!(report.quarantined, 1);
        assert!(history.join("session-a.jsonl").exists());
        assert!(
            report
                .entries
                .iter()
                .all(|entry| entry.name_digest.starts_with("n1-"))
        );
        assert!(
            !std::fs::read_to_string(record_path(temp.path()))
                .unwrap()
                .contains("bad.name")
        );
    }

    #[test]
    fn apply_is_restartable_and_retains_verified_legacy_source() {
        let temp = tempfile::tempdir().unwrap();
        let history = temp.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        std::fs::write(history.join("session-a.jsonl"), b"turn\n").unwrap();

        let first = inventory(temp.path(), true).unwrap();
        assert_eq!(first.migrated, 1);
        assert!(history.join("session-a.jsonl").exists());
        let opaque = format!(
            "{}.jsonl",
            StorageKey::for_session(&SessionId::parse("session-a").unwrap()).as_str()
        );
        assert_eq!(std::fs::read(history.join(&opaque)).unwrap(), b"turn\n");

        let second = inventory(temp.path(), true).unwrap();
        assert_eq!(second.migrated, 1);
        assert_eq!(std::fs::read(history.join(opaque)).unwrap(), b"turn\n");
    }

    #[test]
    fn interrupted_directory_copy_resumes_only_from_a_planned_journal_entry() {
        let temp = tempfile::tempdir().unwrap();
        let artifacts = temp.path().join("artifacts");
        let legacy = artifacts.join("session-a");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("one.json"), b"one").unwrap();
        std::fs::write(legacy.join("two.json"), b"two").unwrap();
        let key = StorageKey::for_session(&SessionId::parse("session-a").unwrap());
        let partial = artifacts.join(key.as_str());
        std::fs::create_dir_all(&partial).unwrap();
        std::fs::write(partial.join("one.json"), b"one").unwrap();
        let mut report = SessionMigrationReport {
            layout_version: 1,
            dry_run: false,
            generated_at_utc: Utc::now(),
            entries_scanned: 1,
            bytes_scanned: 6,
            current: 0,
            planned: 1,
            migrated: 0,
            quarantined: 0,
            entries: vec![SessionMigrationEntry {
                surface: "artifacts".to_string(),
                name_digest: digest_name("session-a"),
                entry_kind: "directory".to_string(),
                size: 0,
                status: MigrationEntryStatus::Planned,
                candidate_session_id: Some("session-a".to_string()),
                storage_key: Some(key.as_str().to_string()),
                collision_group: None,
                reason_class: None,
            }],
        };
        recount(&mut report);
        persist_report(temp.path(), &report).unwrap();

        let resumed = inventory(temp.path(), true).unwrap();
        assert!(
            resumed.entries.iter().any(|entry| {
                entry.surface == "artifacts" && entry.status == MigrationEntryStatus::Migrated
            }),
            "{:#?}",
            resumed.entries
        );
        assert_eq!(std::fs::read(partial.join("two.json")).unwrap(), b"two");
        assert!(legacy.exists());
    }

    #[test]
    fn unjournaled_destination_mismatch_is_quarantined_and_unchanged() {
        let temp = tempfile::tempdir().unwrap();
        let history = temp.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        std::fs::write(history.join("session-a.jsonl"), b"legacy\n").unwrap();
        let opaque = format!(
            "{}.jsonl",
            StorageKey::for_session(&SessionId::parse("session-a").unwrap()).as_str()
        );
        std::fs::write(history.join(&opaque), b"different\n").unwrap();

        let report = inventory(temp.path(), true).unwrap();
        assert!(report.entries.iter().any(|entry| {
            entry.surface == "transcript"
                && entry.status == MigrationEntryStatus::Quarantined
                && entry.reason_class.as_deref() == Some("destination_collision")
        }));
        assert_eq!(std::fs::read(history.join(opaque)).unwrap(), b"different\n");
        assert_eq!(
            std::fs::read(history.join("session-a.jsonl")).unwrap(),
            b"legacy\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn link_backed_legacy_entry_is_quarantined_without_following() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("canary"), b"outside").unwrap();
        let artifacts = temp.path().join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();
        symlink(outside.path(), artifacts.join("session-a")).unwrap();

        let report = inventory(temp.path(), true).unwrap();
        assert!(report.entries.iter().any(|entry| {
            entry.surface == "artifacts"
                && entry.status == MigrationEntryStatus::Quarantined
                && entry.reason_class.as_deref() == Some("unsupported_entry_type")
        }));
        assert_eq!(
            std::fs::read(outside.path().join("canary")).unwrap(),
            b"outside"
        );
    }
}
