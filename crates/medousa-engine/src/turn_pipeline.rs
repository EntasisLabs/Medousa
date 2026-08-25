use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use medousa_types::{
    TurnStreamEnvelopeV2, TurnStreamEnvelopeV3, TurnStreamEventV2, TurnStreamEventV3,
};
use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::TurnEvent;

pub const TURN_PIPELINE_COMMAND_CAPACITY: usize = 256;
pub const TURN_PIPELINE_BYTE_CAPACITY: usize = 1024 * 1024;
pub const TURN_PIPELINE_BATCH_BYTES: usize = 32 * 1024;
pub const TURN_PIPELINE_COALESCE: Duration = Duration::from_millis(16);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnPipelineError {
    Closed,
    Cancelled,
    Terminal,
    PayloadTooLarge { bytes: usize, limit: usize },
    Output(String),
}

impl std::fmt::Display for TurnPipelineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => formatter.write_str("turn pipeline is closed"),
            Self::Cancelled => formatter.write_str("turn pipeline was cancelled"),
            Self::Terminal => formatter.write_str("turn pipeline is already terminal"),
            Self::PayloadTooLarge { bytes, limit } => {
                write!(
                    formatter,
                    "turn pipeline payload is {bytes} bytes; limit is {limit}"
                )
            }
            Self::Output(message) => write!(formatter, "turn pipeline output failed: {message}"),
        }
    }
}

impl std::error::Error for TurnPipelineError {}

pub trait TurnPipelineOutput: Send + Sync + 'static {
    fn publish(
        &self,
        emission: TurnPipelineEmission,
    ) -> impl Future<Output = Result<(), TurnPipelineError>> + Send;
}

/// An actor-owned public emission plus optional richer journal-only state.
pub struct TurnPipelineEmission {
    pub envelope: TurnPipelineEnvelope,
    pub journal_override: Option<TurnEvent>,
}

/// One native stream fact. The actor sequences either version directly; it
/// never reconstructs V3 meaning from a V2 event.
#[derive(Debug, Clone)]
pub enum TurnPipelineEnvelope {
    V2(TurnStreamEnvelopeV2),
    V3(TurnStreamEnvelopeV3),
}

impl TurnPipelineEnvelope {
    pub fn seq(&self) -> u64 {
        match self {
            Self::V2(envelope) => envelope.seq,
            Self::V3(envelope) => envelope.seq,
        }
    }

    pub fn is_terminal(&self) -> bool {
        match self {
            Self::V2(envelope) => envelope.event.is_terminal(),
            Self::V3(envelope) => envelope.event.is_terminal(),
        }
    }
}

#[derive(Debug)]
enum TurnPipelineEvent {
    V2(TurnStreamEventV2),
    V3(TurnStreamEventV3),
}

impl TurnPipelineEvent {
    fn is_terminal(&self) -> bool {
        match self {
            Self::V2(event) => event.is_terminal(),
            Self::V3(event) => event.is_terminal(),
        }
    }
}

struct PipelineCommand {
    event: TurnPipelineEvent,
    journal_override: Option<TurnEvent>,
    admission: PipelineAdmission,
    ack: oneshot::Sender<Result<u64, TurnPipelineError>>,
}

struct PipelineAdmission {
    bytes: usize,
    _turn_message: tokio::sync::OwnedSemaphorePermit,
    _turn_bytes: tokio::sync::OwnedSemaphorePermit,
    _global_bytes: tokio::sync::OwnedSemaphorePermit,
}

struct PipelineReceipt {
    admission: PipelineAdmission,
    ack: oneshot::Sender<Result<u64, TurnPipelineError>>,
}

// Boxing `Emit` would allocate on every provider fragment. The bounded channel
// makes the larger inline slot a deliberate, fixed memory cost.
#[allow(clippy::large_enum_variant)]
enum PipelineMessage {
    Emit(PipelineCommand),
    Flush(oneshot::Sender<Result<(), TurnPipelineError>>),
}

