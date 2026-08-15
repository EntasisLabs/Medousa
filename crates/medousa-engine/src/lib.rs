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
pub mod turn_pipeline;

pub use engine::{EngineTurnHandle, TurnLifecyclePorts, TurnRunOutcome, run_turn};
pub use ports::{
    ChannelToolSink, StoreError, ToolSinkEvent, ToolSinkPort, TurnStorePort,
    TurnStreamRegistryPort, TurnTicketPort, UpsertOutcome,
};
pub use receipt::ArtifactReceiptMeta;
pub use scratch::{TurnScratchPhase, TurnScratchpad, WorkerDelegateScratch};
pub use stream_sink::{AgentStreamSink, SharedAgentStreamSink, ToolInputParam};
pub use turn_event::{
    Principal, PrincipalKind, SequencedTurnEvent, TurnEnvelope, TurnEvent, TurnSurface,
};
pub use turn_event_log::{
    configure_log_root, default_log_root, fold_history_from_events, project_turn_to_history,
    recover_uncommitted, JournalAppendReceipt, JournalCommitReceipt, JournalDurability,
    RecoveredTurn, TurnEventLog, TurnEventLogMetrics, TurnReplayPage, TURN_LOG_DIR,
};
pub use turn_pipeline::{
    TURN_PIPELINE_BATCH_BYTES, TURN_PIPELINE_BYTE_CAPACITY, TURN_PIPELINE_COALESCE,
    TURN_PIPELINE_COMMAND_CAPACITY, TurnPipelineEmission, TurnPipelineError, TurnPipelineHandle,
    TurnPipelineMetrics, TurnPipelineMetricsSnapshot, TurnPipelineOutput,
};
