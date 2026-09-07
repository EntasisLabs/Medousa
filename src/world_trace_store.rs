//! Bounded durable causal ledger for daemon-governed worlds.
//!
//! The pure `medousa-world` crate owns admission semantics. This host-side
//! store appends the resulting envelopes before an admitted driver operation
//! can proceed. An admission without a later terminal receipt is promoted to
//! an explicit interrupted record on the next daemon start; it is never
//! replayed with stale authority.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use medousa_types::{
    WORLD_EVIDENCE_SCHEMA_VERSION, WorldEvidenceRecord, WorldTimelineCheckpoint,
    WorldTimelineRecovery,
};
use medousa_world::{
    WorldActionStatus, WorldEffectClass, WorldEvent, WorldEventEnvelope, WorldEventKind, WorldId,
    WorldIntentId, WorldOwnership, WorldPrincipalKind, WorldRecoveryPlan, WorldRecoveryStrategy,
    WorldSurfaceKind,
};
use serde::{Deserialize, Serialize};

pub const WORLD_TIMELINE_SCHEMA_VERSION: u16 = 1;
const DEFAULT_TIMELINE_CAPACITY: usize = 8_192;
const DEFAULT_TIMELINE_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TIMELINE_LINE_BYTES: usize = 256 * 1024;
const DEFAULT_EVIDENCE_CAPACITY: usize = 1_024;
const DEFAULT_EVIDENCE_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_EVIDENCE_LINE_BYTES: usize = 64 * 1024;
const DEFAULT_TELEMETRY_CAPACITY: usize = 2_048;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableWorldEvent {
    pub schema_version: u16,
    /// Monotonic across every world owned by this workshop daemon.
    pub ledger_sequence: u64,
    pub recorded_at_ms: u64,
    pub envelope: WorldEventEnvelope,
}

/// Process-local timing sample. The bounded ring is intentionally raw and
/// ephemeral; only selected, payload-free evidence is promoted to disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldTelemetrySample {
    pub ledger_sequence: u64,
    pub event_type: String,
    pub effect_class: Option<String>,
    pub status: Option<String>,
    pub action_elapsed_ms: Option<u64>,
    pub durability_latency_us: u64,
}

#[derive(Debug)]
pub struct WorldTraceStore {
    path: PathBuf,
    evidence_path: PathBuf,
    events: VecDeque<DurableWorldEvent>,
    evidence: VecDeque<WorldEvidenceRecord>,
    telemetry: VecDeque<WorldTelemetrySample>,
    next_sequence: u64,
    capacity: usize,
    max_file_bytes: u64,
    evidence_capacity: usize,
    evidence_max_file_bytes: u64,
    telemetry_capacity: usize,
}

impl WorldTraceStore {
    pub fn open(path: impl Into<PathBuf>, now_ms: u64) -> Result<(Self, usize), String> {
        Self::open_with_limits(
            path.into(),
            now_ms,
            DEFAULT_TIMELINE_CAPACITY,
            DEFAULT_TIMELINE_FILE_BYTES,
        )
    }