#[derive(Debug, Default)]
pub struct TurnPipelineMetrics {
    queued_messages: AtomicUsize,
    queued_bytes: AtomicUsize,
    message_high_water: AtomicUsize,
    byte_high_water: AtomicUsize,
    emitted_events: AtomicU64,
    coalesced_commands: AtomicU64,
    rejected_commands: AtomicU64,
    blocked_send_nanos: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnPipelineMetricsSnapshot {
    pub queued_messages: usize,
    pub queued_bytes: usize,
    pub message_high_water: usize,
    pub byte_high_water: usize,
    pub emitted_events: u64,
    pub coalesced_commands: u64,
    pub rejected_commands: u64,
    pub blocked_send_nanos: u64,
}

impl TurnPipelineMetrics {
    pub fn snapshot(&self) -> TurnPipelineMetricsSnapshot {
        TurnPipelineMetricsSnapshot {
            queued_messages: self.queued_messages.load(Ordering::Relaxed),
            queued_bytes: self.queued_bytes.load(Ordering::Relaxed),
            message_high_water: self.message_high_water.load(Ordering::Relaxed),
            byte_high_water: self.byte_high_water.load(Ordering::Relaxed),
            emitted_events: self.emitted_events.load(Ordering::Relaxed),
            coalesced_commands: self.coalesced_commands.load(Ordering::Relaxed),
            rejected_commands: self.rejected_commands.load(Ordering::Relaxed),
            blocked_send_nanos: self.blocked_send_nanos.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone)]
pub struct TurnPipelineHandle {
    tx: mpsc::Sender<PipelineMessage>,
    turn_messages: Arc<Semaphore>,
    turn_bytes: Arc<Semaphore>,
    global_bytes: Arc<Semaphore>,
    metrics: Arc<TurnPipelineMetrics>,
    cancellation: CancellationToken,
}

impl TurnPipelineHandle {
    pub fn spawn(
        turn_id: impl Into<String>,
        initial_seq: u64,
        global_bytes: Arc<Semaphore>,
        output: Arc<impl TurnPipelineOutput>,
    ) -> Self {
        let (tx, rx) = mpsc::channel(TURN_PIPELINE_COMMAND_CAPACITY);
        let metrics = Arc::new(TurnPipelineMetrics::default());
        let cancellation = CancellationToken::new();
        tokio::spawn(run_pipeline(
            turn_id.into(),
            initial_seq,
            rx,
            Arc::clone(&metrics),
            output,
            cancellation.clone(),
        ));
        Self {
            tx,
            turn_messages: Arc::new(Semaphore::new(TURN_PIPELINE_COMMAND_CAPACITY)),
            turn_bytes: Arc::new(Semaphore::new(TURN_PIPELINE_BYTE_CAPACITY)),
            global_bytes,
            metrics,
            cancellation,
        }
    }

    pub async fn emit(&self, event: TurnStreamEventV2) -> Result<u64, TurnPipelineError> {
        self.emit_event_with_journal(TurnPipelineEvent::V2(event), None)
            .await
    }

    /// Transfers an event into the bounded actor without waiting for output.
    /// Semantic and terminal producers should use [`Self::emit`]; this is for
    /// high-frequency provider text whose final fence is a later semantic event.
    pub async fn admit(&self, event: TurnStreamEventV2) -> Result<(), TurnPipelineError> {
        self.send(TurnPipelineEvent::V2(event), None)
            .await
            .map(drop)
    }

    pub async fn emit_v3(&self, event: TurnStreamEventV3) -> Result<u64, TurnPipelineError> {
        self.emit_v3_with_journal(event, None).await
    }

    /// Transfers a V3 provider fragment into the actor without waiting for
    /// output. A later semantic event or explicit flush remains its fence.
    pub async fn admit_v3(&self, event: TurnStreamEventV3) -> Result<(), TurnPipelineError> {
        self.send(TurnPipelineEvent::V3(event), None)
            .await
            .map(drop)
    }

    pub async fn emit_with_journal(
        &self,
        event: TurnStreamEventV2,
        journal_override: Option<TurnEvent>,
    ) -> Result<u64, TurnPipelineError> {
        self.emit_event_with_journal(TurnPipelineEvent::V2(event), journal_override)
            .await
    }

    pub async fn emit_v3_with_journal(
        &self,
        event: TurnStreamEventV3,
        journal_override: Option<TurnEvent>,
    ) -> Result<u64, TurnPipelineError> {
        self.emit_event_with_journal(TurnPipelineEvent::V3(event), journal_override)
            .await
    }

