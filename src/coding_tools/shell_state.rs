//! Observation state owned by the existing Coder turn, never by model input.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;

use super::shell_output::ShellOutput;

#[derive(Default)]
pub(crate) struct CoderShellState {
    pub owned_sessions: HashSet<String>,
    pub preferred_session: Option<String>,
    pub cursors: HashMap<String, u64>,
    pub busy_sessions: HashSet<String>,
    pub(super) commands: HashMap<String, Arc<Mutex<Option<ShellCommand>>>>,
}

pub(super) struct ShellCommand {
    pub command: String,
    pub output: ShellOutput,
    pub next_sequence: u64,
    pub input_written: bool,
    pub input_attempted: bool,
    pub replay_truncated: bool,
    pub exited: bool,
    pub interrupted: bool,
}

impl ShellCommand {
    pub fn new(command: String, marker: Option<&str>, next_sequence: u64) -> Self {
        Self {
            command,
            output: ShellOutput::new(marker),
            next_sequence,
            input_written: false,
            input_attempted: false,
            replay_truncated: false,
            exited: false,
            interrupted: false,
        }
    }

    pub fn active(&self) -> bool {
        self.input_attempted && !self.completed()
    }

    pub fn completed(&self) -> bool {
        self.output.exit_code().is_some() || self.exited
    }
}

tokio::task_local! {
    pub(super) static CODER_SHELL_STATE: Arc<Mutex<CoderShellState>>;
}

pub(crate) async fn with_coder_shell_state<T>(
    state: Arc<Mutex<CoderShellState>>,
    invocation: impl std::future::Future<Output = T>,
) -> T {
    CODER_SHELL_STATE.scope(state, invocation).await
}
