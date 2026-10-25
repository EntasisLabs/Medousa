use super::*;
use std::sync::{Arc, Barrier};
mod job_reconcile;
mod visual_jobs;
fn create(store: &CreativeStore) -> CreativeDocument {
    store
        .create(CreativeCreateRequest {
            id: "project_one".into(),
            title: "Release graphic".into(),
            scene: CreativeScene::default(),
        })
        .unwrap()
}
fn text(id: &str) -> CreativeNode {
    CreativeNode {
        id: id.into(),
        name: None,
        appearance: None,
        design_source: None,
        text_layout: None,
        parent_id: None,
        transform: CreativeTransform {
            x: -100_000.,
            y: 80_000.,
            width: 300.,
            height: 100.,
            rotation: 0.,
        },
        opacity: 1.,
        visible: true,
        locked: false,
        content: CreativeContent::Text {
            text: "Hello".into(),
            color: "#123456".into(),
            font_family: "Inter".into(),
            font_size: 32.,
            font_weight: None,
            font_style: None,
        },
    }
}
fn edit(id: &str, revision: u64, node: CreativeNode) -> CreativeTransaction {
    CreativeTransaction {
        id: id.into(),
        expected_revision: revision,
        label: "Edit text".into(),
        operations: vec![CreativeOperation::PutNode { node: Box::new(node) }],
    }
}
fn history(id: &str, revision: u64, redo: bool) -> CreativeHistoryRequest {
    CreativeHistoryRequest {
        id: id.into(),
        expected_revision: revision,
        redo,
    }
}
#[test]
fn roundtrip_preserves_world_coordinates_and_history() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    store
        .transact("project_one", edit("first", 0, text("title")))
        .unwrap();
    drop(store);
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    let view = store.get("project_one").unwrap();
    assert_eq!(view.document.scene.nodes[0].transform.x, -100_000.);
    assert!(view.can_undo);
    let undone = store
        .history("project_one", history("undo", 1, false))
        .unwrap();
    assert!(undone.document.scene.nodes.is_empty());
    assert_eq!(undone.document.revision, 2);
    let redone = store
        .history("project_one", history("redo", 2, true))
        .unwrap();
    assert_eq!(redone.document.scene.nodes[0], text("title"));
    assert_eq!(redone.document.revision, 3);
}
#[test]
fn concurrent_stale_writers_never_overwrite_each_other() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap());
    create(&store);
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|i| {
            let store = store.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.transact(
                    "project_one",
                    edit(&format!("tx_{i}"), 0, text(&format!("n_{i}"))),
                )
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(CreativeError::Conflict(1))))
            .count(),
        1
    );
    assert_eq!(
        store.get("project_one").unwrap().document.scene.nodes.len(),
        1
    );
}
#[test]
fn retries_are_idempotent_but_changed_input_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    let tx = edit("once", 0, text("title"));
    store.transact("project_one", tx.clone()).unwrap();
    let retry = store.transact("project_one", tx.clone()).unwrap();
    assert!(retry.duplicate);
    assert_eq!(retry.document.revision, 1);
    let mut different = tx;
    different.label = "Different".into();
    assert!(store.transact("project_one", different).is_err());
}
#[test]
fn invalid_batch_is_atomic_and_does_not_consume_history() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    let mut tx = edit("bad", 0, text("title"));
    tx.operations.push(CreativeOperation::RemoveNode {
        id: "absent".into(),
    });
    assert!(store.transact("project_one", tx).is_err());
    let doc = store.get("project_one").unwrap();
    assert_eq!(doc.document.revision, 0);
    assert!(doc.document.scene.nodes.is_empty());
    assert!(!doc.can_undo);
}
#[test]
fn hierarchy_cycles_and_orphans_are_rejected() {
    let mut a = text("a");
    let mut b = text("b");
    a.content = CreativeContent::Group;
    b.content = CreativeContent::Group;
    a.parent_id = Some("b".into());
    b.parent_id = Some("a".into());
    let scene = CreativeScene {
        nodes: vec![a, b],
        ..Default::default()
    };
    assert!(validate_scene(&scene, &BTreeMap::new()).is_err());
    let mut scene = CreativeScene::default();
    let mut n = text("title");
    n.parent_id = Some("missing".into());
    scene.nodes.push(n);
    assert!(validate_scene(&scene, &BTreeMap::new()).is_err());
}
#[test]
fn external_urls_nonfinite_geometry_and_missing_assets_are_rejected() {
    let mut node = text("title");
    node.transform.x = f64::NAN;
    assert!(
        validate_scene(
            &CreativeScene {
                nodes: vec![node],
                ..Default::default()
            },
            &BTreeMap::new()
        )
        .is_err()
    );
    let mut node = text("image");
    node.content = CreativeContent::Image {
        asset_id: "https://evil.invalid/image".into(),
        crop: None,
    };
    assert!(
        validate_scene(
            &CreativeScene {
                nodes: vec![node],
                ..Default::default()
            },
            &BTreeMap::new()
        )
        .is_err()
    );
}
#[test]
fn imported_assets_are_project_owned_and_survive_undo() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    let bytes = b"\x89PNG\r\n\x1a\nfixture";
    let asset = store.import_asset("project_one", bytes).unwrap();
    assert_eq!(store.import_asset("project_one", bytes).unwrap(), asset);
    let doc = store.get("project_one").unwrap().document;
    assert_eq!(doc.revision, 1);
    let mut node = text("image");
    node.content = CreativeContent::Image {
        asset_id: asset.id.clone(),
        crop: None,
    };
    store
        .transact("project_one", edit("place", 1, node))
        .unwrap();
    store
        .history("project_one", history("undo", 2, false))
        .unwrap();
    drop(store);
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    assert_eq!(store.asset("project_one", &asset.id).unwrap().1, bytes);
    assert!(
        store
            .get("project_one")
            .unwrap()
            .document
            .scene
            .nodes
            .is_empty()
    );
}
#[test]
fn stale_proposal_cannot_overwrite_a_human_edit() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    store
        .transact("project_one", edit("initial", 0, text("title")))
        .unwrap();
    store
        .propose(
            "project_one",
            CreativeProposalRequest {
                id: "suggestion".into(),
                expected_revision: 1,
                selected_ids: vec!["title".into()],
                label: "AI edit".into(),
                operations: vec![CreativeOperation::PutNode { node: Box::new(text("title")) }],
            },
        )
        .unwrap();
    let mut human = text("title");
    if let CreativeContent::Text { text, .. } = &mut human.content {
        *text = "Human edit".into();
    }
    store
        .transact("project_one", edit("human", 1, human.clone()))
        .unwrap();
    assert!(matches!(
        store.decide(
            "project_one",
            "suggestion",
            CreativeProposalDecision {
                expected_revision: 2,
                accept: true
            }
        ),
        Err(CreativeError::Conflict(2))
    ));
    assert_eq!(
        store.get("project_one").unwrap().document.scene.nodes[0],
        human
    );
    assert_eq!(
        store.proposals("project_one").unwrap().proposals[0].status,
        CreativeProposalStatus::Ready
    );
}
#[test]
fn accepting_a_proposal_is_one_undoable_idempotent_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    store
        .transact("project_one", edit("initial", 0, text("title")))
        .unwrap();
    let mut replacement = text("title");
    replacement.transform.x = 42.;
    store
        .propose(
            "project_one",
            CreativeProposalRequest {
                id: "suggestion".into(),
                expected_revision: 1,
                selected_ids: vec!["title".into()],
                label: "AI align".into(),
                operations: vec![CreativeOperation::PutNode { node: Box::new(replacement.clone()) }],
            },
        )
        .unwrap();
    let decision = || CreativeProposalDecision {
        expected_revision: 1,
        accept: true,
    };
    let accepted = store
        .decide("project_one", "suggestion", decision())
        .unwrap();
    assert_eq!(accepted.document.scene.nodes[0], replacement);
    assert!(
        store
            .decide("project_one", "suggestion", decision())
            .unwrap()
            .duplicate
    );
    assert_eq!(store.get("project_one").unwrap().document.revision, 2);
    let undo = store
        .history("project_one", history("undo_ai", 2, false))
        .unwrap();
    assert_eq!(undo.document.scene.nodes[0], text("title"));
}
#[test]
fn future_versions_are_never_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    let path = dir.path().join("projects/project_one.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["document"]["scene"]["version"] = serde_json::json!(99);
    let bytes = serde_json::to_vec(&value).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        store.get("project_one"),
        Err(CreativeError::Unsupported)
    ));
    assert!(
        store
            .transact("project_one", edit("edit", 0, text("x")))
            .is_err()
    );
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}
#[test]
fn path_traversal_and_executable_assets_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    assert!(store.get("../outside").is_err());
    create(&store);
    assert!(
        store
            .import_asset("project_one", b"<svg onload='evil()'/>")
            .is_err()
    );
}
#[cfg(unix)]
#[test]
fn symlink_substitution_cannot_escape_store() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    let original = outside.path().join("precious.json");
    std::fs::write(&original, b"preserve").unwrap();
    std::fs::remove_file(dir.path().join("projects/project_one.json")).unwrap();
    std::os::unix::fs::symlink(&original, dir.path().join("projects/project_one.json")).unwrap();
    assert!(
        store
            .transact("project_one", edit("edit", 0, text("x")))
            .is_err()
    );
    assert_eq!(std::fs::read(&original).unwrap(), b"preserve");
}

