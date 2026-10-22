use super::*;
const PNG: &[u8] = b"\x89PNG\r\n\x1a\nselection pixels";

#[test]
fn visual_capture_is_revision_bound_durable_and_never_a_human_edit() {
    let (dir, store) = job_store();
    let before = store.get("project_one").unwrap();
    let asset = store
        .import_visual_context(
            "project_one",
            CreativeVisualContextRequest {
                expected_revision: 1,
                selected_ids: vec!["title".into()],
            },
            PNG,
        )
        .unwrap();
    assert_eq!(store.get("project_one").unwrap().document, before.document);
    let mut request = job_request("visual");
    request.visual_asset_id = Some(asset.id.clone());
    let job = store
        .enqueue_job("project_one", "owner", request.clone())
        .unwrap();
    assert_eq!(job.visual_context, Some(asset.clone()));
    assert_eq!(store.asset("project_one", &asset.id).unwrap().1, PNG);
    drop(store);
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    assert_eq!(
        store
            .enqueue_job("project_one", "owner", request.clone())
            .unwrap()
            .session_id,
        job.session_id
    );
    request.id = "reuse_without_capture".into();
    assert!(store.enqueue_job("project_one", "owner", request).is_err());
    let undo = store
        .history("project_one", history("human_undo", 1, false))
        .unwrap();
    assert!(undo.document.scene.nodes.is_empty());
}

#[test]
fn preview_refinements_share_a_conversation_and_apply_as_one_undoable_replacement() {
    let (_dir, store) = job_store();
    let first = store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    store.claim_job("project_one", "first").unwrap();
    let mut initial = job_result("first");
    if let CreativeOperation::PutNode { node } = &mut initial.operations[0] {
        if let CreativeContent::Text { font_size, .. } = &mut node.content {
            *font_size = 48.;
        }
    }
    store
        .complete_job("project_one", "first", &first.turn_id, initial)
        .unwrap();
    let mut follow = job_request("follow");
    follow.parent_job_id = Some("first".into());
    follow.refine_proposal = true;
    let second = store
        .enqueue_job("project_one", "owner", follow.clone())
        .unwrap();
    assert_eq!(second.session_id, first.session_id);
    assert_ne!(second.turn_id, first.turn_id);
    assert!(matches!(
        second.context.nodes[0].content,
        CreativeContent::Text { font_size: 48., .. }
    ));
    assert_eq!(
        store.get("project_one").unwrap().document.scene.nodes[0],
        text("title")
    );
    follow.id = "parallel".into();
    assert!(store.enqueue_job("project_one", "owner", follow).is_err());
    store.claim_job("project_one", "follow").unwrap();
    let mut node = second.context.nodes[0].clone();
    if let CreativeContent::Text { color, .. } = &mut node.content {
        *color = "#ff0000".into();
    }
    let result = CreativeProposalRequest {
        id: "follow".into(),
        expected_revision: 1,
        selected_ids: vec!["title".into()],
        label: "Larger and red".into(),
        operations: vec![CreativeOperation::PutNode { node }],
    };
    let ready = store
        .complete_job("project_one", "follow", &second.turn_id, result.clone())
        .unwrap();
    assert_eq!(
        serde_json::to_value(
            store
                .complete_job("project_one", "follow", &second.turn_id, result)
                .unwrap()
                .result
        )
        .unwrap(),
        serde_json::to_value(ready.result).unwrap()
    );
    store
        .decide_job(
            "project_one",
            "follow",
            "owner",
            CreativeProposalDecision {
                accept: true,
                expected_revision: 1,
            },
        )
        .unwrap();
    let applied = store.get("project_one").unwrap();
    assert!(
        matches!(applied.document.scene.nodes[0].content, CreativeContent::Text { font_size: 48., ref color, .. } if color == "#ff0000")
    );
    let undo = store
        .history("project_one", history("undo_ai", 2, false))
        .unwrap();
    assert_eq!(undo.document.scene.nodes[0], text("title"));
    let mut after = job_request("after_apply");
    after.expected_revision = 3;
    after.parent_job_id = Some("follow".into());
    assert_eq!(
        store
            .enqueue_job("project_one", "owner", after)
            .unwrap()
            .session_id,
        first.session_id
    );
}

