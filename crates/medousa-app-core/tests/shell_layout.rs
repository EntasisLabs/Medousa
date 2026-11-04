//! Shell layout identity, focus, and pane-limit tests.

use medousa_app_core::shell::{
    ShellCommand, ShellCore, ShellCoreError, ShellTab, ShellUpdate, SplitEdge, MAX_SHELL_PANES,
    MAX_SHELL_DESKTOPS,
};

fn chat(id: &str, session: &str) -> ShellTab {
    ShellTab::Chat {
        id: id.into(),
        session_id: session.into(),
        title: session.into(),
    }
}

#[test]
fn open_chat_focuses_and_reuses_existing_session() {
    let mut core = ShellCore::new("personal");
    let ShellUpdate::Snapshot { snapshot } = core
        .dispatch(ShellCommand::OpenTab {
            tab: chat("t1", "ses_a"),
            group_id: None,
            activate: true,
        })
        .unwrap();
    assert_eq!(snapshot.layout.tabs.len(), 1);
    assert_eq!(
        snapshot.layout.groups[0].active_tab_id.as_deref(),
        Some("t1")
    );

    let ShellUpdate::Snapshot { snapshot } = core
        .dispatch(ShellCommand::OpenTab {
            tab: chat("t2", "ses_a"),
            group_id: None,
            activate: true,
        })
        .unwrap();
    assert_eq!(snapshot.layout.tabs.len(), 1);
    assert_eq!(snapshot.layout.tabs[0].id(), "t1");
}

#[test]
fn background_open_does_not_steal_active_tab() {
    let mut core = ShellCore::new("personal");
    core.dispatch(ShellCommand::OpenTab {
        tab: chat("t1", "ses_a"),
        group_id: None,
        activate: true,
    })
    .unwrap();
    let ShellUpdate::Snapshot { snapshot } = core
        .dispatch(ShellCommand::OpenTab {
            tab: chat("t2", "ses_b"),
            group_id: None,
            activate: false,
        })
        .unwrap();
    assert_eq!(snapshot.layout.tabs.len(), 2);
    assert_eq!(
        snapshot.layout.groups[0].active_tab_id.as_deref(),
        Some("t1")
    );
}

#[test]
fn split_respects_pane_cap_and_close_merges_tabs() {
    let mut core = ShellCore::new("personal");
    core.dispatch(ShellCommand::OpenTab {
        tab: chat("t1", "ses_a"),
        group_id: None,
        activate: true,
    })
    .unwrap();

    for i in 0..(MAX_SHELL_PANES - 1) {
        let host = if i == 0 {
            "main".to_string()
        } else {
            format!("pane-{i}")
        };
        let new_id = format!("pane-{}", i + 1);
        core.dispatch(ShellCommand::SplitPane {
            group_id: host,
            edge: SplitEdge::Right,
            new_group_id: new_id.clone(),
            move_tab_id: None,
        })
        .unwrap();
        core.dispatch(ShellCommand::OpenTab {
            tab: chat(&format!("t{}", i + 2), &format!("ses_{}", i + 2)),
            group_id: Some(new_id),
            activate: true,
        })
        .unwrap();
    }

    let err = core
        .dispatch(ShellCommand::SplitPane {
            group_id: "main".into(),
            edge: SplitEdge::Right,
            new_group_id: "overflow".into(),
            move_tab_id: None,
        })
        .unwrap_err();
    assert_eq!(err, ShellCoreError::PaneLimit);

    // Closing a pane merges its tabs into the sash neighbor.
    let ShellUpdate::Snapshot { snapshot } = core
        .dispatch(ShellCommand::ClosePane {
            group_id: "pane-1".into(),
        })
        .unwrap();
    assert!(snapshot.layout.groups.iter().all(|g| g.id != "pane-1"));
    assert!(snapshot.layout.tabs.iter().any(|t| t.id() == "t2"));
}

#[test]
fn desktop_cap_and_switch_preserve_layouts() {
    let mut core = ShellCore::new("personal");
    core.dispatch(ShellCommand::OpenTab {
        tab: chat("t1", "ses_a"),
        group_id: None,
        activate: true,
    })
    .unwrap();

    for i in 0..(MAX_SHELL_DESKTOPS - 1) {
        core.dispatch(ShellCommand::CreateDesktop {
            desktop_id: format!("d{i}"),
            name: format!("Desk {i}"),
        })
        .unwrap();
    }
    let err = core
        .dispatch(ShellCommand::CreateDesktop {
            desktop_id: "overflow".into(),
            name: "Nope".into(),
        })
        .unwrap_err();
    assert_eq!(err, ShellCoreError::DesktopLimit);

    core.dispatch(ShellCommand::SwitchDesktop {
        desktop_id: "desktop-main".into(),
    })
    .unwrap();
    let ShellUpdate::Snapshot { snapshot } = core.dispatch(ShellCommand::Snapshot).unwrap();
    assert_eq!(snapshot.active_desktop_id, "desktop-main");
    assert_eq!(snapshot.layout.tabs.len(), 1);
}

#[test]
fn hydrate_restores_workspace_scope_layout() {
    let mut core = ShellCore::new("old");
    let ShellUpdate::Snapshot { snapshot: seed } = core
        .dispatch(ShellCommand::OpenTab {
            tab: chat("t1", "ses_a"),
            group_id: None,
            activate: true,
        })
        .unwrap();

    let mut other = ShellCore::new("");
    let ShellUpdate::Snapshot { snapshot } = other
        .dispatch(ShellCommand::Hydrate {
            workspace_scope_id: "workshop-a".into(),
            desktops: seed.desktops,
            active_desktop_id: seed.active_desktop_id,
        })
        .unwrap();
    assert_eq!(snapshot.workspace_scope_id, "workshop-a");
    assert_eq!(snapshot.layout.tabs[0].id(), "t1");
}