    async fn emit_event_with_journal(
        &self,
        event: TurnPipelineEvent,
        journal_override: Option<TurnEvent>,
    ) -> Result<u64, TurnPipelineError> {
        let receipt = self.send(event, journal_override).await?;
        tokio::select! {
            biased;
            () = self.cancellation.cancelled() => Err(TurnPipelineError::Cancelled),
            result = receipt => result.map_err(|_| TurnPipelineError::Closed)?,
        }
    }

    async fn send(
        &self,
        event: TurnPipelineEvent,
        journal_override: Option<TurnEvent>,
    ) -> Result<oneshot::Receiver<Result<u64, TurnPipelineError>>, TurnPipelineError> {
        let mut encoded_size = EncodedSize::default();
        match &event {
            TurnPipelineEvent::V2(event) => serde_json::to_writer(&mut encoded_size, event),
            TurnPipelineEvent::V3(event) => serde_json::to_writer(&mut encoded_size, event),
        }
        .map_err(|error| TurnPipelineError::Output(error.to_string()))?;
        let bytes = encoded_size.bytes.max(1);
        if bytes > TURN_PIPELINE_BYTE_CAPACITY {
            self.metrics
                .rejected_commands
                .fetch_add(1, Ordering::Relaxed);
            return Err(TurnPipelineError::PayloadTooLarge {
                bytes,
                limit: TURN_PIPELINE_BYTE_CAPACITY,
            });
        }
        let blocked_at = std::time::Instant::now();
        let turn_message = tokio::select! {
            biased;
            () = self.cancellation.cancelled() => return Err(TurnPipelineError::Cancelled),
            permit = Arc::clone(&self.turn_messages).acquire_owned() => {
                permit.map_err(|_| TurnPipelineError::Closed)?
            }
        };
        let turn_bytes = tokio::select! {
            biased;
            () = self.cancellation.cancelled() => return Err(TurnPipelineError::Cancelled),
            permit = Arc::clone(&self.turn_bytes).acquire_many_owned(bytes as u32) => {
                permit.map_err(|_| TurnPipelineError::Closed)?
            }
        };
        let global_bytes = tokio::select! {
            biased;
            () = self.cancellation.cancelled() => return Err(TurnPipelineError::Cancelled),
            permit = Arc::clone(&self.global_bytes).acquire_many_owned(bytes as u32) => {
                permit.map_err(|_| TurnPipelineError::Closed)?
            }
        };
        let (ack, receipt) = oneshot::channel();
        let channel_permit = tokio::select! {
            biased;
            () = self.cancellation.cancelled() => return Err(TurnPipelineError::Cancelled),
            result = self.tx.reserve() => result.map_err(|_| TurnPipelineError::Closed)?,
        };
        self.metrics.blocked_send_nanos.fetch_add(
            blocked_at.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
            Ordering::Relaxed,
        );
        self.admitted(bytes);
        channel_permit.send(PipelineMessage::Emit(PipelineCommand {
            event,
            journal_override,
            admission: PipelineAdmission {
                bytes,
                _turn_message: turn_message,
                _turn_bytes: turn_bytes,
                _global_bytes: global_bytes,
            },
            ack,
        }));
        Ok(receipt)
    }

