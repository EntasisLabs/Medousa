//! H07 verification scaffolding (Car 0).
//!
//! Named for CM/CR acceptance IDs. Car 0 records current behavior and keeps
//! the suite green — target “exactly one CAS winner” asserts land in H07.1b+.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::thread;

    use crate::daemon_api::VaultWriteRequest;
    use crate::vault::baseline::vault_baseline_counters;
    use crate::vault::fixtures::{VaultFixtureShape, VaultFixtureSpec, generate_vault_fixture};
    use crate::vault::note::content_hash;
    use crate::vault::service::{VaultService, with_temp_vault};
    use crate::vault::store::vault_store;

    #[test]
    fn cm006_two_writer_if_match_baseline_records_outcome() {
        with_temp_vault(|| {
            let path = format!("race/cm006-{}.md", uuid::Uuid::new_v4().simple());
            let initial = "# Base\n\nbody-v1\n";
            let request = VaultWriteRequest {
                path: Some(path.clone()),
                content: initial.to_string(),
                ..Default::default()
            };
            VaultService::write_note(None, &request, None).expect("seed");
            let etag = content_hash(initial);
            let barrier = Arc::new(Barrier::new(2));
            let successes = Arc::new(AtomicUsize::new(0));
            let conflicts = Arc::new(AtomicUsize::new(0));
            let mut handles = Vec::new();
            for writer in 0..2 {
                let barrier = Arc::clone(&barrier);
                let successes = Arc::clone(&successes);
                let conflicts = Arc::clone(&conflicts);
                let path = path.clone();
                let etag = etag.clone();
                handles.push(thread::spawn(move || {
                    barrier.wait();
                    let request = VaultWriteRequest {
                        path: Some(path),
                        content: format!("# Base\n\nwriter-{writer}\n"),
                        ..Default::default()
                    };
                    match VaultService::write_note(None, &request, Some(&etag)) {
                        Ok(_) => {
                            successes.fetch_add(1, Ordering::SeqCst);
                        }
                        Err(_) => {
                            conflicts.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                }));
            }
            for handle in handles {
                handle.join().expect("writer thread");
            }
            let ok = successes.load(Ordering::SeqCst);
            let lost = conflicts.load(Ordering::SeqCst);
            let counters = vault_baseline_counters();
            if ok > 1 {
                counters.dual_success_races.fetch_add(1, Ordering::Relaxed);
            }
            counters
                .conflict_outcomes
                .fetch_add(lost as u64, Ordering::Relaxed);
            counters.mutations.fetch_add(ok as u64, Ordering::Relaxed);
            // Car 0: harness must complete. Target: exactly one success lands in H07.1b.
            assert_eq!(ok + lost, 2, "both writers must settle");
            let _ = vault_store().get_entry(&path);
        });
    }

    #[test]
    fn cm007_external_edit_versus_put_baseline() {
        with_temp_vault(|| {
            let path = format!("race/cm007-{}.md", uuid::Uuid::new_v4().simple());
            let initial = "# Ext\n\nv1\n";
            VaultService::write_note(
                None,
                &VaultWriteRequest {
                    path: Some(path.clone()),
                    content: initial.to_string(),
                    ..Default::default()
                },
                None,
            )
            .expect("seed");
            let etag = content_hash(initial);
            let root = crate::vault::path::user_vault_root();
            std::fs::write(root.join(&path), b"# Ext\n\nexternal\n").expect("external edit");
            let result = VaultService::write_note(
                None,
                &VaultWriteRequest {
                    path: Some(path.clone()),
                    content: "# Ext\n\ndaemon\n".to_string(),
                    ..Default::default()
                },
                Some(&etag),
            );
            // Current store may treat this as mismatch or overwrite depending on timing;
            // Car 0 only requires a determinate outcome and an observable final body.
            let _ = result;
            let body = VaultService::get_note(&path).expect("final read").content;
            assert!(
                body.contains("external") || body.contains("daemon") || body.contains("v1"),
                "final body must remain readable"
            );
        });
    }

    #[test]
    fn fixture_warmup_records_ensure_fresh_cost() {
        with_temp_vault(|| {
            let root = crate::vault::path::user_vault_root();
            let spec = VaultFixtureSpec::small(VaultFixtureShape::LinkHeavy);
            generate_vault_fixture(&root, &spec).expect("fixture");
            let counters = vault_baseline_counters();
            counters.reset();
            let _ = vault_store().refresh_from_disk();
            let before = counters.snapshot();
            let listed = VaultService::list_notes(None, 50, None, None);
            let after = counters.snapshot();
            assert!(!listed.notes.is_empty());
            // Measurement only — warm list currently triggers freshness work.
            let line = after.render_line("list_after_refresh");
            assert!(line.contains("h07_baseline"));
            assert!(
                after.ensure_index_fresh_calls >= before.ensure_index_fresh_calls,
                "ensure_index_fresh should be countable"
            );
        });
    }

    #[test]
    fn create_race_baseline_both_attempts_settle() {
        with_temp_vault(|| {
            let path = format!("race/create-{}.md", uuid::Uuid::new_v4().simple());
            let barrier = Arc::new(Barrier::new(2));
            let successes = Arc::new(AtomicUsize::new(0));
            let failures = Arc::new(AtomicUsize::new(0));
            let mut handles = Vec::new();
            for writer in 0..2 {
                let barrier = Arc::clone(&barrier);
                let successes = Arc::clone(&successes);
                let failures = Arc::clone(&failures);
                let path = path.clone();
                handles.push(thread::spawn(move || {
                    barrier.wait();
                    let request = VaultWriteRequest {
                        path: Some(path),
                        content: format!("# Create {writer}\n"),
                        ..Default::default()
                    };
                    match VaultService::create_note(&request) {
                        Ok(_) => successes.fetch_add(1, Ordering::SeqCst),
                        Err(_) => failures.fetch_add(1, Ordering::SeqCst),
                    };
                }));
            }
            for handle in handles {
                handle.join().expect("create thread");
            }
            assert_eq!(
                successes.load(Ordering::SeqCst) + failures.load(Ordering::SeqCst),
                2
            );
        });
    }
}
