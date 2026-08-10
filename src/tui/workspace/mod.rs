//! TUI workshop window-manager model (Home sibling).
//!
//! Mirrors `apps/medousa-home` `shellSplitTree` / `shellTabs` caps and algebra so
//! terminal and GUI shells stay aligned. Layout is client-owned; vault/forge/chat
//! authority stays on the daemon.

mod persist;
mod session;
mod split_tree;

pub use persist::{
    clear_workspace_session, clear_workspace_session_for, legacy_workspace_session_path,
    load_workspace_session, load_workspace_session_for, save_workspace_session,
    save_workspace_session_for, workspace_session_path, workspace_session_path_for,
};
pub use session::{
    ChatTab, EditorGroup, FocusDir, MAX_SHELL_DESKTOPS, MAX_SHELL_PANES, MAX_TABS, ShellDesktop,
    ShellDesktopLayout, ShellTab, ShellTabKind, SplitDirection, SplitEdge, WorkspaceShell,
    new_chat_tab, new_code_tab, new_group_id, new_notes_tab, new_review_tab, new_tab_id,
    new_terminal_tab, short_session_title, short_terminal_title,
};
pub use split_tree::{
    RATIO_DEFAULT, RATIO_MAX, RATIO_MIN, SplitBranchDirection, SplitNode, clamp_ratio,
    collect_group_ids, count_leaves, find_group_leaf, leaf_order, merge_target_for_leaf,
    migrate_v1_to_split_root, neighbor_in_direction, new_split_id, remove_leaf, set_branch_ratio,
    split_leaf, split_leaf_at_edge,
};
