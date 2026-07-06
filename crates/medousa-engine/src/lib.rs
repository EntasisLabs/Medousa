//! Medousa turn engine — typed turn events, durable spine, and outbound ports.
//!
//! Transport-free core extracted from the daemon so binaries compose concrete
//! adapters behind the port traits defined here.

pub mod engine;
pub mod ports;
pub mod receipt;
pub mod scratch;
pub mod stream_sink;
pub mod turn_event;
pub mod turn_event_log;

pub use engine::{EngineTurnHandle, TurnLifecyclePorts, TurnRunOutcome, run_turn};
pub use ports::{
    ChannelToolSink, StoreError, ToolSinkEvent, ToolSinkPort, TurnStorePort,
    TurnStreamRegistryPort, TurnTicketPort, UpsertOutcome,
};
pub use receipt::ArtifactReceiptMeta;
pub use scratch::{TurnScratchPhase, TurnScratchpad, WorkerDelegateScratch};
pub use stream_sink::{AgentStreamSink, SharedAgentStreamSink};
pub use turn_event::{
    Principal, PrincipalKind, SequencedTurnEvent, TurnEnvelope, TurnEvent, TurnSurface,
};
pub use turn_event_log::{
    configure_log_root, default_log_root, fold_history_from_events, project_turn_to_history,
    recover_uncommitted, RecoveredTurn, TurnEventLog, TURN_LOG_DIR,
};
