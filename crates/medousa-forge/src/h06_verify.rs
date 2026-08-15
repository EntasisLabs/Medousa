//! H06 verification scaffolding.
//!
//! Tests here are intentionally named for what they currently prove. Full
//! CR-007 / CM-009 / CM-010 / ISO-006 acceptance remains Open/Scaffolding in
//! the H06 acceptance matrix until later cars add real coverage.

#[cfg(test)]
mod tests {
    use crate::catalog::SlugReservationJournal;
    use crate::events::EventPayload;
    use crate::model::{
        ActorKind, ActorRef, GitOid, GitWorkTarget, WorkItem, WorkState, WorkTarget,
    };
    use crate::store::FsWorkStore;
    use std::fs::OpenOptions;
    use std::io::Write;
    use tempfile::TempDir;

    fn actor() -> ActorRef {
        ActorRef {
            kind: ActorKind::System,
            id: "h06".into(),
        }
    }

    fn item(title: &str) -> WorkItem {
        WorkItem::new(
            title,
            "brief",
            WorkTarget::Git(GitWorkTarget {
                repo_path: std::path::PathBuf::from("/tmp/h06-repo"),
                base_ref: "main".into(),
                base_oid: GitOid::new("a".repeat(40)),
            }),
            "user-1",
        )
    }

    fn registered(item: &WorkItem) -> EventPayload {
        EventPayload::ItemRegistered {
            item: Box::new(item.clone()),
        }
    }

    #[test]
    fn partial_final_json_line_is_skipped_on_replay() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let work = item("cr007");
        store
            .append(&work.id, &actor(), registered(&work))
            .unwrap();
        store
            .append(
                &work.id,
                &actor(),
                EventPayload::StateChanged {
                    from: WorkState::Draft,
                    to: WorkState::Ready,
                    reason: Some("ready".into()),
                },
            )
            .unwrap();
        let path = store.events_path(&work.id);
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"schema_version\":1,\"work_id\":\"wor").unwrap();
        drop(file);
        let events = store.replay(&work.id).unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            events[1].payload,
            EventPayload::StateChanged { to: WorkState::Ready, .. }
        ));
        let next = store
            .append(
                &work.id,
                &actor(),
                EventPayload::StateChanged {
                    from: WorkState::Ready,
                    to: WorkState::Draft,
                    reason: Some("repair".into()),
                },
            )
            .unwrap();
        assert_eq!(next.seq, 3);
    }

    #[test]
    fn append_returns_monotonic_seq_and_updates_cached_tail() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let work = item("cm009");
        let first = store
            .append(&work.id, &actor(), registered(&work))
            .unwrap();
        let second = store
            .append(
                &work.id,
                &actor(),
                EventPayload::StateChanged {
                    from: WorkState::Draft,
                    to: WorkState::Ready,
                    reason: None,
                },
            )
            .unwrap();
        assert_eq!(first.seq, 1);
        assert_eq!(second.seq, 2);
        assert_eq!(store.cached_last_seq(&work.id).unwrap(), 2);
    }

    #[test]
    fn unrelated_items_maintain_independent_cached_tails() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let left = item("left");
        let right = item("right");
        store.append(&left.id, &actor(), registered(&left)).unwrap();
        store
            .append(&right.id, &actor(), registered(&right))
            .unwrap();
        store
            .append(
                &left.id,
                &actor(),
                EventPayload::StateChanged {
                    from: WorkState::Draft,
                    to: WorkState::Ready,
                    reason: None,
                },
            )
            .unwrap();
        assert_eq!(store.cached_last_seq(&left.id).unwrap(), 2);
        assert_eq!(store.cached_last_seq(&right.id).unwrap(), 1);
        assert_ne!(left.id, right.id);
    }

    #[test]
    fn distinct_work_ids_use_distinct_event_paths() {
        let tmp = TempDir::new().unwrap();
        let store = FsWorkStore::open(tmp.path()).unwrap();
        let left = item("iso-left");
        let right = item("iso-right");
        store.append(&left.id, &actor(), registered(&left)).unwrap();
        store
            .append(&right.id, &actor(), registered(&right))
            .unwrap();
        assert_ne!(store.events_path(&left.id), store.events_path(&right.id));
        assert!(store.item_exists(&left.id));
        assert!(store.item_exists(&right.id));
    }

    #[test]
    fn slug_reservation_survives_reopen() {
        let tmp = TempDir::new().unwrap();
        {
            let journal = SlugReservationJournal::open(tmp.path()).unwrap();
            journal.reserve("held", "op-crash").unwrap();
        }
        let journal = SlugReservationJournal::open(tmp.path()).unwrap();
        let orphans = journal.recover_orphans().unwrap();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].slug, "held");
        assert!(journal.reserve("held", "op-2").is_err());
    }
}