#[test]
fn selection_context_is_bounded_and_does_not_mutate_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    store
        .transact("project_one", edit("put", 0, text("title")))
        .unwrap();
    let context = store
        .context(
            "project_one",
            CreativeContextRequest {
                selected_ids: vec!["title".into()],
            },
        )
        .unwrap();
    assert_eq!(context.revision, 1);
    assert_eq!(context.nodes, vec![text("title")]);
    assert_eq!(context.fingerprint.len(), 64);
    assert_eq!(store.get("project_one").unwrap().document.revision, 1);
    assert!(
        store
            .context(
                "project_one",
                CreativeContextRequest {
                    selected_ids: vec!["missing".into()]
                }
            )
            .is_err()
    );
}
#[test]
fn proposals_can_change_the_project_and_wait_for_acceptance() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    let mut original = text("title");
    original.locked = true;
    store
        .transact("project_one", edit("put", 0, original.clone()))
        .unwrap();
    let mut revised = original.clone();
    revised.transform.width = 500.;
    let request = CreativeProposalRequest {
        id: "outside".into(),
        expected_revision: 1,
        selected_ids: vec![],
        label: "Build editable design".into(),
        operations: vec![
            CreativeOperation::PutNode {
                node: Box::new(revised.clone()),
            },
            CreativeOperation::PutNode {
                node: Box::new(text("new-root")),
            },
            CreativeOperation::SetBackground {
                color: "#202020".into(),
            },
            CreativeOperation::Rename {
                title: "Changed".into(),
            },
            CreativeOperation::Pin { pinned: true },
            CreativeOperation::Reorder {
                ids: vec!["new-root".into(), "title".into()],
            },
        ],
    };
    store.propose("project_one", request).unwrap();
    let before = store.get("project_one").unwrap().document;
    assert_eq!(before.title, "Release graphic");
    assert_eq!(before.scene.nodes, vec![original]);
    let applied = store
        .decide(
            "project_one",
            "outside",
            CreativeProposalDecision {
                expected_revision: 1,
                accept: true,
            },
        )
        .unwrap();
    assert_eq!(applied.document.title, "Changed");
    assert_eq!(applied.document.scene.background, "#202020");
    assert!(applied.document.pinned);
    assert_eq!(applied.document.scene.nodes[0].id, "new-root");
    assert_eq!(applied.document.scene.nodes[1], revised);
    let undone = store
        .history("project_one", history("undo-proposal", 2, false))
        .unwrap();
    assert_eq!(undone.document.scene, before.scene);
    assert_eq!(undone.document.title, before.title);
    assert_eq!(undone.document.pinned, before.pinned);
}
#[test]
fn asset_publication_can_resume_after_manifest_failure() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let store = CreativeStore::open(&root).unwrap();
    create(&store);
    let bytes = b"\x89PNG\r\n\x1a\nfixture";
    let asset = store.import_asset("project_one", bytes).unwrap();
    let manifest = root.join("projects/project_one.json");
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    record["document"]["assets"] = serde_json::json!({});
    std::fs::write(&manifest, serde_json::to_vec(&record).unwrap()).unwrap();
    assert_eq!(store.import_asset("project_one", bytes).unwrap(), asset);
    assert_eq!(store.asset("project_one", &asset.id).unwrap().1, bytes);
}
#[test]
fn pagination_does_not_repeat_or_omit_live_projects_around_tombstones() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    for id in ["a", "b", "c", "d"] {
        store
            .create(CreativeCreateRequest {
                id: id.into(),
                title: id.into(),
                scene: CreativeScene::default(),
            })
            .unwrap();
    }
    store.delete("b", 0).unwrap();
    let first = store
        .list(CreativeListQuery {
            cursor: None,
            limit: Some(2),
        })
        .unwrap();
    assert_eq!(
        first
            .projects
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "c"]
    );
    let second = store
        .list(CreativeListQuery {
            cursor: first.next_cursor,
            limit: Some(2),
        })
        .unwrap();
    assert_eq!(second.projects[0].id, "d");
    assert!(second.next_cursor.is_none());
}