    fn open_with_limits(
        path: PathBuf,
        now_ms: u64,
        capacity: usize,
        max_file_bytes: u64,
    ) -> Result<(Self, usize), String> {
        let capacity = capacity.max(1);
        let mut events = VecDeque::new();
        let mut next_sequence = 1_u64;
        let mut needs_compaction = false;
        match File::open(&path) {
            Ok(file) => {
                let mut reader = BufReader::new(file);
                loop {
                    let mut raw = Vec::new();
                    let mut bounded = reader
                        .by_ref()
                        .take((MAX_TIMELINE_LINE_BYTES + 2) as u64);
                    let read = bounded.read_until(b'\n', &mut raw).map_err(|error| {
                        format!("read world causal timeline {}: {error}", path.display())
                    })?;
                    if read == 0 {
                        break;
                    }
                    let terminated = raw.last() == Some(&b'\n');
                    if terminated {
                        raw.pop();
                        if raw.last() == Some(&b'\r') {
                            raw.pop();
                        }
                    }
                    if raw.iter().all(u8::is_ascii_whitespace) {
                        continue;
                    }
                    if raw.len() > MAX_TIMELINE_LINE_BYTES {
                        return Err(format!(
                            "world causal timeline record exceeds {} bytes",
                            MAX_TIMELINE_LINE_BYTES
                        ));
                    }
                    let record = match serde_json::from_slice::<DurableWorldEvent>(&raw) {
                        Ok(record) => record,
                        // A process can die between writing a record and its
                        // trailing newline. Ignore only that incomplete tail.
                        Err(_) if !terminated => {
                            needs_compaction = true;
                            break;
                        }
                        Err(error) => {
                            return Err(format!(
                                "decode world causal timeline {}: {error}",
                                path.display()
                            ));
                        }
                    };
                    validate_record(&record, next_sequence)?;
                    next_sequence = record.ledger_sequence.saturating_add(1);
                    events.push_back(record);
                    if events.len() > capacity {
                        events.pop_front();
                        needs_compaction = true;
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "open world causal timeline {}: {error}",
                    path.display()
                ));
            }
        }

        let evidence_path = evidence_path_for(&path);
        let (evidence, evidence_needs_compaction) =
            load_evidence(&evidence_path, DEFAULT_EVIDENCE_CAPACITY)?;
        let mut store = Self {
            path,
            evidence_path,
            events,
            evidence,
            telemetry: VecDeque::new(),
            next_sequence,
            capacity,
            max_file_bytes: max_file_bytes.max(1),
            evidence_capacity: DEFAULT_EVIDENCE_CAPACITY,
            evidence_max_file_bytes: DEFAULT_EVIDENCE_FILE_BYTES,
            telemetry_capacity: DEFAULT_TELEMETRY_CAPACITY,
        };
        if needs_compaction
            || fs::metadata(&store.path)
                .map(|metadata| metadata.len() > store.max_file_bytes)
                .unwrap_or(false)
        {
            store.compact()?;
        }
        if evidence_needs_compaction
            || fs::metadata(&store.evidence_path)
                .map(|metadata| metadata.len() > store.evidence_max_file_bytes)
                .unwrap_or(false)
        {
            store.compact_evidence()?;
        }
        store.repair_evidence_from_timeline()?;
        let recovered = store.recover_interrupted(now_ms)?;
        Ok((store, recovered))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn evidence_path(&self) -> &Path {
        &self.evidence_path
    }

    pub fn events_after(&self, sequence: u64, limit: usize) -> Vec<DurableWorldEvent> {
        self.events
            .iter()
            .filter(|record| record.ledger_sequence > sequence)
            .take(limit.clamp(1, 1_000))
            .cloned()
            .collect()
    }

    pub fn events_for_trace(&self, trace_id: &str) -> Vec<DurableWorldEvent> {
        self.events
            .iter()
            .filter(|record| {
                record
                    .envelope
                    .event
                    .trace_id
                    .as_ref()
                    .is_some_and(|candidate| candidate.as_str() == trace_id)
            })
            .cloned()
            .collect()
    }

    pub fn evidence_after(&self, sequence: u64, limit: usize) -> Vec<WorldEvidenceRecord> {
        self.evidence
            .iter()
            .filter(|record| record.ledger_sequence > sequence)
            .take(limit.clamp(1, 1_000))
            .cloned()
            .collect()
    }

    #[cfg(test)]
    fn telemetry_samples(&self) -> &VecDeque<WorldTelemetrySample> {
        &self.telemetry
    }

    pub fn append_envelopes(
        &mut self,
        envelopes: Vec<WorldEventEnvelope>,
        recorded_at_ms: u64,
    ) -> Result<(), String> {
        if envelopes.is_empty() {
            return Ok(());
        }
        let durability_started = Instant::now();
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "create world causal timeline directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| {
                format!("open world causal timeline {}: {error}", self.path.display())
            })?;
        let mut appended = Vec::with_capacity(envelopes.len());
        for envelope in envelopes {
            let record = DurableWorldEvent {
                schema_version: WORLD_TIMELINE_SCHEMA_VERSION,
                ledger_sequence: self.next_sequence,
                recorded_at_ms,
                envelope,
            };
            let raw = serde_json::to_vec(&record)
                .map_err(|error| format!("encode world causal timeline record: {error}"))?;
            if raw.len() > MAX_TIMELINE_LINE_BYTES {
                return Err(format!(
                    "world causal timeline record exceeds {} bytes",
                    MAX_TIMELINE_LINE_BYTES
                ));
            }
            file.write_all(&raw)
                .and_then(|_| file.write_all(b"\n"))
                .map_err(|error| {
                    format!("append world causal timeline {}: {error}", self.path.display())
                })?;
            self.next_sequence = self.next_sequence.saturating_add(1);
            appended.push(record);
        }
        // The action admission must reach disk before the driver receives its
        // permit. One sync covers every event produced by the kernel mutation.
        file.sync_data().map_err(|error| {
            format!("sync world causal timeline {}: {error}", self.path.display())
        })?;
        drop(file);

        let durability_latency_us =
            u64::try_from(durability_started.elapsed().as_micros()).unwrap_or(u64::MAX);
        let telemetry = appended
            .iter()
            .map(|record| {
                telemetry_sample(
                    record,
                    action_elapsed_ms(&self.events, &appended, record),
                    durability_latency_us,
                )
            })
            .collect::<Vec<_>>();
        let evidence = appended
            .iter()
            .filter_map(|record| {
                promoted_evidence(
                    record,
                    action_elapsed_ms(&self.events, &appended, record),
                    Some(durability_latency_us),
                )
            })
            .collect::<Vec<_>>();

