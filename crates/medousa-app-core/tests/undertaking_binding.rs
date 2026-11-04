use medousa_app_core::{
    UndertakingBindingCommand, UndertakingBindingCore, UndertakingBindingUpdate,
    UndertakingContextFact,
};

fn ctx(work_id: &str, chats: &[&str]) -> UndertakingContextFact {
    UndertakingContextFact {
        work_id: work_id.into(),
        execution_runtime_id: None,
        execution_transport_runtime_id: None,
        repo_id: None,
        title: work_id.into(),
        human_phase: "work".into(),
        forge_state: "ready".into(),
        worktree: None,
        baseline_oid: None,
        sealed_oid: None,
        lease_id: None,
        lease_generation: None,
        executor_kind: None,
        attempt_seq: None,
        bound_chat_session_ids: chats.iter().map(|s| (*s).into()).collect(),
        bound_terminal_session_ids: vec![],
    }
}

#[test]
fn bind_chat_moves_not_duplicates() {
    let mut core = UndertakingBindingCore::new("personal");
    core.dispatch(UndertakingBindingCommand::SetActive {
        group_id: "codePane".into(),
        context: Box::new(ctx("project-a", &["chat-a"])),
    })
    .unwrap();
    core.dispatch(UndertakingBindingCommand::SetActive {
        group_id: "chatPane".into(),
        context: Box::new(ctx("project-a", &[])),
    })
    .unwrap();

    let UndertakingBindingUpdate::Snapshot { snapshot } = core
        .dispatch(UndertakingBindingCommand::BindChat {
            session_id: "chat-a".into(),
            group_id: "chatPane".into(),
        })
        .unwrap();

    let code = snapshot
        .contexts
        .iter()
        .find(|g| g.group_id == "codePane")
        .and_then(|g| g.context.as_ref())
        .unwrap();
    let chat = snapshot
        .contexts
        .iter()
        .find(|g| g.group_id == "chatPane")
        .and_then(|g| g.context.as_ref())
        .unwrap();
    assert!(code.bound_chat_session_ids.is_empty());
    assert_eq!(chat.bound_chat_session_ids, vec!["chat-a".to_string()]);
}

#[test]
fn detach_chat_clears_all_groups() {
    let mut core = UndertakingBindingCore::new("personal");
    core.dispatch(UndertakingBindingCommand::SetActive {
        group_id: "codePane".into(),
        context: Box::new(ctx("project-a", &["chat-a"])),
    })
    .unwrap();
    let UndertakingBindingUpdate::Snapshot { snapshot } = core
        .dispatch(UndertakingBindingCommand::DetachChat {
            session_id: "chat-a".into(),
        })
        .unwrap();
    let code = snapshot
        .contexts
        .iter()
        .find(|g| g.group_id == "codePane")
        .and_then(|g| g.context.as_ref())
        .unwrap();
    assert!(code.bound_chat_session_ids.is_empty());
}

#[test]
fn set_active_preserves_bindings_for_same_work() {
    let mut core = UndertakingBindingCore::new("personal");
    core.dispatch(UndertakingBindingCommand::SetActive {
        group_id: "main".into(),
        context: Box::new(ctx("project-a", &["chat-a"])),
    })
    .unwrap();
    let UndertakingBindingUpdate::Snapshot { snapshot } = core
        .dispatch(UndertakingBindingCommand::SetActive {
            group_id: "main".into(),
            context: Box::new(ctx("project-a", &[])),
        })
        .unwrap();
    let main = snapshot
        .contexts
        .iter()
        .find(|g| g.group_id == "main")
        .and_then(|g| g.context.as_ref())
        .unwrap();
    assert_eq!(main.bound_chat_session_ids, vec!["chat-a".to_string()]);
}