#[test]
fn shared_legacy_migration_fixture_roundtrips_through_backend_storage() {
    let scene: CreativeScene = serde_json::from_str(include_str!(
        "../../../tests/fixtures/creative/v3-from-legacy.json"
    ))
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let store = CreativeStore::open(&root).unwrap();
    store
        .create(CreativeCreateRequest {
            id: "migrated".into(),
            title: "Original ink".into(),
            scene: scene.clone(),
        })
        .unwrap();
    drop(store);
    let reopened = CreativeStore::open(&root)
        .unwrap()
        .get("migrated")
        .unwrap()
        .document;
    assert_eq!(reopened.scene, scene);
}
#[test]
fn delete_retries_preserve_the_tombstone_and_assets() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    store.delete("project_one", 0).unwrap();
    store.delete("project_one", 0).unwrap();
    assert!(matches!(
        store.get("project_one"),
        Err(CreativeError::NotFound)
    ));
}
#[test]
fn frame_context_captures_descendants_and_locks_do_not_block_proposals() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    let mut frame = text("frame");
    frame.content = CreativeContent::Frame {
        name: "Frame".into(),
        background: "transparent".into(),
    };
    frame.locked = true;
    let mut child = text("title");
    child.parent_id = Some("frame".into());
    store
        .transact(
            "project_one",
            CreativeTransaction {
                id: "scene".into(),
                expected_revision: 0,
                label: "Scene".into(),
                operations: vec![
                    CreativeOperation::PutNode { node: Box::new(frame) },
                    CreativeOperation::PutNode {
                        node: Box::new(child.clone()),
                    },
                ],
            },
        )
        .unwrap();
    let context = store
        .context(
            "project_one",
            CreativeContextRequest {
                selected_ids: vec!["frame".into()],
            },
        )
        .unwrap();
    assert_eq!(context.nodes.len(), 2);
    let request = CreativeProposalRequest {
        id: "locked".into(),
        expected_revision: 1,
        selected_ids: context.selected_ids,
        label: "Change".into(),
        operations: vec![CreativeOperation::PutNode { node: Box::new(child) }],
    };
    store.propose("project_one", request).unwrap();
    let applied = store
        .decide(
            "project_one",
            "locked",
            CreativeProposalDecision {
                expected_revision: 1,
                accept: true,
            },
        )
        .unwrap();
    assert!(
        applied
            .document
            .scene
            .nodes
            .iter()
            .find(|n| n.id == "frame")
            .unwrap()
            .locked
    );
}

#[test]
fn cancelled_proposal_retries_do_not_resurrect_after_terminal_eviction() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    store
        .transact("project_one", edit("put", 0, text("title")))
        .unwrap();
    let original = CreativeProposalRequest {
        id: "first".into(),
        expected_revision: 1,
        selected_ids: vec!["title".into()],
        label: "Original".into(),
        operations: vec![CreativeOperation::PutNode { node: Box::new(text("title")) }],
    };
    store.propose("project_one", original.clone()).unwrap();
    store
        .decide(
            "project_one",
            "first",
            CreativeProposalDecision {
                expected_revision: 1,
                accept: false,
            },
        )
        .unwrap();
    assert!(
        store
            .decide(
                "project_one",
                "first",
                CreativeProposalDecision {
                    expected_revision: 1,
                    accept: false
                }
            )
            .unwrap()
            .duplicate
    );
    for i in 0..33 {
        let revision = store.get("project_one").unwrap().document.revision;
        let mut request = original.clone();
        request.id = format!("p_{i}");
        request.expected_revision = revision;
        store.propose("project_one", request.clone()).unwrap();
        store
            .decide(
                "project_one",
                &request.id,
                CreativeProposalDecision {
                    expected_revision: revision,
                    accept: false,
                },
            )
            .unwrap();
    }
    assert!(matches!(
        store.propose("project_one", original),
        Err(CreativeError::Conflict(_))
    ));
}

fn job_request(id: &str) -> CreativeJobRequest {
    CreativeJobRequest {
        id: id.into(),
        expected_revision: 1,
        selected_ids: vec!["title".into()],
        prompt: "Make the title clearer".into(),
        ..Default::default()
    }
}
fn job_result(id: &str) -> CreativeProposalRequest {
    let mut node = text("title");
    if let CreativeContent::Text { text, .. } = &mut node.content {
        *text = "A clearer title".into();
    }
    CreativeProposalRequest {
        id: id.into(),
        expected_revision: 1,
        selected_ids: vec!["title".into()],
        label: "Clarify title".into(),
        operations: vec![CreativeOperation::PutNode { node: Box::new(node) }],
    }
}
fn job_store() -> (tempfile::TempDir, Arc<CreativeStore>) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap());
    create(&store);
    store
        .transact("project_one", edit("seed", 0, text("title")))
        .unwrap();
    (dir, store)
}
fn decision(accept: bool, revision: u64) -> CreativeProposalDecision {
    CreativeProposalDecision {
        accept,
        expected_revision: revision,
    }
}

