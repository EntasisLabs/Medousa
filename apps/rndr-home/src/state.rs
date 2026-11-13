//! Canonical app session/projection and pending-action state.
use crate::backend::Request;
use medousa_rndr_home::Chat;
use tokio::sync::mpsc;

pub(super) struct Model {
    pub(super) chat: Chat,
    pub(super) commands: Option<mpsc::UnboundedSender<Request>>,
    pub(super) models: Vec<(String, String)>,
    pub(super) selection: Option<usize>,
    pub(super) loading: bool,
    pub(super) generation: u64,
    // Memory only. Never reconcile by matching prose: identical prompts are valid.
    pub(super) recovery: Vec<(String, String)>,
    pub(super) pending_recovery: Option<usize>,
    pub(super) history_messages: Vec<(String, String)>,
    pub(super) mode: Option<String>,
    pub(super) modes: Vec<(String, String, bool)>,
    pub(super) depth: String,
}
