//! Bounded streaming recovery for workspace snapshots, journals, and legacy feeds.

use super::*;
use serde::de::DeserializeOwned;
use std::io::{BufRead, BufReader};

pub(super) fn load_projection_at(
    root_path: &Path,
) -> Result<WorkspaceProjection, PersistenceError> {
    let _disk = DISK_ACCESS.lock().expect("workspace disk access");
    let root = StoreRoot::open_or_create_nofollow(root_path)?;
    let Some(snapshot) = open_optional(&root, SNAPSHOT_FILE)? else {
        reject_nonempty_journal_without_snapshot(&root)?;
        return load_legacy_projection(root_path, &root);
    };
    let mut projection: WorkspaceProjection = serde_json::from_reader(BufReader::new(snapshot))
        .map_err(|error| json_read_error(SNAPSHOT_FILE, error))?;
    if projection.schema_version != 2 {
        return Err(PersistenceError::new(
            PersistenceErrorKind::Corruption,
            "unsupported workspace projection version",
        ));
    }
    let snapshot_generation = projection.generation;
    let mut replay_generation = snapshot_generation;
    let mut saw_newer_generation = false;

    stream_complete_records(
        &root,
        JOURNAL_FILE,
        MAX_JOURNAL_RECORD_BYTES,
        |record_index, line| {
            let record: WorkspaceJournalRecord = serde_json::from_slice(line).map_err(|error| {
                PersistenceError::new(
                    PersistenceErrorKind::Corruption,
                    format!(
                        "workspace journal `{JOURNAL_FILE}` record {record_index} is invalid: {error}"
                    ),
                )
            })?;
            if record.schema_version != 2 {
                return Err(PersistenceError::new(
                    PersistenceErrorKind::Corruption,
                    format!(
                        "workspace journal `{JOURNAL_FILE}` record {record_index} has unsupported version {}",
                        record.schema_version
                    ),
                ));
            }
            if record.generation <= snapshot_generation && !saw_newer_generation {
                return Ok(());
            }
            if Some(record.generation) != replay_generation.checked_add(1) {
                return Err(PersistenceError::new(
                    PersistenceErrorKind::Corruption,
                    format!(
                        "workspace journal `{JOURNAL_FILE}` has a duplicate, regression, or generation gap at record {record_index}"
                    ),
                ));
            }
            saw_newer_generation = true;
            replay_generation = record.generation;
            projection.generation = record.generation;
            for mutation in record.mutations {
                projection.apply(mutation);
            }
            Ok(())
        },
    )?;

    projection.recalculate_bounds();
    Ok(projection)
}

fn load_legacy_projection(
    root_path: &Path,
    root: &StoreRoot,
) -> Result<WorkspaceProjection, PersistenceError> {
    let mut projection = WorkspaceProjection::default();
    if let Some(raw) = read_optional(root, LEGACY_REVISION_FILE, 64)? {
        let revision = std::str::from_utf8(&raw)
            .map_err(|error| {
                PersistenceError::new(
                    PersistenceErrorKind::Corruption,
                    format!("workspace revision `{LEGACY_REVISION_FILE}` is invalid: {error}"),
                )
            })?
            .trim()
            .parse::<u64>()
            .map_err(|error| {
                PersistenceError::new(
                    PersistenceErrorKind::Corruption,
                    format!("workspace revision `{LEGACY_REVISION_FILE}` is invalid: {error}"),
                )
            })?;
        projection.revision = revision;
    }
    stream_complete_records(
        root,
        LEGACY_FEED_FILE,
        MAX_JOURNAL_RECORD_BYTES,
        |record_index, line| {
            let event: WorkspaceEvent = serde_json::from_slice(line).map_err(|error| {
                PersistenceError::new(
                    PersistenceErrorKind::Corruption,
                    format!(
                        "legacy workspace feed `{LEGACY_FEED_FILE}` record {record_index} is invalid: {error}"
                    ),
                )
            })?;
            projection.retain_feed_event(event);
            Ok(())
        },
    )?;

    if let Some(snapshot) =
        read_json_optional::<LegacyCardStateSnapshot>(root, LEGACY_CARD_STATE_FILE)?
    {
        projection.card_columns = snapshot.columns;
    }
    if let Some(rows) = read_json_optional::<Vec<LegacyAssociationRecord>>(root, LEGACY_ASSOC_FILE)?
    {
        projection.associations = rows
            .into_iter()
            .map(|row| {
                (
                    row.card_id,
                    WorkCardAssociations {
                        vault_paths: row.vault_paths,
                        artifact_ids: row.artifact_ids,
                        locus_node_ids: row.locus_node_ids,
                    },
                )
            })
            .collect();
    }
    if let Some(records) =
        read_json_optional::<HashMap<String, AskJobRecord>>(root, LEGACY_ASK_JOBS_FILE)?
    {
        projection.ask_jobs = records;
    }
    let workspace_workers =
        read_json_optional::<HashMap<String, TurnWorkRecord>>(root, LEGACY_TURN_WORKERS_FILE)?;
    if let Some(records) = workspace_workers {
        projection.turn_workers = records;
    } else if let Some(data_dir) = root_path.parent() {
        let data_root = StoreRoot::open_or_create_nofollow(data_dir)?;
        if let Some(records) =
            read_json_optional::<HashMap<String, TurnWorkRecord>>(&data_root, "turn_workers.json")?
        {
            projection.turn_workers = records;
        }
    }
    projection.recalculate_bounds();
    Ok(projection)
}