#[test]
fn creative_job_admission_retries_keep_the_original_snapshot_after_restart() {
    let (dir, store) = job_store();
    let job = store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    assert_eq!(job.status, CreativeJobStatus::Queued);
    store
        .transact("project_one", edit("human", 1, text("second")))
        .unwrap();
    drop(store);
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    let duplicate = store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    assert_eq!(duplicate.turn_id, job.turn_id);
    assert_eq!(duplicate.context.nodes.len(), 1);
    assert_eq!(duplicate.context.revision, 1);
    let mut changed = job_request("first");
    changed.prompt.push('!');
    assert!(store.enqueue_job("project_one", "owner", changed).is_err());
    assert!(
        store
            .enqueue_job("project_one", "other", job_request("first"))
            .is_err()
    );
    assert!(store.jobs("project_one", "other").unwrap().jobs.is_empty());
    assert!(store.job("project_one", "first", "other").is_err());
}
#[test]
fn creative_job_claim_is_atomic_across_store_instances() {
    let (dir, store) = job_store();
    store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    let other = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let a = barrier.clone();
    let task = std::thread::spawn(move || {
        a.wait();
        store.claim_job("project_one", "first").unwrap()
    });
    barrier.wait();
    let second = other.claim_job("project_one", "first").unwrap();
    assert_eq!(
        usize::from(second.is_some()) + usize::from(task.join().unwrap().is_some()),
        1
    );
}
#[test]
fn creative_job_recovery_distinguishes_queued_from_ambiguous_dispatch() {
    let (dir, store) = job_store();
    for id in ["queued", "claimed", "running"] {
        store
            .enqueue_job("project_one", "owner", job_request(id))
            .unwrap();
    }
    store.claim_job("project_one", "claimed").unwrap();
    store.claim_job("project_one", "running").unwrap();
    store.mark_job_running("project_one", "running").unwrap();
    drop(store);
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    let (page, _, _) = store.job_dispatch_page(None).unwrap();
    assert_eq!(page.len(), 3);
    for id in ["claimed", "running"] {
        store
            .observe_job_turn("project_one", id, "interrupted")
            .unwrap();
        assert!(store.claim_job("project_one", id).unwrap().is_none());
        let job = store.job("project_one", id, "owner").unwrap();
        assert_eq!(job.status, CreativeJobStatus::Interrupted);
        assert!(
            store
                .complete_job("project_one", id, &job.turn_id, job_result(id))
                .is_err()
        );
    }
    assert!(store.claim_job("project_one", "queued").unwrap().is_some());
}
#[test]
fn creative_job_accept_is_one_atomic_undoable_transaction_and_never_replays() {
    let (dir, store) = job_store();
    let job = store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    store.claim_job("project_one", "first").unwrap();
    let ready = store
        .complete_job("project_one", "first", &job.turn_id, job_result("first"))
        .unwrap();
    assert_eq!(ready.status, CreativeJobStatus::Ready);
    store.mark_job_running("project_one", "first").unwrap(); // completion beat spawn acknowledgement
    store
        .observe_job_turn("project_one", "first", "error")
        .unwrap(); // late error cannot discard output
    assert_eq!(store.get("project_one").unwrap().document.revision, 1);
    let accepted = store
        .decide_job("project_one", "first", "owner", decision(true, 1))
        .unwrap();
    assert_eq!(accepted.applied_revision, Some(2));
    store
        .history("project_one", history("undo_job", 2, false))
        .unwrap();
    let original = store
        .get("project_one")
        .unwrap()
        .document
        .scene
        .nodes
        .clone();
    drop(store);
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    assert_eq!(
        store
            .decide_job("project_one", "first", "owner", decision(true, 1))
            .unwrap()
            .applied_revision,
        Some(2)
    );
    assert_eq!(
        store
            .complete_job("project_one", "first", &job.turn_id, job_result("first"))
            .unwrap()
            .status,
        CreativeJobStatus::Accepted
    );
    let mut different = job_result("first");
    different.label.push('!');
    assert!(
        store
            .complete_job("project_one", "first", &job.turn_id, different)
            .is_err()
    );
    assert_eq!(
        store.get("project_one").unwrap().document.scene.nodes,
        original
    );
    assert_eq!(store.get("project_one").unwrap().document.revision, 3);
}
#[test]
fn creative_job_stale_completion_keeps_human_movement_when_applying_content() {
    let (_dir, store) = job_store();
    let job = store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    store.claim_job("project_one", "first").unwrap();
    let mut human = text("title");
    human.transform.x = 200.;
    store
        .transact("project_one", edit("human", 1, human.clone()))
        .unwrap();
    assert_eq!(
        store
            .job_context("project_one", "first", &job.turn_id)
            .unwrap()
            .nodes[0]
            .transform
            .x,
        -100_000.
    );
    let result = store
        .complete_job("project_one", "first", &job.turn_id, job_result("first"))
        .unwrap();
    assert_eq!(result.status, CreativeJobStatus::Stale);
    assert!(result.result.is_some());
    assert!(
        store
            .decide_job("project_one", "first", "owner", decision(true, 2))
            .is_ok()
    );
    if let CreativeContent::Text { text, .. } = &mut human.content {
        *text = "A clearer title".into();
    }
    assert_eq!(
        store.get("project_one").unwrap().document.scene.nodes[0],
        human
    );
}
#[test]
fn creative_job_ready_output_becomes_stale_after_human_undo_or_edit() {
    let (_dir, store) = job_store();
    let job = store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    store.claim_job("project_one", "first").unwrap();
    store
        .complete_job("project_one", "first", &job.turn_id, job_result("first"))
        .unwrap();
    store
        .history("project_one", history("human_undo", 1, false))
        .unwrap();
    assert_eq!(
        store.job("project_one", "first", "owner").unwrap().status,
        CreativeJobStatus::Stale
    );
    store
        .history("project_one", history("human_redo", 2, true))
        .unwrap();
    assert!(
        store
            .decide_job("project_one", "first", "owner", decision(true, 3))
            .is_ok()
    );
}
#[test]
fn creative_job_cancel_fences_late_results_and_runtime_acknowledgements() {
    let (_dir, store) = job_store();
    let job = store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    store.claim_job("project_one", "first").unwrap();
    let cancelled = store
        .decide_job("project_one", "first", "owner", decision(false, 0))
        .unwrap();
    assert_eq!(cancelled.status, CreativeJobStatus::Cancelled);
    store.mark_job_running("project_one", "first").unwrap();
    store
        .reject_job_dispatch("project_one", "first", true, "late rejection".into())
        .unwrap();
    store
        .observe_job_turn("project_one", "first", "done")
        .unwrap();
    assert!(
        store
            .complete_job("project_one", "first", &job.turn_id, job_result("first"))
            .is_err()
    );
    assert!(store.claim_job("project_one", "first").unwrap().is_none());
    assert_eq!(
        store
            .enqueue_job("project_one", "owner", job_request("first"))
            .unwrap()
            .status,
        CreativeJobStatus::Cancelled
    );
    assert_eq!(store.get("project_one").unwrap().document.revision, 1);
}
#[test]
fn creative_job_cancel_racing_completion_always_wins_before_acceptance() {
    let (_dir, store) = job_store();
    let job = store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    store.claim_job("project_one", "first").unwrap();
    let other = store.clone();
    let task = std::thread::spawn(move || {
        other.complete_job("project_one", "first", &job.turn_id, job_result("first"))
    });
    store
        .decide_job("project_one", "first", "owner", decision(false, 1))
        .unwrap();
    let _ = task.join().unwrap();
    assert_eq!(
        store.job("project_one", "first", "owner").unwrap().status,
        CreativeJobStatus::Cancelled
    );
    assert!(
        store
            .decide_job("project_one", "first", "owner", decision(true, 1))
            .is_err()
    );
    assert_eq!(store.get("project_one").unwrap().document.revision, 1);
}
#[test]
fn creative_job_outputs_are_bound_to_execution_and_captured_selection() {
    let (_dir, store) = job_store();
    let job = store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    assert!(
        store
            .complete_job("project_one", "first", &job.turn_id, job_result("first"))
            .is_err()
    );
    store.claim_job("project_one", "first").unwrap();
    assert!(
        store
            .complete_job("project_one", "first", "wrong_turn", job_result("first"))
            .is_err()
    );
    for field in ["id", "revision", "selection", "operation"] {
        let mut result = job_result("first");
        match field {
            "id" => result.id = "other".into(),
            "revision" => result.expected_revision = 2,
            "selection" => result.selected_ids = vec!["other".into()],
            _ => {
                result.operations = vec![CreativeOperation::Reorder {
                    ids: vec!["missing_node".into()],
                }]
            }
        }
        assert!(
            store
                .complete_job("project_one", "first", &job.turn_id, result)
                .is_err()
        );
    }
    assert_eq!(
        store.job("project_one", "first", "owner").unwrap().status,
        CreativeJobStatus::Dispatching
    );
}
#[test]
fn creative_job_stages_images_without_invalidating_its_own_base() {
    let (_dir, store) = job_store();
    let job = store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    store.claim_job("project_one", "first").unwrap();
    let png = b"\x89PNG\r\n\x1a\nimage payload";
    let asset = store
        .import_job_asset("project_one", "first", &job.turn_id, png)
        .unwrap();
    assert_eq!(
        store
            .import_job_asset("project_one", "first", &job.turn_id, png)
            .unwrap(),
        asset
    );
    let before = store.get("project_one").unwrap().document;
    assert_eq!(before.revision, 1);
    assert!(before.assets.is_empty());
    assert_eq!(store.asset("project_one", &asset.id).unwrap().1, png);
    let mut result = job_result("first");
    if let CreativeOperation::PutNode { node } = &mut result.operations[0] {
        node.content = CreativeContent::Image {
            asset_id: asset.id.clone(),
            crop: None,
        };
    }
    assert_eq!(
        store
            .complete_job("project_one", "first", &job.turn_id, result)
            .unwrap()
            .status,
        CreativeJobStatus::Ready
    );
    store
        .decide_job("project_one", "first", "owner", decision(true, 1))
        .unwrap();
    assert!(
        store
            .get("project_one")
            .unwrap()
            .document
            .assets
            .contains_key(&asset.id)
    );
    store
        .history("project_one", history("undo_image", 2, false))
        .unwrap();
    assert!(
        store
            .get("project_one")
            .unwrap()
            .document
            .assets
            .contains_key(&asset.id)
    );
    assert_eq!(store.asset("project_one", &asset.id).unwrap().1, png);
}
#[test]
fn creative_job_no_output_and_deleted_projects_are_terminal() {
    let (_dir, store) = job_store();
    let job = store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    store.claim_job("project_one", "first").unwrap();
    store
        .observe_job_turn("project_one", "first", "done")
        .unwrap();
    assert_eq!(
        store.job("project_one", "first", "owner").unwrap().status,
        CreativeJobStatus::Failed
    );
    assert!(
        store
            .complete_job("project_one", "first", &job.turn_id, job_result("first"))
            .is_err()
    );
    let second = store
        .enqueue_job("project_one", "owner", job_request("second"))
        .unwrap();
    store.claim_job("project_one", "second").unwrap();
    store.delete("project_one", 1).unwrap();
    assert_eq!(
        store.job("project_one", "second", "owner").unwrap().status,
        CreativeJobStatus::Cancelled
    );
    assert!(
        store
            .complete_job(
                "project_one",
                "second",
                &second.turn_id,
                job_result("second")
            )
            .is_err()
    );
    assert!(
        store
            .job_dispatch_page(None)
            .unwrap()
            .0
            .iter()
            .any(|j| j.job_id == "second" && j.status == CreativeJobStatus::Cancelled)
    );
}
#[test]
fn creative_job_limits_never_evict_an_admission_identity() {
    let (_dir, store) = job_store();
    for n in 0..32 {
        let id = format!("job_{n}");
        store
            .enqueue_job("project_one", "owner", job_request(&id))
            .unwrap();
        store
            .decide_job("project_one", &id, "owner", decision(false, 1))
            .unwrap();
    }
    assert!(
        store
            .enqueue_job("project_one", "owner", job_request("overflow"))
            .is_ok()
    );
    assert_eq!(
        store
            .enqueue_job("project_one", "owner", job_request("job_0"))
            .unwrap()
            .status,
        CreativeJobStatus::Cancelled
    );
}
#[test]
fn creative_job_only_proven_dispatch_rejection_can_requeue() {
    let (_dir, store) = job_store();
    store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    store.claim_job("project_one", "first").unwrap();
    store
        .reject_job_dispatch("project_one", "first", true, "capacity".into())
        .unwrap();
    assert!(store.claim_job("project_one", "first").unwrap().is_some());
    store.mark_job_running("project_one", "first").unwrap();
    store
        .reject_job_dispatch("project_one", "first", true, "late rejection".into())
        .unwrap();
    assert_eq!(
        store.job("project_one", "first", "owner").unwrap().status,
        CreativeJobStatus::Running
    );
}

