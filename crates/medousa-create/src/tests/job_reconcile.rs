use super::*;

#[test]
fn generation_applies_after_unrelated_edits_and_frame_movement_with_one_undo() {
    let (dir, store) = job_store();
    let mut frame = text("title");
    frame.content = CreativeContent::Frame {
        name: "Canvas".into(),
        background: "#fff".into(),
    };
    store
        .transact("project_one", edit("frame", 1, frame.clone()))
        .unwrap();
    let mut request = job_request("image");
    request.expected_revision = 2;
    let job = store.enqueue_job("project_one", "owner", request).unwrap();
    store.claim_job("project_one", "image").unwrap();
    let asset = store
        .import_job_asset(
            "project_one",
            "image",
            &job.turn_id,
            b"\x89PNG\r\n\x1a\ngenerated",
        )
        .unwrap();
    let mut image = text("generated");
    image.parent_id = Some("title".into());
    image.content = CreativeContent::Image {
        asset_id: asset.id.clone(),
        crop: None,
    };
    store
        .complete_job(
            "project_one",
            "image",
            &job.turn_id,
            CreativeProposalRequest {
                id: "image".into(),
                expected_revision: 2,
                selected_ids: vec!["title".into()],
                label: "Generated image".into(),
                operations: vec![CreativeOperation::PutNode {
                    node: Box::new(image.clone()),
                }],
            },
        )
        .unwrap();
    frame.transform.x += 50.;
    store
        .transact("project_one", edit("move_frame", 2, frame.clone()))
        .unwrap();
    let human = text("human_work");
    store
        .transact("project_one", edit("other_work", 3, human.clone()))
        .unwrap();
    let before = store.get("project_one").unwrap().document;
    drop(store);
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    // The existing on-disk generation survives restart and is not regenerated.
    let applied = store
        .decide_job("project_one", "image", "owner", decision(true, 4))
        .unwrap();
    assert_eq!(applied.applied_revision, Some(5));
    let current = store.get("project_one").unwrap().document;
    assert_eq!(current.scene.nodes, vec![frame, human, image]);
    assert!(current.assets.contains_key(&asset.id));
    let undo = store
        .history("project_one", history("undo_result", 5, false))
        .unwrap();
    assert_eq!(undo.document.scene, before.scene);
    assert_eq!(
        store
            .decide_job("project_one", "image", "owner", decision(true, 4))
            .unwrap()
            .applied_revision,
        Some(5)
    );
}

#[test]
fn conflicting_fields_preserve_the_result_and_clear_after_undo() {
    let (_dir, store) = job_store();
    let job = store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    store.claim_job("project_one", "first").unwrap();
    store
        .complete_job("project_one", "first", &job.turn_id, job_result("first"))
        .unwrap();
    let mut human = text("title");
    if let CreativeContent::Text { text, .. } = &mut human.content {
        *text = "Human title".into();
    }
    store
        .transact("project_one", edit("human", 1, human.clone()))
        .unwrap();
    assert!(
        store
            .decide_job("project_one", "first", "owner", decision(true, 2))
            .unwrap_err()
            .to_string()
            .contains("generation is still saved")
    );
    assert_eq!(
        store.get("project_one").unwrap().document.scene.nodes[0],
        human
    );
    assert!(
        store
            .job("project_one", "first", "owner")
            .unwrap()
            .result
            .is_some()
    );
    store
        .history("project_one", history("undo_conflict", 2, false))
        .unwrap();
    store
        .decide_job("project_one", "first", "owner", decision(true, 3))
        .unwrap();
}