    /// Forces a semantic coalescing boundary after all previously admitted events.
    pub async fn flush(&self) -> Result<(), TurnPipelineError> {
        let (ack, receipt) = oneshot::channel();
        tokio::select! {
            biased;
            () = self.cancellation.cancelled() => return Err(TurnPipelineError::Cancelled),
            result = self.tx.send(PipelineMessage::Flush(ack)) => {
                result.map_err(|_| TurnPipelineError::Closed)?;
            }
        }
        tokio::select! {
            biased;
            () = self.cancellation.cancelled() => Err(TurnPipelineError::Cancelled),
            result = receipt => result.map_err(|_| TurnPipelineError::Closed)?,
        }
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub fn metrics(&self) -> TurnPipelineMetricsSnapshot {
        self.metrics.snapshot()
    }

    fn admitted(&self, bytes: usize) {
        let messages = self.metrics.queued_messages.fetch_add(1, Ordering::Relaxed) + 1;
        let queued_bytes = self
            .metrics
            .queued_bytes
            .fetch_add(bytes, Ordering::Relaxed)
            + bytes;
        self.metrics
            .message_high_water
            .fetch_max(messages, Ordering::Relaxed);
        self.metrics
            .byte_high_water
            .fetch_max(queued_bytes, Ordering::Relaxed);
    }
}

#[derive(Default)]
struct EncodedSize {
    bytes: usize,
}

impl std::io::Write for EncodedSize {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.bytes = self.bytes.saturating_add(buffer.len());
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

async fn run_pipeline(
    turn_id: String,
    mut seq: u64,
    mut rx: mpsc::Receiver<PipelineMessage>,
    metrics: Arc<TurnPipelineMetrics>,
    output: Arc<impl TurnPipelineOutput>,
    cancellation: CancellationToken,
) {
    let mut deferred = None;
    let mut terminal = false;
    loop {
        let command = match deferred.take() {
            Some(command) => command,
            None => tokio::select! {
                biased;
                () = cancellation.cancelled() => break,
                command = rx.recv() => match command {
                    Some(command) => command,
                    None => break,
                },
            },
        };
        let command = match command {
            PipelineMessage::Emit(command) => command,
            PipelineMessage::Flush(ack) => {
                let _ = ack.send(Ok(()));
                continue;
            }
        };
        if terminal {
            reject_command(command, &metrics, TurnPipelineError::Terminal);
            continue;
        }

        if is_text_event(&command.event) {
            let (event, journal_override, acks, next) =
                coalesce_text(command, &mut rx, &metrics).await;
            deferred = next;
            seq = publish_event(
                &turn_id,
                seq,
                event,
                journal_override,
                acks,
                &metrics,
                &output,
                &cancellation,
            )
            .await;
            continue;
        }

        terminal = command.event.is_terminal();
        seq = publish_event(
            &turn_id,
            seq,
            command.event,
            command.journal_override,
            vec![PipelineReceipt {
                admission: command.admission,
                ack: command.ack,
            }],
            &metrics,
            &output,
            &cancellation,
        )
        .await;
    }
    while let Ok(message) = rx.try_recv() {
        match message {
            PipelineMessage::Emit(command) => {
                reject_command(command, &metrics, TurnPipelineError::Cancelled);
            }
            PipelineMessage::Flush(ack) => {
                let _ = ack.send(Err(TurnPipelineError::Cancelled));
            }
        }
    }
}

async fn coalesce_text(
    first: PipelineCommand,
    rx: &mut mpsc::Receiver<PipelineMessage>,
    metrics: &TurnPipelineMetrics,
) -> (
    TurnPipelineEvent,
    Option<TurnEvent>,
    Vec<PipelineReceipt>,
    Option<PipelineMessage>,
) {
    let mut event = first.event;
    let journal_override = first.journal_override;
    let mut receipts = vec![PipelineReceipt {
        admission: first.admission,
        ack: first.ack,
    }];
    let deadline = tokio::time::Instant::now() + TURN_PIPELINE_COALESCE;
    while let Some(wait) = deadline.checked_duration_since(tokio::time::Instant::now()) {
        let next = match tokio::time::timeout(wait, rx.recv()).await {
            Ok(Some(command)) => command,
            _ => break,
        };
        let PipelineMessage::Emit(next) = next else {
            return (event, journal_override, receipts, Some(next));
        };
        if journal_override.is_none()
            && next.journal_override.is_none()
            && text_events_compatible(&event, &next.event)
            && text_len(&event).saturating_add(text_len(&next.event)) <= TURN_PIPELINE_BATCH_BYTES
        {
            append_text(&mut event, next.event);
            receipts.push(PipelineReceipt {
                admission: next.admission,
                ack: next.ack,
            });
            metrics.coalesced_commands.fetch_add(1, Ordering::Relaxed);
            if receipts.len() >= TURN_PIPELINE_COMMAND_CAPACITY {
                break;
            }
            continue;
        }
        return (
            event,
            journal_override,
            receipts,
            Some(PipelineMessage::Emit(next)),
        );
    }
    (event, journal_override, receipts, None)
}

#[allow(clippy::too_many_arguments)]
async fn publish_event(
    turn_id: &str,
    current_seq: u64,
    event: TurnPipelineEvent,
    journal_override: Option<TurnEvent>,
    receipts: Vec<PipelineReceipt>,
    metrics: &TurnPipelineMetrics,
    output: &Arc<impl TurnPipelineOutput>,
    cancellation: &CancellationToken,
) -> u64 {
    let seq = current_seq.saturating_add(1);
    let envelope = match event {
        TurnPipelineEvent::V2(event) => {
            TurnStreamEnvelopeV2::new(turn_id, seq, chrono::Utc::now(), event)
                .map(TurnPipelineEnvelope::V2)
        }
        TurnPipelineEvent::V3(event) => {
            TurnStreamEnvelopeV3::new(turn_id, seq, chrono::Utc::now(), event)
                .map(TurnPipelineEnvelope::V3)
        }
    };
    let result = match envelope {
        Ok(envelope) => tokio::select! {
            biased;
            () = cancellation.cancelled() => Err(TurnPipelineError::Cancelled),
            published = output.publish(TurnPipelineEmission { envelope, journal_override }) => published.map(|()| seq),
        },
        Err(error) => Err(TurnPipelineError::Output(error.to_string())),
    };
    if result.is_ok() {
        metrics.emitted_events.fetch_add(1, Ordering::Relaxed);
    }
    for receipt in receipts {
        metrics.queued_messages.fetch_sub(1, Ordering::Relaxed);
        metrics
            .queued_bytes
            .fetch_sub(receipt.admission.bytes, Ordering::Relaxed);
        let _ = receipt.ack.send(result.clone());
    }
    if result.is_ok() { seq } else { current_seq }
}

fn reject_command(
    command: PipelineCommand,
    metrics: &TurnPipelineMetrics,
    error: TurnPipelineError,
) {
    let bytes = command.admission.bytes;
    metrics.queued_messages.fetch_sub(1, Ordering::Relaxed);
    metrics.queued_bytes.fetch_sub(bytes, Ordering::Relaxed);
    metrics.rejected_commands.fetch_add(1, Ordering::Relaxed);
    let _ = command.ack.send(Err(error));
}

fn is_text_event(event: &TurnPipelineEvent) -> bool {
    matches!(
        event,
        TurnPipelineEvent::V2(
            TurnStreamEventV2::ContentAppend { .. } | TurnStreamEventV2::ReasoningAppend { .. }
        ) | TurnPipelineEvent::V3(
            TurnStreamEventV3::ContentAppend { .. } | TurnStreamEventV3::ReasoningAppend { .. }
        )
    )
}

fn text_events_compatible(left: &TurnPipelineEvent, right: &TurnPipelineEvent) -> bool {
    matches!(
        (left, right),
        (
            TurnPipelineEvent::V2(TurnStreamEventV2::ContentAppend { .. }),
            TurnPipelineEvent::V2(TurnStreamEventV2::ContentAppend { .. })
        ) | (
            TurnPipelineEvent::V2(TurnStreamEventV2::ReasoningAppend { .. }),
            TurnPipelineEvent::V2(TurnStreamEventV2::ReasoningAppend { .. })
        ) | (
            TurnPipelineEvent::V3(TurnStreamEventV3::ReasoningAppend { .. }),
            TurnPipelineEvent::V3(TurnStreamEventV3::ReasoningAppend { .. })
        )
    ) || matches!(
        (left, right),
        (
            TurnPipelineEvent::V3(TurnStreamEventV3::ContentAppend {
                segment_id: left_id,
                ..
            }),
            TurnPipelineEvent::V3(TurnStreamEventV3::ContentAppend {
                segment_id: right_id,
                ..
            })
        ) if left_id == right_id
    )
}

fn text_len(event: &TurnPipelineEvent) -> usize {
    match event {
        TurnPipelineEvent::V2(
            TurnStreamEventV2::ContentAppend { text } | TurnStreamEventV2::ReasoningAppend { text },
        )
        | TurnPipelineEvent::V3(
            TurnStreamEventV3::ContentAppend { text, .. }
            | TurnStreamEventV3::ReasoningAppend { text },
        ) => text.len(),
        _ => 0,
    }
}

fn append_text(target: &mut TurnPipelineEvent, source: TurnPipelineEvent) {
    match (target, source) {
        (
            TurnPipelineEvent::V2(TurnStreamEventV2::ContentAppend { text }),
            TurnPipelineEvent::V2(TurnStreamEventV2::ContentAppend { text: next }),
        )
        | (
            TurnPipelineEvent::V2(TurnStreamEventV2::ReasoningAppend { text }),
            TurnPipelineEvent::V2(TurnStreamEventV2::ReasoningAppend { text: next }),
        )
        | (
            TurnPipelineEvent::V3(TurnStreamEventV3::ContentAppend { text, .. }),
            TurnPipelineEvent::V3(TurnStreamEventV3::ContentAppend { text: next, .. }),
        )
        | (
            TurnPipelineEvent::V3(TurnStreamEventV3::ReasoningAppend { text }),
            TurnPipelineEvent::V3(TurnStreamEventV3::ReasoningAppend { text: next }),
        ) => text.push_str(&next),
        _ => unreachable!("text compatibility checked before append"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingOutput {
        events: Mutex<Vec<TurnPipelineEnvelope>>,
    }

    impl TurnPipelineOutput for RecordingOutput {
        async fn publish(&self, emission: TurnPipelineEmission) -> Result<(), TurnPipelineError> {
            self.events.lock().unwrap().push(emission.envelope);
            Ok(())
        }
    }

    struct GatedOutput {
        started: tokio::sync::Notify,
        release: Semaphore,
    }

    impl GatedOutput {
        fn new() -> Self {
            Self {
                started: tokio::sync::Notify::new(),
                release: Semaphore::new(0),
            }
        }
    }

    impl TurnPipelineOutput for GatedOutput {
        async fn publish(&self, _emission: TurnPipelineEmission) -> Result<(), TurnPipelineError> {
            self.started.notify_one();
            self.release
                .acquire()
                .await
                .map_err(|_| TurnPipelineError::Closed)?
                .forget();
            Ok(())
        }
    }

    fn content(text: impl Into<String>) -> TurnStreamEventV2 {
        TurnStreamEventV2::ContentAppend { text: text.into() }
    }

    fn status(phase: &str) -> TurnStreamEventV2 {
        TurnStreamEventV2::Status {
            phase: phase.to_string(),
            operator_message: None,
            debug_message: None,
        }
    }

    fn pipeline(output: Arc<impl TurnPipelineOutput>) -> TurnPipelineHandle {
        TurnPipelineHandle::spawn(
            "turn-1",
            0,
            Arc::new(Semaphore::new(TURN_PIPELINE_BYTE_CAPACITY * 4)),
            output,
        )
    }

    #[tokio::test]
    async fn adjacent_fragments_coalesce_and_share_one_sequence() {
        let output = Arc::new(RecordingOutput::default());
        let pipeline = pipeline(Arc::clone(&output));
        let left = {
            let pipeline = pipeline.clone();
            tokio::spawn(async move { pipeline.emit(content("hello ")).await })
        };
        tokio::task::yield_now().await;
        let right = {
            let pipeline = pipeline.clone();
            tokio::spawn(async move { pipeline.emit(content("world")).await })
        };

        assert_eq!(left.await.unwrap().unwrap(), 1);
        assert_eq!(right.await.unwrap().unwrap(), 1);
        let events = output.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            TurnPipelineEnvelope::V2(TurnStreamEnvelopeV2 {
                event: TurnStreamEventV2::ContentAppend { text },
                ..
            }) if text == "hello world"
        ));
        assert_eq!(pipeline.metrics().coalesced_commands, 1);
    }

    #[tokio::test]
    async fn admitted_provider_fragments_batch_before_a_semantic_fence() {
        let output = Arc::new(RecordingOutput::default());
        let pipeline = pipeline(Arc::clone(&output));

        pipeline.admit(content("one")).await.unwrap();
        pipeline.admit(content(" two")).await.unwrap();
        pipeline.flush().await.unwrap();

        let events = output.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            TurnPipelineEnvelope::V2(TurnStreamEnvelopeV2 {
                event: TurnStreamEventV2::ContentAppend { text },
                ..
            }) if text == "one two"
        ));
    }