#[test]
fn creative_job_locator_recovers_missing_publication_and_rejects_orphan_authority() {
    let (dir, store) = job_store();
    let job = store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::remove_file(root.join("jobs").join(format!("{}.json", job.turn_id))).unwrap();
    assert!(store.job_for_turn(&job.turn_id).is_err());
    store.job_dispatch_page(None).unwrap();
    assert_eq!(store.job_for_turn(&job.turn_id).unwrap().job_id, "first");
    std::fs::write(
        root.join("jobs/creative_job_orphan.json"),
        b"[\"project_one\",\"missing\"]",
    )
    .unwrap();
    assert!(store.job_for_turn("creative_job_orphan").is_err());
    std::fs::write(
        root.join("jobs/creative_job_wrong.json"),
        b"[\"project_one\",\"first\"]",
    )
    .unwrap();
    assert!(store.job_for_turn("creative_job_wrong").is_err());
}
#[test]
fn creative_job_rejects_oversized_result_without_losing_the_job() {
    let (_dir, store) = job_store();
    let job = store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    store.claim_job("project_one", "first").unwrap();
    let mut result = job_result("first");
    let mut node = text("title");
    if let CreativeContent::Text { text, .. } = &mut node.content {
        *text = "a".repeat(100_000);
    }
    result.operations = vec![CreativeOperation::PutNode { node: Box::new(node) }; 6];
    assert!(
        store
            .complete_job("project_one", "first", &job.turn_id, result)
            .is_err()
    );
    assert_eq!(store.get("project_one").unwrap().document.revision, 1);
    assert_eq!(
        store
            .complete_job("project_one", "first", &job.turn_id, job_result("first"))
            .unwrap()
            .status,
        CreativeJobStatus::Ready
    );
}
#[test]
fn creative_job_snapshot_budget_does_not_corrupt_large_documents() {
    let (_dir, store) = job_store();
    let mut operations = vec![];
    for n in 0..24 {
        let mut node = text(&format!("large_{n}"));
        if let CreativeContent::Text { text, .. } = &mut node.content {
            *text = "a".repeat(100_000);
        }
        operations.push(CreativeOperation::PutNode { node: Box::new(node) });
    }
    store
        .transact(
            "project_one",
            CreativeTransaction {
                id: "large_edit".into(),
                expected_revision: 1,
                label: "Large scene".into(),
                operations,
            },
        )
        .unwrap();
    let mut request = job_request("oversized");
    request.expected_revision = 2;
    assert!(store.enqueue_job("project_one", "owner", request).is_err());
    assert_eq!(
        store.get("project_one").unwrap().document.scene.nodes.len(),
        25
    );
    assert!(store.jobs("project_one", "owner").unwrap().jobs.is_empty());
}

#[test]
fn creative_job_outbox_cleans_orphans_and_does_not_scan_idle_or_damaged_projects() {
    let (dir, store) = job_store();
    let job = store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::write(root.join("projects/idle.json"), b"damaged idle project").unwrap();
    std::fs::write(
        root.join("projects/damaged.json"),
        b"damaged active project",
    )
    .unwrap();
    std::fs::write(
        root.join("outbox/creative_job_damaged.json"),
        b"[\"damaged\",\"first\"]",
    )
    .unwrap();
    std::fs::write(
        root.join("outbox/creative_job_orphan.json"),
        b"[\"missing\",\"first\"]",
    )
    .unwrap();
    let (dispatches, _, errors) = store.job_dispatch_page(None).unwrap();
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0].turn_id, job.turn_id);
    assert_eq!(errors.len(), 1);
    assert!(!root.join("outbox/creative_job_orphan.json").exists());
    store.claim_job("project_one", "first").unwrap();
    store
        .complete_job("project_one", "first", &job.turn_id, job_result("first"))
        .unwrap();
    assert!(store.job_dispatch_page(None).unwrap().0.is_empty());
    assert!(
        !root
            .join("outbox")
            .join(format!("{}.json", job.turn_id))
            .exists()
    );
    assert!(store.job_for_turn(&job.turn_id).is_ok()); // callback locator survives terminal cleanup
}