#[test]
fn reconciliation_preserves_new_human_children_and_current_lock_state() {
    let (_dir, store) = job_store();
    let mut frame = text("title");
    frame.content = CreativeContent::Frame {
        name: "F".into(),
        background: "#fff".into(),
    };
    store
        .transact("project_one", edit("frame", 1, frame))
        .unwrap();
    let mut request = job_request("delete");
    request.expected_revision = 2;
    let job = store.enqueue_job("project_one", "owner", request).unwrap();
    store.claim_job("project_one", "delete").unwrap();
    store
        .complete_job(
            "project_one",
            "delete",
            &job.turn_id,
            CreativeProposalRequest {
                id: "delete".into(),
                expected_revision: 2,
                selected_ids: vec!["title".into()],
                label: "Remove frame".into(),
                operations: vec![CreativeOperation::RemoveNode { id: "title".into() }],
            },
        )
        .unwrap();
    let mut child = text("new_child");
    child.parent_id = Some("title".into());
    store
        .transact("project_one", edit("new_work", 2, child))
        .unwrap();
    assert!(
        store
            .decide_job("project_one", "delete", "owner", decision(true, 3))
            .is_err()
    );
    assert_eq!(
        store.get("project_one").unwrap().document.scene.nodes.len(),
        2
    );
    let (_dir, store) = job_store();
    let job = store
        .enqueue_job("project_one", "owner", job_request("locked"))
        .unwrap();
    store.claim_job("project_one", "locked").unwrap();
    store
        .complete_job("project_one", "locked", &job.turn_id, job_result("locked"))
        .unwrap();
    let mut locked = text("title");
    locked.locked = true;
    store
        .transact("project_one", edit("lock", 1, locked))
        .unwrap();
    store
        .decide_job("project_one", "locked", "owner", decision(true, 2))
        .unwrap();
    assert!(store.get("project_one").unwrap().document.scene.nodes[0].locked);
}

#[test]
fn refinement_reconciles_against_the_current_human_base_before_capturing() {
    let (_dir, store) = job_store();
    let first = store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    store.claim_job("project_one", "first").unwrap();
    store
        .complete_job("project_one", "first", &first.turn_id, job_result("first"))
        .unwrap();
    let mut moved = text("title");
    moved.transform.x = 42.;
    store
        .transact("project_one", edit("move", 1, moved.clone()))
        .unwrap();
    let mut request = job_request("follow");
    request.expected_revision = 2;
    request.parent_job_id = Some("first".into());
    request.refine_proposal = true;
    let next = store.enqueue_job("project_one", "owner", request).unwrap();
    assert_eq!(next.session_id, first.session_id);
    assert_eq!(next.base_nodes, Some(vec![moved]));
    assert_eq!(next.context.nodes[0].transform.x, 42.);
    assert!(
        matches!(next.context.nodes[0].content, CreativeContent::Text { ref text, .. } if text == "A clearer title")
    );
}

#[test]
fn existing_saved_generations_without_base_nodes_recover_after_upgrade() {
    let (dir, store) = job_store();
    let first = store
        .enqueue_job("project_one", "owner", job_request("old"))
        .unwrap();
    store.claim_job("project_one", "old").unwrap();
    store
        .complete_job("project_one", "old", &first.turn_id, job_result("old"))
        .unwrap();
    let mut follow = job_request("old_follow");
    follow.parent_job_id = Some("old".into());
    follow.refine_proposal = true;
    let second = store.enqueue_job("project_one", "owner", follow).unwrap();
    store.claim_job("project_one", "old_follow").unwrap();
    let mut result = job_result("old_follow");
    if let CreativeOperation::PutNode { node } = &mut result.operations[0] {
        node.opacity = 0.5;
    }
    store
        .complete_job("project_one", "old_follow", &second.turn_id, result)
        .unwrap();
    drop(store);
    let file = dir.path().join("projects/project_one.json");
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    for job in record["jobs"].as_array_mut().unwrap() {
        job["job"].as_object_mut().unwrap().remove("base_nodes");
    }
    std::fs::write(file, serde_json::to_vec(&record).unwrap()).unwrap();
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    assert_eq!(
        store
            .job("project_one", "old_follow", "owner")
            .unwrap()
            .base_nodes,
        Some(vec![text("title")])
    );
    let mut human = text("title");
    human.transform.x = 150.;
    store
        .transact("project_one", edit("after_upgrade", 1, human))
        .unwrap();
    store
        .decide_job("project_one", "old_follow", "owner", decision(true, 2))
        .unwrap();
    let current = store.get("project_one").unwrap();
    assert_eq!(current.document.scene.nodes[0].transform.x, 150.);
    assert_eq!(current.document.scene.nodes[0].opacity, 0.5);
}
