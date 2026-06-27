use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum TurnTicketMode {
    Interactive,
    Background,
}

impl Default for TurnTicketMode {
    fn default() -> Self {
        Self::Interactive
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum TurnTicketPhase {
    Accepted,
    Streaming,
    WorkerHandoff,
    BudgetBlocked,
    Done,
    Error,
    Cancelled,
}

impl TurnTicketPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Streaming => "streaming",
            Self::WorkerHandoff => "worker_handoff",
            Self::BudgetBlocked => "budget_blocked",
            Self::Done => "done",
            Self::Error => "error",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn terminal(self) -> bool {
        matches!(self, Self::Done | Self::Error | Self::Cancelled)
    }

    pub fn composer_handoff(self) -> bool {
        matches!(self, Self::WorkerHandoff | Self::BudgetBlocked) || self.terminal()
    }
}