#[test]
fn refinement_keeps_generated_image_references_without_importing_a_human_edit() {
    let (_dir, store) = job_store();
    let first = store
        .enqueue_job("project_one", "owner", job_request("image"))
        .unwrap();
    store.claim_job("project_one", "image").unwrap();
    let asset = store
        .import_job_asset("project_one", "image", &first.turn_id, PNG)
        .unwrap();
    let mut proposal = job_result("image");
    if let CreativeOperation::PutNode { node } = &mut proposal.operations[0] {
        node.content = CreativeContent::Image {
            asset_id: asset.id.clone(),
            crop: None,
        };
    }
    store
        .complete_job("project_one", "image", &first.turn_id, proposal)
        .unwrap();
    let mut follow = job_request("refine_image");
    follow.parent_job_id = Some("image".into());
    follow.refine_proposal = true;
    let next = store.enqueue_job("project_one", "owner", follow).unwrap();
    assert_eq!(next.context.assets, vec![asset.clone()]);
    assert_eq!(store.asset("project_one", &asset.id).unwrap().1, PNG);
    assert_eq!(store.get("project_one").unwrap().document.revision, 1);
    assert!(store.get("project_one").unwrap().document.assets.is_empty());
    store.claim_job("project_one", "refine_image").unwrap();
    let mut node = next.context.nodes[0].clone();
    node.transform.x += 20.;
    store
        .complete_job(
            "project_one",
            "refine_image",
            &next.turn_id,
            CreativeProposalRequest {
                id: "refine_image".into(),
                expected_revision: 1,
                selected_ids: vec!["title".into()],
                label: "Refine image placement".into(),
                operations: vec![CreativeOperation::PutNode { node }],
            },
        )
        .unwrap();
    store
        .decide_job("project_one", "refine_image", "owner", decision(true, 1))
        .unwrap();
    assert!(
        store
            .get("project_one")
            .unwrap()
            .document
            .assets
            .contains_key(&asset.id)
    );
    let undone = store
        .history("project_one", history("undo_image", 2, false))
        .unwrap();
    assert_eq!(undone.document.scene.nodes[0], text("title"));
}

#[test]
fn continuation_cannot_borrow_another_owner_or_refine_a_stale_preview() {
    let (_dir, store) = job_store();
    let first = store
        .enqueue_job("project_one", "owner", job_request("first"))
        .unwrap();
    store.claim_job("project_one", "first").unwrap();
    store
        .complete_job("project_one", "first", &first.turn_id, job_result("first"))
        .unwrap();
    let mut follow = job_request("follow");
    follow.parent_job_id = Some("first".into());
    assert!(
        store
            .enqueue_job("project_one", "stranger", follow.clone())
            .is_err()
    );
    let asset = store
        .import_visual_context(
            "project_one",
            CreativeVisualContextRequest {
                expected_revision: 1,
                selected_ids: vec!["title".into()],
            },
            PNG,
        )
        .unwrap();
    store
        .transact("project_one", edit("human", 1, text("other")))
        .unwrap();
    follow.expected_revision = 2;
    follow.refine_proposal = true;
    assert!(
        store
            .enqueue_job("project_one", "owner", follow.clone())
            .is_err()
    );
    follow.refine_proposal = false;
    follow.visual_asset_id = Some(asset.id);
    assert!(
        store
            .enqueue_job("project_one", "owner", follow.clone())
            .is_err()
    );
    follow.visual_asset_id = None;
    assert_eq!(
        store
            .enqueue_job("project_one", "owner", follow)
            .unwrap()
            .session_id,
        first.session_id
    );
}

#[test]
fn continuing_conversations_archive_finished_jobs_without_replaying_old_ids() {
    let (dir, store) = job_store();
    let mut parent: Option<String> = None;
    let mut session = String::new();
    for n in 0..40 {
        let id = format!("refine_{n}");
        let mut request = job_request(&id);
        request.parent_job_id = parent.clone();
        request.refine_proposal = parent.is_some();
        let job = store.enqueue_job("project_one", "owner", request).unwrap();
        if n == 0 {
            session = job.session_id.clone();
        } else {
            assert_eq!(job.session_id, session);
        }
        store.claim_job("project_one", &id).unwrap();
        let mut proposal = job_result(&id);
        if let CreativeOperation::PutNode { node } = &mut proposal.operations[0] {
            if let CreativeContent::Text { text, .. } = &mut node.content {
                *text = format!("Refinement {n}");
            }
        }
        store
            .complete_job("project_one", &id, &job.turn_id, proposal)
            .unwrap();
        parent = Some(id);
    }
    assert!(store.jobs("project_one", "owner").unwrap().jobs.len() <= 32);
    assert_eq!(store.get("project_one").unwrap().document.revision, 1);
    drop(store);
    let store = CreativeStore::open(&dir.path().canonicalize().unwrap()).unwrap();
    // Lost admission receipts must still resolve after subsequent human edits.
    store
        .transact("project_one", edit("after_archive", 1, text("other")))
        .unwrap();
    assert_eq!(
        store
            .enqueue_job("project_one", "owner", job_request("refine_0"))
            .unwrap()
            .status,
        CreativeJobStatus::Superseded
    );
    assert_eq!(
        store
            .job("project_one", "refine_0", "owner")
            .unwrap()
            .session_id,
        session
    );
    assert!(store.job("project_one", "refine_0", "other").is_err());
    assert!(
        store
            .decide_job("project_one", "refine_0", "owner", decision(true, 1))
            .is_err()
    );
}