        self.events.extend(appended);
        while self.events.len() > self.capacity {
            self.events.pop_front();
        }
        self.telemetry.extend(telemetry);
        while self.telemetry.len() > self.telemetry_capacity {
            self.telemetry.pop_front();
        }
        self.append_evidence(evidence)?;
        if fs::metadata(&self.path)
            .map(|metadata| metadata.len() > self.max_file_bytes)
            .unwrap_or(false)
        {
            self.compact()?;
        }
        Ok(())
    }

    fn append_evidence(&mut self, evidence: Vec<WorldEvidenceRecord>) -> Result<(), String> {
        if evidence.is_empty() {
            return Ok(());
        }
        if let Some(parent) = self.evidence_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "create world evidence directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.evidence_path)
            .map_err(|error| {
                format!(
                    "open world evidence {}: {error}",
                    self.evidence_path.display()
                )
            })?;
        for record in &evidence {
            let raw = serde_json::to_vec(record)
                .map_err(|error| format!("encode world evidence record: {error}"))?;
            if raw.len() > MAX_EVIDENCE_LINE_BYTES {
                return Err(format!(
                    "world evidence record exceeds {} bytes",
                    MAX_EVIDENCE_LINE_BYTES
                ));
            }
            file.write_all(&raw)
                .and_then(|_| file.write_all(b"\n"))
                .map_err(|error| {
                    format!(
                        "append world evidence {}: {error}",
                        self.evidence_path.display()
                    )
                })?;
        }
        file.sync_data().map_err(|error| {
            format!(
                "sync world evidence {}: {error}",
                self.evidence_path.display()
            )
        })?;
        drop(file);

        self.evidence.extend(evidence);
        while self.evidence.len() > self.evidence_capacity {
            self.evidence.pop_front();
        }
        if fs::metadata(&self.evidence_path)
            .map(|metadata| metadata.len() > self.evidence_max_file_bytes)
            .unwrap_or(false)
        {
            self.compact_evidence()?;
        }
        Ok(())
    }

    fn repair_evidence_from_timeline(&mut self) -> Result<(), String> {
        let last_evidence_sequence = self
            .evidence
            .back()
            .map(|record| record.ledger_sequence)
            .unwrap_or_default();
        let repaired = self
            .events
            .iter()
            .filter(|record| record.ledger_sequence > last_evidence_sequence)
            .filter_map(|record| {
                promoted_evidence(
                    record,
                    action_elapsed_ms(&self.events, &[], record),
                    None,
                )
            })
            .collect();
        self.append_evidence(repaired)
    }

    fn recover_interrupted(&mut self, now_ms: u64) -> Result<usize, String> {
        let mut pending = HashMap::<(WorldId, WorldIntentId), DurableWorldEvent>::new();
        let mut source_sequences = BTreeMap::<WorldId, u64>::new();
        for record in &self.events {
            let event = &record.envelope.event;
            source_sequences
                .entry(event.world_id.clone())
                .and_modify(|sequence| *sequence = (*sequence).max(event.sequence))
                .or_insert(event.sequence);
            let Some(intent_id) = event.intent_id.clone() else {
                continue;
            };
            let key = (event.world_id.clone(), intent_id);
            match &event.event {
                WorldEventKind::ActionAdmitted { .. } => {
                    pending.insert(key, record.clone());
                }
                WorldEventKind::ActionCommitted { .. }
                | WorldEventKind::ActionFailed { .. }
                | WorldEventKind::ActionInterrupted { .. } => {
                    pending.remove(&key);
                }
                _ => {}
            }
        }
        let mut interrupted = pending.into_values().collect::<Vec<_>>();
        interrupted.sort_by_key(|record| record.ledger_sequence);
        let recovered = interrupted.len();
        let envelopes = interrupted
            .into_iter()
            .filter_map(|admission| {
                let WorldEventKind::ActionAdmitted {
                    effect_class,
                    checkpoint,
                    recovery,
                    ..
                } = &admission.envelope.event.event
                else {
                    return None;
                };
                let event = &admission.envelope.event;
                let sequence = source_sequences
                    .entry(event.world_id.clone())
                    .and_modify(|sequence| *sequence = sequence.saturating_add(1))
                    .or_insert(1);
                let status = interrupted_status(*effect_class);
                Some(WorldEventEnvelope {
                    schema_version: admission.envelope.schema_version,
                    authority_id: admission.envelope.authority_id,
                    driver_id: admission.envelope.driver_id,
                    ownership: admission.envelope.ownership,
                    surface: admission.envelope.surface,
                    event: WorldEvent {
                        sequence: *sequence,
                        world_id: event.world_id.clone(),
                        world_revision: event.world_revision,
                        at_ms: now_ms,
                        principal: event.principal.clone(),
                        resource_id: event.resource_id.clone(),
                        intent_id: event.intent_id.clone(),
                        trace_id: event.trace_id.clone(),
                        event: WorldEventKind::ActionInterrupted {
                            effect_class: *effect_class,
                            status,
                            summary: "daemon restarted before a terminal driver receipt; the old permit was discarded"
                                .to_string(),
                            checkpoint: checkpoint.clone(),
                            recovery: recovery.clone(),
                        },
                    },
                })
            })
            .collect();
        self.append_envelopes(envelopes, now_ms)?;
        Ok(recovered)
    }

    fn compact(&mut self) -> Result<(), String> {
        let mut raw = Vec::new();
        for record in &self.events {
            serde_json::to_writer(&mut raw, record)
                .map_err(|error| format!("encode compacted world causal timeline: {error}"))?;
            raw.push(b'\n');
        }
        crate::session::atomic_write(&self.path, &raw).map_err(|error| {
            format!("compact world causal timeline {}: {error}", self.path.display())
        })
    }

    fn compact_evidence(&mut self) -> Result<(), String> {
        let mut raw = Vec::new();
        for record in &self.evidence {
            serde_json::to_writer(&mut raw, record)
                .map_err(|error| format!("encode compacted world evidence: {error}"))?;
            raw.push(b'\n');
        }
        crate::session::atomic_write(&self.evidence_path, &raw).map_err(|error| {
            format!(
                "compact world evidence {}: {error}",
                self.evidence_path.display()
            )
        })
    }
}

