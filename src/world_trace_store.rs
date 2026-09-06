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

use medousa_world::{
    WorldActionStatus, WorldEffectClass, WorldEvent, WorldEventEnvelope, WorldEventKind, WorldId,
    WorldIntentId,
};
use serde::{Deserialize, Serialize};

pub const WORLD_TIMELINE_SCHEMA_VERSION: u16 = 1;
const DEFAULT_TIMELINE_CAPACITY: usize = 8_192;
const DEFAULT_TIMELINE_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TIMELINE_LINE_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableWorldEvent {
    pub schema_version: u16,
    /// Monotonic across every world owned by this workshop daemon.
    pub ledger_sequence: u64,
    pub recorded_at_ms: u64,
    pub envelope: WorldEventEnvelope,
}

#[derive(Debug)]
pub struct WorldTraceStore {
    path: PathBuf,
    events: VecDeque<DurableWorldEvent>,
    next_sequence: u64,
    capacity: usize,
    max_file_bytes: u64,
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

        let mut store = Self {
            path,
            events,
            next_sequence,
            capacity,
            max_file_bytes: max_file_bytes.max(1),
        };
        if needs_compaction
            || fs::metadata(&store.path)
                .map(|metadata| metadata.len() > store.max_file_bytes)
                .unwrap_or(false)
        {
            store.compact()?;
        }
        let recovered = store.recover_interrupted(now_ms)?;
        Ok((store, recovered))
    }

    pub fn path(&self) -> &Path {
        &self.path
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

    pub fn append_envelopes(
        &mut self,
        envelopes: Vec<WorldEventEnvelope>,
        recorded_at_ms: u64,
    ) -> Result<(), String> {
        if envelopes.is_empty() {
            return Ok(());
        }
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

        self.events.extend(appended);
        while self.events.len() > self.capacity {
            self.events.pop_front();
        }
        if fs::metadata(&self.path)
            .map(|metadata| metadata.len() > self.max_file_bytes)
            .unwrap_or(false)
        {
            self.compact()?;
        }
        Ok(())
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
}