    #[tokio::test]
    async fn semantic_events_and_flushes_are_coalescing_boundaries() {
        let output = Arc::new(RecordingOutput::default());
        let pipeline = pipeline(Arc::clone(&output));

        assert_eq!(pipeline.emit(content("a")).await.unwrap(), 1);
        assert_eq!(pipeline.emit(status("working")).await.unwrap(), 2);
        assert_eq!(pipeline.emit(content("b")).await.unwrap(), 3);
        pipeline.flush().await.unwrap();
        assert_eq!(pipeline.emit(content("c")).await.unwrap(), 4);

        let events = output.events.lock().unwrap();
        assert_eq!(events.len(), 4);
        assert!(matches!(
            &events[0],
            TurnPipelineEnvelope::V2(TurnStreamEnvelopeV2 {
                event: TurnStreamEventV2::ContentAppend { text },
                ..
            }) if text == "a"
        ));
        assert!(matches!(
            &events[1],
            TurnPipelineEnvelope::V2(TurnStreamEnvelopeV2 {
                event: TurnStreamEventV2::Status { phase, .. },
                ..
            }) if phase == "working"
        ));
        assert!(matches!(
            &events[2],
            TurnPipelineEnvelope::V2(TurnStreamEnvelopeV2 {
                event: TurnStreamEventV2::ContentAppend { text },
                ..
            }) if text == "b"
        ));
        assert!(matches!(
            &events[3],
            TurnPipelineEnvelope::V2(TurnStreamEnvelopeV2 {
                event: TurnStreamEventV2::ContentAppend { text },
                ..
            }) if text == "c"
        ));
    }

