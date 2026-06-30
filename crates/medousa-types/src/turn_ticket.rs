use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct TurnTicket {
    pub turn_id: String,
    pub session_id: String,
    pub mode: TurnTicketMode,
    pub phase: TurnTicketPhase,
    pub stream_url: String,
    pub prompt_preview: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_card_id: Option<String>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TurnTicket {
    pub fn composer_handoff(&self) -> bool {
        self.mode == TurnTicketMode::Background || self.phase.composer_handoff()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnTicketConflict {
    pub message: String,
}

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