fn read_optional(
    root: &StoreRoot,
    relative: &str,
    limit: u64,
) -> Result<Option<Vec<u8>>, PersistenceError> {
    let path = StorePath::parse(relative)?;
    match root.read_limited(&path, limit) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.is_not_found() => Ok(None),
        Err(error) => Err(with_store_context(
            format!("workspace file `{relative}`"),
            error,
        )),
    }
}

fn open_optional(
    root: &StoreRoot,
    relative: &str,
) -> Result<Option<std::fs::File>, PersistenceError> {
    let path = StorePath::parse(relative)?;
    match root.open_read_file(&path) {
        Ok(file) => Ok(Some(file)),
        Err(error) if error.is_not_found() => Ok(None),
        Err(error) => Err(with_store_context(
            format!("workspace file `{relative}`"),
            error,
        )),
    }
}

fn reject_nonempty_journal_without_snapshot(root: &StoreRoot) -> Result<(), PersistenceError> {
    let Some(file) = open_optional(root, JOURNAL_FILE)? else {
        return Ok(());
    };
    let size = file
        .metadata()
        .map_err(|error| {
            PersistenceError::new(
                PersistenceErrorKind::PermanentIo,
                format!("reading workspace journal `{JOURNAL_FILE}` metadata failed: {error}"),
            )
        })?
        .len();
    if size > 0 {
        return Err(PersistenceError::new(
            PersistenceErrorKind::Corruption,
            format!("workspace journal `{JOURNAL_FILE}` has data but its snapshot is missing"),
        ));
    }
    Ok(())
}

fn read_json_optional<T: DeserializeOwned>(
    root: &StoreRoot,
    relative: &str,
) -> Result<Option<T>, PersistenceError> {
    let Some(file) = open_optional(root, relative)? else {
        return Ok(None);
    };
    serde_json::from_reader(BufReader::new(file))
        .map(Some)
        .map_err(|error| json_read_error(relative, error))
}

fn json_read_error(relative: &str, error: serde_json::Error) -> PersistenceError {
    let kind = match error.classify() {
        serde_json::error::Category::Io => PersistenceErrorKind::PermanentIo,
        serde_json::error::Category::Syntax
        | serde_json::error::Category::Data
        | serde_json::error::Category::Eof => PersistenceErrorKind::Corruption,
    };
    PersistenceError::new(
        kind,
        format!("workspace file `{relative}` is invalid: {error}"),
    )
}

fn stream_complete_records(
    root: &StoreRoot,
    relative: &str,
    max_record_bytes: usize,
    mut consume: impl FnMut(u64, &[u8]) -> Result<(), PersistenceError>,
) -> Result<(), PersistenceError> {
    let path = StorePath::parse(relative)?;
    let file = match root.open_read_file(&path) {
        Ok(file) => file,
        Err(error) if error.is_not_found() => return Ok(()),
        Err(error) => {
            return Err(with_store_context(
                format!("workspace stream `{relative}`"),
                error,
            ));
        }
    };

    let mut reader = BufReader::new(file);
    let mut record = Vec::new();
    let mut record_index = 0_u64;
    loop {
        let available = reader.fill_buf().map_err(|error| {
            PersistenceError::new(
                PersistenceErrorKind::PermanentIo,
                format!("reading workspace stream `{relative}`: {error}"),
            )
        })?;
        if available.is_empty() {
            // A final line without its delimiter may be a crash-truncated write.
            // It was historically ignored for both the journal and legacy feed.
            break;
        }
        if let Some(newline) = available.iter().position(|byte| *byte == b'\n') {
            if record.len().saturating_add(newline) > max_record_bytes {
                return Err(oversized_record_error(
                    relative,
                    record_index + 1,
                    max_record_bytes,
                ));
            }
            record.extend_from_slice(&available[..newline]);
            reader.consume(newline + 1);
            record_index += 1;
            if record.last() == Some(&b'\r') {
                record.pop();
            }
            if !record.iter().all(u8::is_ascii_whitespace) {
                consume(record_index, &record)?;
            }
            record.clear();
        } else {
            let length = available.len();
            if record.len().saturating_add(length) > max_record_bytes {
                return Err(oversized_record_error(
                    relative,
                    record_index + 1,
                    max_record_bytes,
                ));
            }
            record.extend_from_slice(available);
            reader.consume(length);
        }
    }
    Ok(())
}