fn evidence_path_for(timeline_path: &Path) -> PathBuf {
    timeline_path.with_extension("evidence.jsonl")
}

fn load_evidence(
    path: &Path,
    capacity: usize,
) -> Result<(VecDeque<WorldEvidenceRecord>, bool), String> {
    let mut evidence = VecDeque::new();
    let mut needs_compaction = false;
    let mut last_sequence = 0_u64;
    match File::open(path) {
        Ok(file) => {
            let mut reader = BufReader::new(file);
            loop {
                let mut raw = Vec::new();
                let mut bounded = reader.by_ref().take((MAX_EVIDENCE_LINE_BYTES + 2) as u64);
                let read = bounded
                    .read_until(b'\n', &mut raw)
                    .map_err(|error| format!("read world evidence {}: {error}", path.display()))?;
                if read == 0 {
                    break;
                }
                let terminated = raw.last() == Some(&b'\n');
                if terminated {
                    raw.pop();
                    if raw.last() == Some(&b'\r') {
                        raw.pop();
                    }
                }
                if raw.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                if raw.len() > MAX_EVIDENCE_LINE_BYTES {
                    return Err(format!(
                        "world evidence record exceeds {} bytes",
                        MAX_EVIDENCE_LINE_BYTES
                    ));
                }
                let record = match serde_json::from_slice::<WorldEvidenceRecord>(&raw) {
                    Ok(record) => record,
                    Err(_) if !terminated => {
                        needs_compaction = true;
                        break;
                    }
                    Err(error) => {
                        return Err(format!("decode world evidence {}: {error}", path.display()));
                    }
                };
                if record.schema_version != WORLD_EVIDENCE_SCHEMA_VERSION {
                    return Err(format!(
                        "unsupported world evidence schema {}",
                        record.schema_version
                    ));
                }
                if record.ledger_sequence == 0 || record.ledger_sequence <= last_sequence {
                    return Err("world evidence sequence must increase monotonically".to_string());
                }
                last_sequence = record.ledger_sequence;
                evidence.push_back(record);
                if evidence.len() > capacity.max(1) {
                    evidence.pop_front();
                    needs_compaction = true;
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!("open world evidence {}: {error}", path.display()));
        }
    }
    Ok((evidence, needs_compaction))
}

fn action_elapsed_ms(
    existing: &VecDeque<DurableWorldEvent>,
    appended: &[DurableWorldEvent],
    terminal: &DurableWorldEvent,
) -> Option<u64> {
    if !matches!(
        terminal.envelope.event.event,
        WorldEventKind::ActionCommitted { .. }
            | WorldEventKind::ActionFailed { .. }
            | WorldEventKind::ActionInterrupted { .. }
    ) {
        return None;
    }
    let intent_id = terminal.envelope.event.intent_id.as_ref()?;
    appended
        .iter()
        .rev()
        .chain(existing.iter().rev())
        .find_map(|candidate| {
            if candidate.envelope.event.world_id != terminal.envelope.event.world_id
                || candidate.envelope.event.intent_id.as_ref() != Some(intent_id)
            {
                return None;
            }
            match &candidate.envelope.event.event {
                WorldEventKind::ActionAdmitted { checkpoint, .. } => Some(
                    terminal
                        .envelope
                        .event
                        .at_ms
                        .saturating_sub(checkpoint.admitted_at_ms),
                ),
                _ => None,
            }
        })
}

fn telemetry_sample(
    record: &DurableWorldEvent,
    action_elapsed_ms: Option<u64>,
    durability_latency_us: u64,
) -> WorldTelemetrySample {
    let details = evidence_event_details(&record.envelope.event.event);
    WorldTelemetrySample {
        ledger_sequence: record.ledger_sequence,
        event_type: details.event_type.to_string(),
        effect_class: details
            .effect_class
            .map(effect_class_label)
            .map(str::to_string),
        status: details.status.map(str::to_string),
        action_elapsed_ms,
        durability_latency_us,
    }
}

fn promoted_evidence(
    record: &DurableWorldEvent,
    action_elapsed_ms: Option<u64>,
    durability_latency_us: Option<u64>,
) -> Option<WorldEvidenceRecord> {
    let event = &record.envelope.event;
    let details = evidence_event_details(&event.event);
    let mut reasons = Vec::new();
    if details.effect_class.is_some_and(|effect| {
        matches!(
            effect,
            WorldEffectClass::ExternalEffect | WorldEffectClass::Irreversible
        )
    }) {
        reasons.push("sensitive_effect".to_string());
    }
    match &event.event {
        WorldEventKind::ActionAdmitted {
            recipe_hint: Some(hint),
            ..
        } if hint
            .operations
            .iter()
            .any(|operation| operation.requires_operator_confirmation) =>
        {
            reasons.push("operator_confirmation".to_string());
        }
        WorldEventKind::ActionFailed { .. } => reasons.push("failure".to_string()),
        WorldEventKind::ActionCommitted { status, .. }
            if *status != WorldActionStatus::Confirmed =>
        {
            reasons.push("uncertain_outcome".to_string());
        }
        WorldEventKind::ActionInterrupted { .. } => {
            reasons.push("interrupted".to_string());
            reasons.push("uncertain_outcome".to_string());
        }
        _ => {}
    }
    if reasons.is_empty() {
        return None;
    }
    Some(WorldEvidenceRecord {
        schema_version: WORLD_EVIDENCE_SCHEMA_VERSION,
        evidence_id: format!("world-evidence:{}", record.ledger_sequence),
        ledger_sequence: record.ledger_sequence,
        promoted_at_ms: record.recorded_at_ms,
        world_id: event.world_id.to_string(),
        authority_id: record.envelope.authority_id.to_string(),
        driver_id: record.envelope.driver_id.to_string(),
        ownership: ownership_label(record.envelope.ownership).to_string(),
        surface: surface_label(record.envelope.surface).to_string(),
        principal_id: event
            .principal
            .as_ref()
            .map(|principal| principal.principal_id.to_string()),
        principal_kind: event
            .principal
            .as_ref()
            .map(|principal| principal_kind_label(principal.kind).to_string()),
        resource_id: event.resource_id.as_ref().map(ToString::to_string),
        intent_id: event.intent_id.as_ref().map(ToString::to_string),
        trace_id: event.trace_id.as_ref().map(ToString::to_string),
        event_type: details.event_type.to_string(),
        effect_class: details
            .effect_class
            .map(effect_class_label)
            .map(str::to_string),
        status: details.status.map(str::to_string),
        reasons,
        action_elapsed_ms,
        durability_latency_us,
        checkpoint: details.checkpoint.map(public_checkpoint),
        recovery: details.recovery.map(public_recovery),
    })
}

struct EvidenceEventDetails<'a> {
    event_type: &'static str,
    effect_class: Option<WorldEffectClass>,
    status: Option<&'static str>,
    checkpoint: Option<&'a medousa_world::WorldActionCheckpoint>,
    recovery: Option<&'a WorldRecoveryPlan>,
}