#[test]
fn creative_job_can_propose_reparenting_into_another_container() {
    let (_dir, store) = job_store();
    let mut outside = text("outside");
    outside.content = CreativeContent::Frame {
        name: "Outside".into(),
        background: "#fff".into(),
    };
    store
        .transact("project_one", edit("outside", 1, outside.clone()))
        .unwrap();
    let mut request = job_request("first");
    request.expected_revision = 2;
    let job = store.enqueue_job("project_one", "owner", request).unwrap();
    store.claim_job("project_one", "first").unwrap();
    let mut result = job_result("first");
    result.expected_revision = 2;
    if let CreativeOperation::PutNode { node } = &mut result.operations[0] {
        node.parent_id = Some("outside".into());
    }
    store
        .complete_job("project_one", "first", &job.turn_id, result)
        .unwrap();
    outside.locked = true;
    store
        .transact("project_one", edit("lock_outside", 2, outside))
        .unwrap();
    let mut request = job_request("locked");
    request.expected_revision = 3;
    request.selected_ids.push("outside".into());
    let job = store
        .enqueue_job("project_one", "owner", request.clone())
        .unwrap();
    store.claim_job("project_one", "locked").unwrap();
    let mut result = job_result("locked");
    result.expected_revision = 3;
    result.selected_ids = request.selected_ids;
    if let CreativeOperation::PutNode { node } = &mut result.operations[0] {
        node.parent_id = Some("outside".into());
    }
    store
        .complete_job("project_one", "locked", &job.turn_id, result)
        .unwrap();
    assert_eq!(
        store.get("project_one").unwrap().document.scene.nodes[0].parent_id,
        None
    );
    store
        .decide_job(
            "project_one",
            "locked",
            "owner",
            CreativeProposalDecision {
                expected_revision: 3,
                accept: true,
            },
        )
        .unwrap();
    assert_eq!(
        store.get("project_one").unwrap().document.scene.nodes[0]
            .parent_id
            .as_deref(),
        Some("outside")
    );
}

#[test]
fn creative_job_keeps_runtime_failure_and_does_not_overwrite_ready_output() {
    let (_dir, store) = job_store();
    store
        .enqueue_job("project_one", "owner", job_request("failure"))
        .unwrap();
    store.claim_job("project_one", "failure").unwrap();
    let message =
        "Reconnect ChatGPT in Settings → Medousa Agent → Models, then submit a new request.";
    store
        .observe_job_failure("project_one", "failure", message)
        .unwrap();
    store
        .observe_job_turn("project_one", "failure", "done")
        .unwrap();
    let job = store.job("project_one", "failure", "owner").unwrap();
    assert_eq!(job.status, CreativeJobStatus::Failed);
    assert_eq!(job.error.as_deref(), Some(message));
    assert!(store.claim_job("project_one", "failure").unwrap().is_none());
    let next = store
        .enqueue_job("project_one", "owner", job_request("ready"))
        .unwrap();
    store.claim_job("project_one", "ready").unwrap();
    store
        .complete_job("project_one", "ready", &next.turn_id, job_result("ready"))
        .unwrap();
    store
        .observe_job_failure("project_one", "ready", message)
        .unwrap();
    let ready = store.job("project_one", "ready", "owner").unwrap();
    assert_eq!(ready.status, CreativeJobStatus::Ready);
    assert!(ready.error.is_none());
}

#[test]
fn deleting_a_duplicated_frame_removes_its_subtree_atomically_and_undo_restores_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    let mut frame = text("original");
    frame.content = CreativeContent::Frame {
        name: "Original".into(),
        background: "#fff".into(),
    };
    let mut group = text("original_group");
    group.parent_id = Some(frame.id.clone());
    group.content = CreativeContent::Group;
    let mut child = text("original_text");
    child.parent_id = Some(group.id.clone());
    let mut duplicate = frame.clone();
    duplicate.id = "duplicate".into();
    let mut duplicate_group = group.clone();
    duplicate_group.id = "duplicate_group".into();
    duplicate_group.parent_id = Some(duplicate.id.clone());
    let mut duplicate_child = child.clone();
    duplicate_child.id = "duplicate_text".into();
    duplicate_child.parent_id = Some(duplicate_group.id.clone());
    // Children may precede their parent in persisted order.
    let nodes = vec![
        duplicate_child,
        group,
        child,
        duplicate_group,
        frame,
        duplicate,
    ];
    let original = store
        .create(CreativeCreateRequest {
            id: "project_one".into(),
            title: "Duplicated frames".into(),
            scene: CreativeScene {
                nodes,
                ..Default::default()
            },
        })
        .unwrap();
    let deletion = CreativeTransaction {
        id: "delete_frame".into(),
        expected_revision: 0,
        label: "Delete frame".into(),
        operations: vec![CreativeOperation::RemoveNode {
            id: "duplicate".into(),
        }],
    };
    let deleted = store.transact("project_one", deletion.clone()).unwrap();
    assert_eq!(deleted.document.scene.nodes.len(), 3);
    assert!(
        deleted
            .document
            .scene
            .nodes
            .iter()
            .all(|n| n.id.starts_with("original"))
    );
    assert!(store.transact("project_one", deletion).unwrap().duplicate);
    let restored = store
        .history("project_one", history("restore", 1, false))
        .unwrap();
    assert_eq!(restored.document.scene, original.scene);
    let redone = store
        .history("project_one", history("remove_again", 2, true))
        .unwrap();
    assert_eq!(redone.document.scene, deleted.document.scene);
}

#[test]
fn subtree_removal_accepts_explicit_children_but_rejects_unknown_targets_atomically() {
    let mut frame = text("frame");
    frame.content = CreativeContent::Group;
    let mut child = text("child");
    child.parent_id = Some("frame".into());
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    let document = store
        .create(CreativeCreateRequest {
            id: "project_one".into(),
            title: "Nested".into(),
            scene: CreativeScene {
                nodes: vec![frame, child],
                ..Default::default()
            },
        })
        .unwrap();
    let parent = CreativeOperation::RemoveNode { id: "frame".into() };
    let child = CreativeOperation::RemoveNode { id: "child".into() };
    assert!(
        apply_operations(&document, &[parent.clone(), child.clone()])
            .unwrap()
            .scene
            .nodes
            .is_empty()
    );
    assert!(
        apply_operations(&document, &[child, parent.clone()])
            .unwrap()
            .scene
            .nodes
            .is_empty()
    );
    assert!(apply_operations(&document, &[parent.clone(), parent.clone()]).is_err());
    let bad = CreativeTransaction {
        id: "bad_delete".into(),
        expected_revision: 0,
        label: "Delete".into(),
        operations: vec![
            parent,
            CreativeOperation::RemoveNode {
                id: "unknown".into(),
            },
        ],
    };
    assert!(store.transact("project_one", bad).is_err());
    assert_eq!(store.get("project_one").unwrap().document, document);
    assert!(!store.get("project_one").unwrap().can_undo);
}

