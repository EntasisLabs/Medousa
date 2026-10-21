use super::*;
use std::sync::{Arc, Barrier};
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
        },
    }
}
fn edit(id: &str, revision: u64, node: CreativeNode) -> CreativeTransaction {
    CreativeTransaction {
        id: id.into(),
        expected_revision: revision,
        label: "Edit text".into(),
        operations: vec![CreativeOperation::PutNode { node }],
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
                operations: vec![CreativeOperation::PutNode {
                    node: text("title"),
                }],
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
                operations: vec![CreativeOperation::PutNode {
                    node: replacement.clone(),
                }],
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
fn proposals_cannot_change_objects_outside_their_selection() {
    let dir = tempfile::tempdir().unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    create(&store);
    store
        .transact("project_one", edit("put", 0, text("title")))
        .unwrap();
    let request = CreativeProposalRequest {
        id: "outside".into(),
        expected_revision: 1,
        selected_ids: vec!["title".into()],
        label: "Outside".into(),
        operations: vec![CreativeOperation::Rename {
            title: "Changed".into(),
        }],
    };
    assert!(store.propose("project_one", request).is_err());
    assert_eq!(
        store.get("project_one").unwrap().document.title,
        "Release graphic"
    );
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
fn frame_context_captures_descendants_and_locked_work_cannot_be_changed_by_a_proposal() {
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
                    CreativeOperation::PutNode { node: frame },
                    CreativeOperation::PutNode {
                        node: child.clone(),
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
        operations: vec![CreativeOperation::PutNode { node: child }],
    };
    assert!(store.propose("project_one", request).is_err());
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
        operations: vec![CreativeOperation::PutNode {
            node: text("title"),
        }],
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