fn evidence_event_details(event: &WorldEventKind) -> EvidenceEventDetails<'_> {
    match event {
        WorldEventKind::ActionAdmitted {
            effect_class,
            checkpoint,
            recovery,
            ..
        } => EvidenceEventDetails {
            event_type: "action_admitted",
            effect_class: Some(*effect_class),
            status: Some("admitted"),
            checkpoint: Some(checkpoint),
            recovery: Some(recovery),
        },
        WorldEventKind::ActionCommitted {
            effect_class,
            status,
            recovery,
            ..
        } => EvidenceEventDetails {
            event_type: "action_committed",
            effect_class: Some(*effect_class),
            status: Some(action_status_label(*status)),
            checkpoint: None,
            recovery: recovery.as_ref(),
        },
        WorldEventKind::ActionFailed { effect_class, .. } => EvidenceEventDetails {
            event_type: "action_failed",
            effect_class: Some(*effect_class),
            status: Some("failed"),
            checkpoint: None,
            recovery: None,
        },
        WorldEventKind::ActionInterrupted {
            effect_class,
            status,
            checkpoint,
            recovery,
            ..
        } => EvidenceEventDetails {
            event_type: "action_interrupted",
            effect_class: Some(*effect_class),
            status: Some(action_status_label(*status)),
            checkpoint: Some(checkpoint),
            recovery: Some(recovery),
        },
        WorldEventKind::WorldCreated { .. } => EvidenceEventDetails {
            event_type: "world_created",
            effect_class: None,
            status: None,
            checkpoint: None,
            recovery: None,
        },
        WorldEventKind::CapabilityGranted { .. } => EvidenceEventDetails {
            event_type: "capability_granted",
            effect_class: None,
            status: None,
            checkpoint: None,
            recovery: None,
        },
        WorldEventKind::CapabilityRevoked { .. } => EvidenceEventDetails {
            event_type: "capability_revoked",
            effect_class: None,
            status: None,
            checkpoint: None,
            recovery: None,
        },
        WorldEventKind::ControlAcquired { .. } => EvidenceEventDetails {
            event_type: "control_acquired",
            effect_class: None,
            status: None,
            checkpoint: None,
            recovery: None,
        },
        WorldEventKind::ControlReleased { .. } => EvidenceEventDetails {
            event_type: "control_released",
            effect_class: None,
            status: None,
            checkpoint: None,
            recovery: None,
        },
        WorldEventKind::ExternalMutationObserved { .. } => EvidenceEventDetails {
            event_type: "external_mutation_observed",
            effect_class: None,
            status: None,
            checkpoint: None,
            recovery: None,
        },
    }
}