#[test]
fn optional_layer_labels_roundtrip_with_undo_and_legacy_nodes_remain_unchanged() {
    let mut node = text("title");
    let legacy = serde_json::to_value(&node).unwrap();
    assert!(legacy.get("name").is_none());
    assert!(
        serde_json::from_value::<CreativeNode>(legacy)
            .unwrap()
            .name
            .is_none()
    );
    node.name = Some("Caption".into());
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    let first = store
        .transact("project_one", edit("label", 0, node.clone()))
        .unwrap();
    assert_eq!(
        first.document.scene.nodes[0].name.as_deref(),
        Some("Caption")
    );
    let mut renamed = node.clone();
    renamed.name = Some("Smiley".into());
    store
        .transact("project_one", edit("rename", 1, renamed))
        .unwrap();
    let restored = store
        .history("project_one", history("undo_name", 2, false))
        .unwrap();
    assert_eq!(restored.document.scene.nodes[0], node);
    let mut bad = node.clone();
    bad.name = Some(" ".into());
    assert!(
        store
            .transact("project_one", edit("bad_name", 3, bad))
            .is_err()
    );
    node.name = Some("x".repeat(257));
    assert!(
        store
            .transact("project_one", edit("long_name", 3, node))
            .is_err()
    );
    assert_eq!(store.get("project_one").unwrap().document.revision, 3);
}

#[test]
fn text_emphasis_roundtrips_with_undo_and_rejects_invalid_weights() {
    let mut node = text("title");
    let legacy = serde_json::to_value(&node).unwrap();
    assert!(legacy["content"].get("font_style").is_none());
    assert!(legacy["content"].get("font_weight").is_none());
    assert_eq!(
        serde_json::from_value::<CreativeNode>(legacy).unwrap(),
        node
    );
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    store
        .transact("project_one", edit("original", 0, node.clone()))
        .unwrap();
    if let CreativeContent::Text {
        font_weight,
        font_style,
        ..
    } = &mut node.content
    {
        *font_weight = Some(700);
        *font_style = Some(CreativeFontStyle::Italic);
    }
    let styled = store
        .transact("project_one", edit("emphasis", 1, node.clone()))
        .unwrap();
    assert_eq!(styled.document.scene.nodes[0], node);
    let restored = store
        .history("project_one", history("undo_emphasis", 2, false))
        .unwrap();
    assert_eq!(restored.document.scene.nodes[0], text("title"));
    if let CreativeContent::Text { font_weight, .. } = &mut node.content {
        *font_weight = Some(999);
    }
    assert!(
        store
            .transact("project_one", edit("bad_weight", 3, node))
            .is_err()
    );
    assert_eq!(
        store.get("project_one").unwrap().document,
        restored.document
    );
}

#[test]
fn line_geometry_and_rotation_roundtrip_atomically_with_undo() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    let mut node = text("arrow");
    node.content = CreativeContent::Shape {
        shape: CreativeShapeKind::Line,
        fill: "transparent".into(),
        stroke: "#123456".into(),
        stroke_width: 6.,
        line: Some(CreativeLine {
            start: CreativeLinePoint { x: 200., y: 0. },
            end: CreativeLinePoint { x: 0., y: 100. },
            start_arrow: false,
            end_arrow: true,
            dashed: true,
        }),
    };
    node.transform.rotation = 45.;
    store
        .transact("project_one", edit("line", 0, node.clone()))
        .unwrap();
    let mut moved = node.clone();
    moved.transform.rotation = 90.;
    if let CreativeContent::Shape {
        line: Some(line), ..
    } = &mut moved.content
    {
        line.end.y = 200.;
    }
    let result = store
        .transact("project_one", edit("move_endpoint", 1, moved.clone()))
        .unwrap();
    assert_eq!(result.document.scene.nodes[0], moved);
    let restored = store
        .history("project_one", history("undo_endpoint", 2, false))
        .unwrap();
    assert_eq!(restored.document.scene.nodes[0], node);
    if let CreativeContent::Shape { shape, .. } = &mut moved.content {
        *shape = CreativeShapeKind::Rectangle;
    }
    assert!(
        store
            .transact("project_one", edit("wrong_shape", 3, moved.clone()))
            .is_err()
    );
    if let CreativeContent::Shape {
        shape,
        line: Some(line),
        ..
    } = &mut moved.content
    {
        *shape = CreativeShapeKind::Line;
        line.end.x = f64::NAN;
    }
    assert!(
        store
            .transact("project_one", edit("bad_point", 3, moved))
            .is_err()
    );
    assert_eq!(
        store.get("project_one").unwrap().document,
        restored.document
    );
    let mut legacy = serde_json::to_value(&node).unwrap();
    legacy["content"].as_object_mut().unwrap().remove("line");
    let parsed = serde_json::from_value::<CreativeNode>(legacy).unwrap();
    assert!(
        serde_json::to_value(parsed).unwrap()["content"]
            .get("line")
            .is_none()
    );
}

#[test]
fn surface_and_text_layout_are_durable_undoable_and_legacy_optional() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().canonicalize().unwrap();
    let store = CreativeStore::open(&path).unwrap();
    create(&store);
    let mut node = text("label");
    let legacy = serde_json::to_value(&node).unwrap();
    assert!(legacy.get("appearance").is_none());
    assert!(legacy.get("text_layout").is_none());
    node.text_layout = Some(CreativeTextLayout {
        letter_spacing: None, paragraph_spacing: None, underline: None, spans: vec![],
        width: Some(240.),
        align: CreativeTextAlign::Center,
        line_height: 1.5,
    });
    store
        .transact("project_one", edit("wrap", 0, node.clone()))
        .unwrap();
    let mut card = text("card");
    card.content = CreativeContent::Shape {
        shape: CreativeShapeKind::Rectangle,
        fill: "#fff".into(),
        stroke: "#123456".into(),
        stroke_width: 2.,
        line: None,
    };
    card.appearance = Some(CreativeAppearance {
        corner_radius: 16.,
        shadow: Some(CreativeShadow {
            x: 0.,
            y: 6.,
            blur: 12.,
            color: "#000".into(),
            opacity: 0.18,
        }),
    });
    let applied = store
        .transact("project_one", edit("surface", 1, card.clone()))
        .unwrap();
    drop(store);
    let store = CreativeStore::open(&path).unwrap();
    assert_eq!(store.get("project_one").unwrap().document, applied.document);
    let undo = store
        .history("project_one", history("undo_surface", 2, false))
        .unwrap();
    assert_eq!(undo.document.scene.nodes, vec![node]);
    let redo = store
        .history("project_one", history("redo_surface", 3, true))
        .unwrap();
    assert_eq!(redo.document.scene.nodes[1], card);
}

#[test]
fn invalid_surface_and_text_layout_fail_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    let before = create(&store);
    for (i, (width, leading)) in [
        (Some(0.), 1.3),
        (Some(f64::INFINITY), 1.3),
        (None, 0.1),
        (None, f64::NAN),
    ]
    .into_iter()
    .enumerate()
    {
        let mut node = text("label");
        node.text_layout = Some(CreativeTextLayout {
        letter_spacing: None, paragraph_spacing: None, underline: None, spans: vec![],
            width,
            align: CreativeTextAlign::Left,
            line_height: leading,
        });
        assert!(
            store
                .transact("project_one", edit(&format!("bad_text_{i}"), 0, node))
                .is_err()
        );
    }
    for (i, (radius, blur, opacity, color)) in [
        (-1., 4., 0.2, "#000"),
        (10., 129., 0.2, "#000"),
        (10., 4., 2., "#000"),
        (10., 4., 0.2, "url(https://example.com)"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut node = text("card");
        node.content = CreativeContent::Shape {
            shape: CreativeShapeKind::Rectangle,
            fill: "#fff".into(),
            stroke: "#000".into(),
            stroke_width: 0.,
            line: None,
        };
        node.appearance = Some(CreativeAppearance {
            corner_radius: radius,
            shadow: Some(CreativeShadow {
                x: 0.,
                y: 0.,
                blur,
                opacity,
                color: color.into(),
            }),
        });
        assert!(
            store
                .transact("project_one", edit(&format!("bad_surface_{i}"), 0, node))
                .is_err()
        );
    }
    let mut wrong_kind = text("title");
    wrong_kind.appearance = Some(CreativeAppearance {
        corner_radius: 2.,
        shadow: None,
    });
    assert!(
        store
            .transact("project_one", edit("wrong_kind", 0, wrong_kind))
            .is_err()
    );
    assert_eq!(store.get("project_one").unwrap().document, before);
}