    #[tokio::test]
    async fn iso_010_terminal_fences_later_content() {
        let output = Arc::new(GatedOutput::new());
        let pipeline = pipeline(Arc::clone(&output));
        let terminal = {
            let pipeline = pipeline.clone();
            tokio::spawn(async move {
                pipeline
                    .emit(TurnStreamEventV2::Final {
                        text: "done".into(),
                        tool_names: Vec::new(),
                    })
                    .await
            })
        };
        output.started.notified().await;
        let stale = {
            let pipeline = pipeline.clone();
            tokio::spawn(async move { pipeline.emit(content("stale")).await })
        };
        tokio::task::yield_now().await;
        assert!(!stale.is_finished());
        output.release.add_permits(1);

        assert_eq!(terminal.await.unwrap().unwrap(), 1);
        assert_eq!(stale.await.unwrap(), Err(TurnPipelineError::Terminal));
    }

    #[tokio::test]
    async fn bp_001_002_004_stalled_output_is_bounded_and_cancellable() {
        let output = Arc::new(GatedOutput::new());
        let pipeline = pipeline(Arc::clone(&output));
        let first = {
            let pipeline = pipeline.clone();
            tokio::spawn(async move {
                pipeline
                    .emit(content("x".repeat(TURN_PIPELINE_BYTE_CAPACITY * 3 / 4)))
                    .await
            })
        };
        output.started.notified().await;
        let blocked = {
            let pipeline = pipeline.clone();
            tokio::spawn(async move {
                pipeline
                    .emit(content("y".repeat(TURN_PIPELINE_BYTE_CAPACITY / 2)))
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(!blocked.is_finished());
        let snapshot = pipeline.metrics();
        assert!(snapshot.queued_bytes <= TURN_PIPELINE_BYTE_CAPACITY);
        assert!(snapshot.byte_high_water <= TURN_PIPELINE_BYTE_CAPACITY);

        pipeline.cancel();
        assert_eq!(first.await.unwrap(), Err(TurnPipelineError::Cancelled));
        assert_eq!(blocked.await.unwrap(), Err(TurnPipelineError::Cancelled));
    }

    #[tokio::test]
    async fn iso_007_one_stalled_turn_does_not_block_another() {
        let global = Arc::new(Semaphore::new(TURN_PIPELINE_BYTE_CAPACITY * 2));
        let stalled_output = Arc::new(GatedOutput::new());
        let stalled = TurnPipelineHandle::spawn(
            "stalled",
            0,
            Arc::clone(&global),
            Arc::clone(&stalled_output),
        );
        let healthy_output = Arc::new(RecordingOutput::default());
        let healthy = TurnPipelineHandle::spawn("healthy", 0, global, Arc::clone(&healthy_output));
        let stalled_emit = {
            let stalled = stalled.clone();
            tokio::spawn(async move {
                stalled
                    .emit(content("x".repeat(TURN_PIPELINE_BYTE_CAPACITY - 64)))
                    .await
            })
        };
        stalled_output.started.notified().await;

        let healthy_seq =
            tokio::time::timeout(Duration::from_millis(100), healthy.emit(status("working")))
                .await
                .expect("healthy turn was blocked by an unrelated sink")
                .unwrap();
        assert_eq!(healthy_seq, 1);
        stalled.cancel();
        assert_eq!(
            stalled_emit.await.unwrap(),
            Err(TurnPipelineError::Cancelled)
        );
    }

    #[tokio::test]
    async fn oversized_payload_is_rejected_before_admission() {
        let output = Arc::new(RecordingOutput::default());
        let pipeline = pipeline(Arc::clone(&output));
        let error = pipeline
            .emit(content("x".repeat(TURN_PIPELINE_BYTE_CAPACITY + 1)))
            .await
            .unwrap_err();

        assert!(matches!(error, TurnPipelineError::PayloadTooLarge { .. }));
        assert_eq!(pipeline.metrics().queued_messages, 0);
        assert!(output.events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn v3_batches_only_deltas_for_the_same_segment() {
        let output = Arc::new(RecordingOutput::default());
        let pipeline = pipeline(Arc::clone(&output));

        pipeline
            .admit_v3(TurnStreamEventV3::ContentAppend {
                segment_id: "segment-1".into(),
                text: "hello ".into(),
            })
            .await
            .unwrap();
        pipeline
            .admit_v3(TurnStreamEventV3::ContentAppend {
                segment_id: "segment-1".into(),
                text: "world".into(),
            })
            .await
            .unwrap();
        pipeline
            .emit_v3(TurnStreamEventV3::ContentAppend {
                segment_id: "segment-2".into(),
                text: "separate".into(),
            })
            .await
            .unwrap();

        let events = output.events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            TurnPipelineEnvelope::V3(TurnStreamEnvelopeV3 {
                event: TurnStreamEventV3::ContentAppend { segment_id, text },
                ..
            }) if segment_id == "segment-1" && text == "hello world"
        ));
        assert!(matches!(
            &events[1],
            TurnPipelineEnvelope::V3(TurnStreamEnvelopeV3 {
                event: TurnStreamEventV3::ContentAppend { segment_id, text },
                ..
            }) if segment_id == "segment-2" && text == "separate"
        ));
    }

    #[tokio::test]
    async fn v3_native_facts_receive_one_cursor_each_in_observation_order() {
        let output = Arc::new(RecordingOutput::default());
        let pipeline = pipeline(Arc::clone(&output));

        assert_eq!(
            pipeline
                .emit_v3(TurnStreamEventV3::AssistantTextStarted {
                    segment_id: "segment-1".into(),
                    model_round: 1,
                })
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            pipeline
                .emit_v3(TurnStreamEventV3::ContentAppend {
                    segment_id: "segment-1".into(),
                    text: "I’ll check.".into(),
                })
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            pipeline
                .emit_v3(TurnStreamEventV3::AssistantTextCommitted {
                    segment_id: "segment-1".into(),
                })
                .await
                .unwrap(),
            3
        );
        assert_eq!(
            pipeline
                .emit_v3(TurnStreamEventV3::ToolStarted {
                    tool_run_id: "run-1".into(),
                    tool_name: "search".into(),
                    input_summary: "query".into(),
                    input_params: Vec::new(),
                    tool_round: 1,
                })
                .await
                .unwrap(),
            4
        );
        assert_eq!(
            pipeline
                .emit_v3(TurnStreamEventV3::TurnCompleted {
                    outcome: medousa_types::TurnCompletionOutcomeV3::Completed,
                    aggregate_text: "I’ll check.".into(),
                    tool_names: vec!["search".into()],
                })
                .await
                .unwrap(),
            5
        );

        let events = output.events.lock().unwrap();
        assert_eq!(
            events
                .iter()
                .map(TurnPipelineEnvelope::seq)
                .collect::<Vec<_>>(),
            [1, 2, 3, 4, 5]
        );
        assert!(
            events
                .iter()
                .all(|event| matches!(event, TurnPipelineEnvelope::V3(_)))
        );
    }
}