fn oversized_record_error(
    relative: &str,
    record_index: u64,
    max_record_bytes: usize,
) -> PersistenceError {
    PersistenceError::new(
        PersistenceErrorKind::Corruption,
        format!(
            "workspace stream `{relative}` record {record_index} exceeds the {max_record_bytes}-byte record limit"
        ),
    )
}

fn with_store_context(
    context: String,
    error: crate::store_root::StoreRootError,
) -> PersistenceError {
    let error: PersistenceError = error.into();
    PersistenceError::new(error.kind, format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn test_root() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap().join("workspace");
        std::fs::create_dir_all(&root).unwrap();
        (directory, root)
    }

    fn event(id: impl Into<String>, summary: &str) -> WorkspaceEvent {
        WorkspaceEvent {
            id: id.into(),
            timestamp_utc: Utc::now(),
            kind: crate::daemon_api::WorkspaceEventKind::TurnCompleted,
            actor: crate::daemon_api::WorkspaceEventActor::System,
            summary: summary.to_string(),
            refs: Vec::new(),
            detail_line: None,
            context_line: None,
            intent: None,
            tool_names: Vec::new(),
        }
    }

    fn write_legacy_feed(root: &Path, event_count: usize, summary_bytes: usize) -> u64 {
        let mut file = std::fs::File::create(root.join(LEGACY_FEED_FILE)).unwrap();
        let summary = "x".repeat(summary_bytes);
        for index in 0..event_count {
            let event = event(format!("legacy-{index:06}"), &summary);
            writeln!(file, "{}", serde_json::to_string(&event).unwrap()).unwrap();
        }
        file.metadata().unwrap().len()
    }

    #[test]
    fn legacy_feed_over_eight_mebibytes_streams_and_retains_its_bounded_tail() {
        let (_directory, root_path) = test_root();
        let file_bytes = write_legacy_feed(&root_path, 9_000, 1_000);
        assert!(file_bytes > MAX_JOURNAL_BYTES as u64);
        std::fs::write(root_path.join(LEGACY_REVISION_FILE), "42\n").unwrap();
        std::fs::write(
            root_path.join(LEGACY_CARD_STATE_FILE),
            r#"{"columns":{"card-1":"done"}}"#,
        )
        .unwrap();
        std::fs::write(
            root_path.join(LEGACY_ASSOC_FILE),
            r#"[{"card_id":"card-1","vault_paths":["notes/project.md"]}]"#,
        )
        .unwrap();

        let projection = load_projection_at(&root_path).unwrap();
        assert_eq!(projection.revision, 42);
        assert_eq!(
            projection.card_columns.get("card-1"),
            Some(&WorkBoardColumn::Done)
        );
        assert_eq!(
            projection.associations["card-1"].vault_paths,
            vec!["notes/project.md"]
        );
        assert!(projection.feed.len() < 9_000);
        assert!(projection.feed_bytes <= MAX_WORKSPACE_FEED_BYTES);
        assert_eq!(
            projection.feed.back().map(|event| event.id.as_str()),
            Some("legacy-008999")
        );
        assert_ne!(
            projection.feed.front().map(|event| event.id.as_str()),
            Some("legacy-000000")
        );
    }

    #[test]
    fn journal_over_sixty_four_mebibytes_streams_and_replays_all_generations() {
        let (_directory, root_path) = test_root();
        let store_root = Arc::new(StoreRoot::open_or_create_nofollow(&root_path).unwrap());
        let transaction = FileTransaction::new(Arc::clone(&store_root));
        publish_snapshot(&transaction, &WorkspaceProjection::default()).unwrap();
        let journal_path = root_path.join(JOURNAL_FILE);
        let mut journal = std::fs::File::create(journal_path).unwrap();
        let mut bytes_written = 0_u64;
        for generation in 1..=70_000 {
            let record = WorkspaceJournalRecord {
                schema_version: 2,
                generation,
                mutations: vec![WorkspaceMutation::AppendEventAndRevision {
                    event: event(format!("journal-{generation:06}"), &"j".repeat(1_000)),
                    revision: generation,
                }],
            };
            let encoded = serde_json::to_vec(&record).unwrap();
            bytes_written += encoded.len() as u64 + 1;
            journal.write_all(&encoded).unwrap();
            journal.write_all(b"\n").unwrap();
        }
        drop(journal);
        assert!(bytes_written > 64 * 1024 * 1024);

        let projection = load_projection_at(&root_path).unwrap();
        assert_eq!(projection.generation, 70_000);
        assert_eq!(projection.revision, 70_000);
        assert!(projection.feed_bytes <= MAX_WORKSPACE_FEED_BYTES);
        assert_eq!(
            projection.feed.back().map(|event| event.id.as_str()),
            Some("journal-070000")
        );
    }

    #[test]
    fn journal_ignores_only_an_incomplete_final_record() {
        let (_directory, root_path) = test_root();
        let root = Arc::new(StoreRoot::open_or_create_nofollow(&root_path).unwrap());
        let transaction = FileTransaction::new(Arc::clone(&root));
        publish_snapshot(&transaction, &WorkspaceProjection::default()).unwrap();
        let journal = StorePath::parse(JOURNAL_FILE).unwrap();
        let committed = WorkspaceJournalRecord {
            schema_version: 2,
            generation: 1,
            mutations: vec![WorkspaceMutation::SetRevision { revision: 7 }],
        };
        transaction
            .append_record(
                &journal,
                &serde_json::to_vec(&committed).unwrap(),
                DurabilityLevel::Written,
            )
            .unwrap();
        root.append(&journal, b"{\"schema_version\":2").unwrap();

        let recovered = load_projection_at(&root_path).unwrap();
        assert_eq!(recovered.generation, 1);
        assert_eq!(recovered.revision, 7);
    }

    #[test]
    fn journal_rejects_corrupt_complete_records_and_generation_gaps() {
        for second_record in [
            b"{invalid}\n".as_slice(),
            br#"{"schema_version":2,"generation":3,"mutations":[]}"#.as_slice(),
            br#"{"schema_version":2,"generation":1,"mutations":[]}"#.as_slice(),
            br#"{"schema_version":2,"generation":0,"mutations":[]}"#.as_slice(),
        ] {
            let (_directory, root_path) = test_root();
            let root = Arc::new(StoreRoot::open_or_create_nofollow(&root_path).unwrap());
            let transaction = FileTransaction::new(Arc::clone(&root));
            publish_snapshot(&transaction, &WorkspaceProjection::default()).unwrap();
            let journal = StorePath::parse(JOURNAL_FILE).unwrap();
            let first_record = WorkspaceJournalRecord {
                schema_version: 2,
                generation: 1,
                mutations: vec![WorkspaceMutation::SetRevision { revision: 1 }],
            };
            transaction
                .append_record(
                    &journal,
                    &serde_json::to_vec(&first_record).unwrap(),
                    DurabilityLevel::Written,
                )
                .unwrap();
            transaction
                .append_record(&journal, second_record, DurabilityLevel::Written)
                .unwrap();

            assert!(
                load_projection_at(&root_path).is_err(),
                "accepted corrupt record: {:?}",
                second_record
            );
        }
    }

    #[test]
    fn journal_without_snapshot_is_not_misread_as_a_legacy_workspace() {
        let (_directory, root_path) = test_root();
        std::fs::write(root_path.join(JOURNAL_FILE), b"{\"schema_version\":2}\n").unwrap();

        let error = load_projection_at(&root_path).unwrap_err();
        assert!(error.to_string().contains("snapshot is missing"));
    }

    #[test]
    fn malformed_legacy_revision_is_reported_as_corruption() {
        let (_directory, root_path) = test_root();
        std::fs::write(root_path.join(LEGACY_REVISION_FILE), "not-a-revision\n").unwrap();

        let error = load_projection_at(&root_path).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("workspace revision `revision` is invalid")
        );
    }

    #[test]
    fn stream_rejects_an_oversized_record_before_unbounded_buffering() {
        let (_directory, root_path) = test_root();
        let root = StoreRoot::open_or_create_nofollow(&root_path).unwrap();
        let feed = StorePath::parse(LEGACY_FEED_FILE).unwrap();
        let oversized = vec![b'x'; MAX_JOURNAL_RECORD_BYTES + 1];
        root.append(&feed, &oversized).unwrap();

        let error = load_projection_at(&root_path).unwrap_err();
        assert!(error.to_string().contains("record 1 exceeds"));
        assert!(error.to_string().contains(LEGACY_FEED_FILE));
    }

    #[test]
    fn journal_read_errors_are_not_treated_as_an_empty_journal() {
        let (_directory, root_path) = test_root();
        let root = Arc::new(StoreRoot::open_or_create_nofollow(&root_path).unwrap());
        let transaction = FileTransaction::new(Arc::clone(&root));
        publish_snapshot(&transaction, &WorkspaceProjection::default()).unwrap();
        std::fs::remove_file(root_path.join(JOURNAL_FILE)).unwrap();
        std::fs::create_dir(root_path.join(JOURNAL_FILE)).unwrap();

        let error = load_projection_at(&root_path).unwrap_err();
        assert!(error.to_string().contains("journal-v2.jsonl"));
    }
}