fn public_checkpoint(checkpoint: &medousa_world::WorldActionCheckpoint) -> WorldTimelineCheckpoint {
    WorldTimelineCheckpoint {
        surface: surface_label(checkpoint.surface).to_string(),
        world_revision: checkpoint.world_revision,
        control_generation: checkpoint.control_generation,
        admitted_at_ms: checkpoint.admitted_at_ms,
        permit_expires_at_ms: checkpoint.permit_expires_at_ms,
    }
}

fn public_recovery(recovery: &WorldRecoveryPlan) -> WorldTimelineRecovery {
    WorldTimelineRecovery {
        strategy: recovery_strategy_label(recovery.strategy).to_string(),
        requires_fresh_admission: recovery.requires_fresh_admission,
    }
}

fn ownership_label(ownership: WorldOwnership) -> &'static str {
    match ownership {
        WorldOwnership::Owned => "owned",
        WorldOwnership::Managed => "managed",
        WorldOwnership::Attached => "attached",
    }
}

fn surface_label(surface: WorldSurfaceKind) -> &'static str {
    match surface {
        WorldSurfaceKind::Browser => "browser",
        WorldSurfaceKind::Desktop => "desktop",
        WorldSurfaceKind::Application => "application",
        WorldSurfaceKind::Terminal => "terminal",
        WorldSurfaceKind::Composite => "composite",
    }
}

fn principal_kind_label(kind: WorldPrincipalKind) -> &'static str {
    match kind {
        WorldPrincipalKind::Human => "human",
        WorldPrincipalKind::Agent => "agent",
        WorldPrincipalKind::Bot => "bot",
        WorldPrincipalKind::Worker => "worker",
        WorldPrincipalKind::Peer => "peer",
        WorldPrincipalKind::System => "system",
    }
}

fn effect_class_label(effect: WorldEffectClass) -> &'static str {
    match effect {
        WorldEffectClass::Observe => "observe",
        WorldEffectClass::ObservePixels => "observe_pixels",
        WorldEffectClass::LocalReversible => "local_reversible",
        WorldEffectClass::LocalMutation => "local_mutation",
        WorldEffectClass::ExternalEffect => "external_effect",
        WorldEffectClass::Irreversible => "irreversible",
    }
}

fn action_status_label(status: WorldActionStatus) -> &'static str {
    match status {
        WorldActionStatus::Confirmed => "confirmed",
        WorldActionStatus::NeedsReconciliation => "needs_reconciliation",
        WorldActionStatus::Indeterminate => "indeterminate",
    }
}

fn recovery_strategy_label(strategy: WorldRecoveryStrategy) -> &'static str {
    match strategy {
        WorldRecoveryStrategy::Reobserve => "reobserve",
        WorldRecoveryStrategy::ReconcileFromFreshObservation => {
            "reconcile_from_fresh_observation"
        }
        WorldRecoveryStrategy::OperatorReview => "operator_review",
    }
}

fn interrupted_status(effect_class: WorldEffectClass) -> WorldActionStatus {
    match effect_class {
        WorldEffectClass::Observe | WorldEffectClass::ObservePixels => {
            WorldActionStatus::NeedsReconciliation
        }
        WorldEffectClass::LocalReversible
        | WorldEffectClass::LocalMutation
        | WorldEffectClass::ExternalEffect
        | WorldEffectClass::Irreversible => WorldActionStatus::Indeterminate,
    }
}