#[test]
fn editable_shell_wireframe_roundtrips_through_the_real_store() {
    let scene: CreativeScene = serde_json::from_str(include_str!(
        "../../../tests/fixtures/creative/medousa-shell-wireframe.json"
    ))
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    let doc = store
        .create(CreativeCreateRequest {
            id: "shell".into(),
            title: "Medousa shell".into(),
            scene: scene.clone(),
        })
        .unwrap();
    assert_eq!(doc.scene, scene);
    assert!(doc.scene.nodes.iter().any(|n| n.text_layout.is_some()));
    assert!(
        doc.scene
            .nodes
            .iter()
            .any(|n| n.appearance.as_ref().is_some_and(|a| a.shadow.is_some()))
    );
}

#[test]
fn code_handoff_context_requires_the_exact_frame_revision_and_visual() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    let scene: CreativeScene = serde_json::from_str(include_str!(
        "../../../tests/fixtures/creative/medousa-shell-wireframe.json"
    ))
    .unwrap();
    store
        .create(CreativeCreateRequest {
            id: "shell".into(),
            title: "Shell".into(),
            scene,
        })
        .unwrap();
    let visual = store
        .import_visual_context(
            "shell",
            CreativeVisualContextRequest {
                expected_revision: 0,
                selected_ids: vec!["shell".into()],
            },
            b"\x89PNG\r\n\x1a\nframe",
        )
        .unwrap();
    let mut request = CreativeCodeContextRequest {
        expected_revision: 0,
        frame_id: "shell".into(),
        visual_asset_id: visual.id,
        session_id: "build_session".into(),
    };
    let (context, _) = store.code_context("shell", &request).unwrap();
    assert_eq!(context.revision, 0);
    let snapshot = store
        .save_context_snapshot(context.clone(), "build_session")
        .unwrap();
    assert_eq!(
        snapshot,
        store
            .save_context_snapshot(context.clone(), "build_session")
            .unwrap()
    );
    assert!(
        store
            .read_context_snapshot(&snapshot, "shell", "unrelated_session")
            .is_err()
    );
    assert!(
        store
            .read_context_snapshot(&snapshot, "other", "build_session")
            .is_err()
    );
    assert!(context.nodes.len() > 20);
    assert_eq!(store.get("shell").unwrap().document.revision, 0);
    request.frame_id = "composer".into();
    assert!(store.code_context("shell", &request).is_err());
    request.frame_id = "shell".into();
    request.expected_revision = 1;
    assert!(store.code_context("shell", &request).is_err());
    store
        .transact(
            "shell",
            CreativeTransaction {
                id: "rename_for_handoff".into(),
                expected_revision: 0,
                label: "Rename".into(),
                operations: vec![CreativeOperation::Rename {
                    title: "Updated shell".into(),
                }],
            },
        )
        .unwrap();
    // A matching document revision cannot reuse an older visual.
    assert!(store.code_context("shell", &request).is_err());
    let fresh = store
        .import_visual_context(
            "shell",
            CreativeVisualContextRequest {
                expected_revision: 1,
                selected_ids: vec!["shell".into()],
            },
            b"\x89PNG\r\n\x1a\nframe",
        )
        .unwrap();
    // Editing the canvas cannot change a previously admitted Code reference.
    assert_eq!(
        serde_json::to_value(
            store
                .read_context_snapshot(&snapshot, "shell", "build_session")
                .unwrap()
        )
        .unwrap(),
        serde_json::to_value(context).unwrap()
    );
    request.visual_asset_id = fresh.id;
    assert_eq!(store.code_context("shell", &request).unwrap().0.revision, 1);
    request.session_id = "../escape".into();
    assert!(store.code_context("shell", &request).is_err());
}

#[test]
fn design_origin_is_advisory_frame_metadata_and_survives_storage() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    let mut node = text("frame");
    node.content = CreativeContent::Frame {
        name: "From preview".into(),
        background: "#fff".into(),
    };
    node.design_source = Some(CreativeDesignSource {
        original_asset_id: None, scroll_y: None, page_index: None, page_count: None,
        work_id: Some("code_project".into()),
        session_id: Some("build_session".into()),
        url: "http://localhost:5173/".into(),
        viewport_width: 1280,
        viewport_height: 720,
        captured_at_ms: 1,
    });
    let saved = store
        .transact("project_one", edit("origin", 0, node.clone()))
        .unwrap();
    assert_eq!(
        saved.document.scene.nodes[0].design_source,
        node.design_source
    );
    node.content = text("label").content;
    assert!(
        store
            .transact("project_one", edit("invalid_origin", 1, node))
            .is_err()
    );
}

#[test]
fn rich_typography_is_durable_and_rejects_invalid_inline_ranges() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    let mut node = text("rich_text");
    node.content = CreativeContent::Text {
        text: "A🌟 title".into(),
        color: "#111111".into(),
        font_family: "Georgia".into(),
        font_size: 40.,
        font_weight: Some(500),
        font_style: None,
    };
    node.text_layout = Some(serde_json::from_value(serde_json::json!({"width":300,"align":"left","line_height":1.2,"letter_spacing":-2.5,"paragraph_spacing":12,"underline":true,"spans":[{"start":1,"end":3,"font_weight":700,"font_style":"italic","color":"#a7442d","underline":false}]})).unwrap());
    let saved = store.transact("project_one", edit("rich",0,node.clone())).unwrap();
    assert_eq!(saved.document.scene.nodes[0].text_layout, node.text_layout);
    let mut invalid = node.clone(); invalid.text_layout.as_mut().unwrap().spans[0].end = 2;
    assert!(store.transact("project_one",edit("split_surrogate",1,invalid)).is_err());
    let mut invalid = node.clone(); invalid.text_layout.as_mut().unwrap().spans[0].font_weight=Some(999);
    assert!(store.transact("project_one",edit("bad_weight",1,invalid)).is_err());
    let mut invalid = node.clone(); invalid.text_layout.as_mut().unwrap().letter_spacing=Some(f64::NAN);
    assert!(store.transact("project_one",edit("bad_tracking",1,invalid)).is_err());
    store.history("project_one",history("undo_rich",1,false)).unwrap();
    let restored=store.history("project_one",history("redo_rich",2,true)).unwrap();
    assert_eq!(restored.document.scene.nodes[0].text_layout,node.text_layout);
}