fn validate_record(record: &DurableWorldEvent, next_sequence: u64) -> Result<(), String> {
    if record.schema_version != WORLD_TIMELINE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported world causal timeline schema {}",
            record.schema_version
        ));
    }
    if record.ledger_sequence < next_sequence {
        return Err("world causal timeline sequence moved backwards".to_string());
    }
    if record.ledger_sequence == 0 {
        return Err("world causal timeline sequence must be positive".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_world::{
        WORLD_ACTION_CHECKPOINT_SCHEMA_VERSION, WORLD_EVENT_ENVELOPE_SCHEMA_VERSION,
        WorldActionCheckpoint, WorldAuthorityId, WorldDriverId, WorldOwnership, WorldPrincipal,
        WorldRecoveryPlan, WorldRecoveryStrategy, WorldResourceId, WorldSurfaceKind, WorldTraceId,
    };

    fn admission(intent: &str, effect_class: WorldEffectClass) -> WorldEventEnvelope {
        let checkpoint = WorldActionCheckpoint {
            schema_version: WORLD_ACTION_CHECKPOINT_SCHEMA_VERSION,
            surface: WorldSurfaceKind::Browser,
            world_revision: 4,
            control_generation: Some(2),
            admitted_at_ms: 10,
            permit_expires_at_ms: 20,
        };
        WorldEventEnvelope {
            schema_version: WORLD_EVENT_ENVELOPE_SCHEMA_VERSION,
            authority_id: WorldAuthorityId::new("workshop:test"),
            driver_id: WorldDriverId::new("driver:test"),
            ownership: WorldOwnership::Owned,
            surface: WorldSurfaceKind::Browser,
            event: WorldEvent {
                sequence: 1,
                world_id: WorldId::new("world:test"),
                world_revision: 4,
                at_ms: 10,
                principal: Some(WorldPrincipal::agent("agent:test")),
                resource_id: Some(WorldResourceId::new("browser-tab:test")),
                intent_id: Some(WorldIntentId::new(intent)),
                trace_id: Some(WorldTraceId::new(format!("trace:{intent}"))),
                event: WorldEventKind::ActionAdmitted {
                    grant_id: medousa_world::WorldGrantId::new("grant:test"),
                    effect_class,
                    summary: "click continue".to_string(),
                    checkpoint,
                    recovery: WorldRecoveryPlan {
                        strategy: effect_class.recovery_strategy(),
                        requires_fresh_admission: true,
                    },
                    recipe_hint: None,
                },
            },
        }
    }

    fn terminal(
        intent: &str,
        effect_class: WorldEffectClass,
        event: WorldEventKind,
        at_ms: u64,
    ) -> WorldEventEnvelope {
        let mut envelope = admission(intent, effect_class);
        envelope.event.sequence = 2;
        envelope.event.at_ms = at_ms;
        envelope.event.event = event;
        envelope
    }

    #[test]
    fn startup_promotes_unmatched_mutation_to_indeterminate_without_replay() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("timeline.jsonl");
        let (mut store, recovered) = WorldTraceStore::open(&path, 10).unwrap();
        assert_eq!(recovered, 0);
        store
            .append_envelopes(
                vec![admission("intent:crashed", WorldEffectClass::LocalMutation)],
                10,
            )
            .unwrap();
        drop(store);

        let (store, recovered) = WorldTraceStore::open(&path, 30).unwrap();
        assert_eq!(recovered, 1);
        let events = store.events_after(0, 10);
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[1].envelope.event.event,
            WorldEventKind::ActionInterrupted {
                status: WorldActionStatus::Indeterminate,
                recovery: WorldRecoveryPlan {
                    strategy: WorldRecoveryStrategy::ReconcileFromFreshObservation,
                    requires_fresh_admission: true,
                },
                ..
            }
        ));
    }

    #[test]
    fn bounded_ledger_compacts_without_reusing_global_sequence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("timeline.jsonl");
        let (mut store, _) =
            WorldTraceStore::open_with_limits(path.clone(), 10, 3, 1).unwrap();
        for index in 0..5 {
            let mut envelope = admission(
                &format!("intent:{index}"),
                WorldEffectClass::Observe,
            );
            envelope.event.intent_id = None;
            envelope.event.event = WorldEventKind::ExternalMutationObserved {
                summary: format!("event {index}"),
            };
            store.append_envelopes(vec![envelope], 10 + index).unwrap();
        }
        assert_eq!(
            store
                .events_after(0, 10)
                .iter()
                .map(|record| record.ledger_sequence)
                .collect::<Vec<_>>(),
            vec![3, 4, 5]
        );
        drop(store);

        let (store, recovered) =
            WorldTraceStore::open_with_limits(path, 30, 3, 1).unwrap();
        assert_eq!(recovered, 0);
        assert_eq!(
            store.events_after(0, 10).last().unwrap().ledger_sequence,
            5
        );
    }

    #[test]
    fn startup_discards_only_a_malformed_unterminated_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("timeline.jsonl");
        let (mut store, _) = WorldTraceStore::open(&path, 10).unwrap();
        let mut envelope = admission("intent:complete", WorldEffectClass::Observe);
        envelope.event.intent_id = None;
        envelope.event.event = WorldEventKind::ExternalMutationObserved {
            summary: "complete event".to_string(),
        };
        store.append_envelopes(vec![envelope], 10).unwrap();
        drop(store);

        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"schema_version\":1").unwrap();
        file.sync_data().unwrap();
        drop(file);

        let (store, recovered) = WorldTraceStore::open(&path, 20).unwrap();
        assert_eq!(recovered, 0);
        assert_eq!(store.events_after(0, 10).len(), 1);
        let compacted = fs::read_to_string(path).unwrap();
        assert!(compacted.ends_with('\n'));
        let lines = compacted.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 1);
        serde_json::from_str::<DurableWorldEvent>(lines[0]).expect("compacted event is valid");
    }

    #[test]
    fn sensitive_and_failed_actions_promote_payload_free_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("timeline.jsonl");
        let (mut store, _) = WorldTraceStore::open(&path, 10).unwrap();
        let mut admitted = admission("intent:sensitive", WorldEffectClass::ExternalEffect);
        let WorldEventKind::ActionAdmitted { summary, .. } = &mut admitted.event.event else {
            unreachable!()
        };
        *summary = "send private value super-secret-value".to_string();
        store.append_envelopes(vec![admitted], 10).unwrap();
        store
            .append_envelopes(
                vec![terminal(
                    "intent:sensitive",
                    WorldEffectClass::ExternalEffect,
                    WorldEventKind::ActionFailed {
                        effect_class: WorldEffectClass::ExternalEffect,
                        error: "driver error included super-secret-value".to_string(),
                    },
                    16,
                )],
                16,
            )
            .unwrap();

        let evidence = store.evidence_after(0, 10);
        assert_eq!(evidence.len(), 2);
        assert_eq!(evidence[0].reasons, vec!["sensitive_effect"]);
        assert_eq!(evidence[1].reasons, vec!["sensitive_effect", "failure"]);
        assert_eq!(evidence[1].action_elapsed_ms, Some(6));
        let encoded = serde_json::to_string(&evidence).unwrap();
        assert!(!encoded.contains("super-secret-value"));
        assert!(!encoded.contains("driver error"));
        assert!(!encoded.contains("grant:test"));

        let evidence_path = store.evidence_path().to_path_buf();
        drop(store);
        assert!(evidence_path.exists());
        let (store, _) = WorldTraceStore::open(path, 20).unwrap();
        assert_eq!(store.evidence_after(0, 10), evidence);
    }

    #[test]
    fn startup_repairs_missing_trailing_evidence_from_the_causal_ledger() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("timeline.jsonl");
        let (mut store, _) = WorldTraceStore::open(&path, 10).unwrap();
        store
            .append_envelopes(
                vec![admission(
                    "intent:sensitive",
                    WorldEffectClass::ExternalEffect,
                )],
                10,
            )
            .unwrap();
        store
            .append_envelopes(
                vec![terminal(
                    "intent:sensitive",
                    WorldEffectClass::ExternalEffect,
                    WorldEventKind::ActionFailed {
                        effect_class: WorldEffectClass::ExternalEffect,
                        error: "driver payload must not survive promotion".to_string(),
                    },
                    16,
                )],
                16,
            )
            .unwrap();

        let evidence_path = store.evidence_path().to_path_buf();
        drop(store);
        fs::remove_file(evidence_path).unwrap();

        let (store, recovered) = WorldTraceStore::open(path, 20).unwrap();
        assert_eq!(recovered, 0);
        let evidence = store.evidence_after(0, 10);
        assert_eq!(evidence.len(), 2);
        assert_eq!(evidence[0].reasons, vec!["sensitive_effect"]);
        assert_eq!(evidence[1].reasons, vec!["sensitive_effect", "failure"]);
        assert_eq!(evidence[1].action_elapsed_ms, Some(6));
        assert!(
            evidence
                .iter()
                .all(|record| record.durability_latency_us.is_none())
        );
    }

    #[test]
    fn ordinary_confirmed_action_stays_in_the_bounded_raw_ring() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("timeline.jsonl");
        let (mut store, _) = WorldTraceStore::open(&path, 10).unwrap();
        store
            .append_envelopes(
                vec![admission(
                    "intent:ordinary",
                    WorldEffectClass::LocalMutation,
                )],
                10,
            )
            .unwrap();
        store
            .append_envelopes(
                vec![terminal(
                    "intent:ordinary",
                    WorldEffectClass::LocalMutation,
                    WorldEventKind::ActionCommitted {
                        effect_class: WorldEffectClass::LocalMutation,
                        status: WorldActionStatus::Confirmed,
                        summary: "confirmed".to_string(),
                        recovery: None,
                    },
                    15,
                )],
                15,
            )
            .unwrap();

        assert!(store.evidence_after(0, 10).is_empty());
        assert_eq!(store.telemetry_samples().len(), 2);
        assert_eq!(store.telemetry_samples()[1].action_elapsed_ms, Some(5));
        assert_eq!(
            store.telemetry_samples()[1].status.as_deref(),
            Some("confirmed")
        );
    }

    #[test]
    fn raw_telemetry_ring_is_bounded_without_promoting_routine_events() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("timeline.jsonl");
        let (mut store, _) = WorldTraceStore::open(&path, 10).unwrap();
        let envelopes = (0..DEFAULT_TELEMETRY_CAPACITY + 3)
            .map(|index| {
                let mut envelope = admission(&format!("intent:{index}"), WorldEffectClass::Observe);
                envelope.event.sequence = index as u64 + 1;
                envelope.event.intent_id = None;
                envelope.event.event = WorldEventKind::ExternalMutationObserved {
                    summary: "routine mirror update".to_string(),
                };
                envelope
            })
            .collect();
        store.append_envelopes(envelopes, 10).unwrap();

        assert_eq!(store.telemetry_samples().len(), DEFAULT_TELEMETRY_CAPACITY);
        assert_eq!(
            store.telemetry_samples().front().unwrap().ledger_sequence,
            4
        );
        assert!(store.evidence_after(0, 10).is_empty());
    }
}
