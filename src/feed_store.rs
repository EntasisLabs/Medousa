//! Persistent feed event log per profile.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use anyhow::Result;
use medousa_types::authority_id::{EnvironmentProfileId, FeedId};
use medousa_types::feed::{FeedEvent, FeedLatestGoodResponse};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex as AsyncMutex, RwLock as AsyncRwLock, Semaphore};

use crate::persistence::{CommitReceipt, DurabilityLevel, FileTransaction, StoreKind};
use crate::store_root::{StoreEntryKind, StorePath, StoreRoot};

const STORE_DIR: &str = "feeds";
const MAX_EVENTS_PER_FEED: usize = 200;
const MAX_FEED_LOG_BYTES: u64 = 16 * 1024 * 1024;
const MAX_FEED_EVENT_BYTES: usize = 256 * 1024;
const MAX_FEED_HOT_BYTES: usize = 4 * 1024 * 1024;
const COMPACT_AFTER_RECORDS: usize = 400;
const COMPACT_AFTER_BYTES: usize = 8 * 1024 * 1024;
const FEED_IO_CONCURRENCY: usize = 16;

static FEED_IO_PERMITS: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(FEED_IO_CONCURRENCY));

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct FeedKey {
    profile_id: String,
    feed_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct FeedLogRecord {
    version: u8,
    generation: u64,
    event: FeedEvent,
}

#[derive(Debug, Serialize, Deserialize)]
struct FeedCursorRecord {
    version: u8,
    generation: u64,
    event_generation: u64,
    read_cursor: u64,
}

#[derive(Debug, Default)]
struct FeedOwnerState {
    loaded: bool,
    events: VecDeque<Arc<FeedEvent>>,
    retained_bytes: usize,
    next_seq: u64,
    generation: u64,
    read_cursor: u64,
    cursor_generation: u64,
    log_records: usize,
    log_bytes: usize,
    latest_good: Option<FeedLatestGoodResponse>,
    legacy_loaded: bool,
}

struct FeedOwner {
    root: Arc<StoreRoot>,
    transaction: FileTransaction,
    log_path: StorePath,
    cursor_path: StorePath,
    legacy_path: StorePath,
    state: AsyncMutex<FeedOwnerState>,
}

#[derive(Clone)]
pub struct FeedStore {
    root_path: PathBuf,
    owners: Arc<AsyncRwLock<HashMap<FeedKey, Arc<FeedOwner>>>>,
}

#[derive(Debug, Clone)]
pub struct FeedAppendReceipt {
    pub seq: u64,
    pub commit: CommitReceipt,
}

impl Default for FeedStore {
    fn default() -> Self {
        Self::new()
    }
}

impl FeedStore {
    pub fn new() -> Self {
        Self::new_in(Self::store_root())
    }

    pub fn new_in(root_path: PathBuf) -> Self {
        Self {
            root_path,
            owners: Arc::new(AsyncRwLock::new(HashMap::new())),
        }
    }

    fn store_root() -> PathBuf {
        crate::paths::medousa_data_dir().join(STORE_DIR)
    }

    fn store_at(path: &Path) -> Result<StoreRoot> {
        Ok(StoreRoot::open_or_create_nofollow(path)?)
    }

    fn profile_path(profile_id: &EnvironmentProfileId) -> StorePath {
        StorePath::parse(profile_id.storage_key().as_str())
            .expect("opaque feed profile key is a valid store path")
    }

    fn feed_path(profile_id: &str, feed_id: &str) -> Result<StorePath> {
        let profile = EnvironmentProfileId::parse(profile_id)?;
        let feed = FeedId::parse(feed_id)?;
        Ok(StorePath::parse(&format!(
            "{}/{}.jsonl",
            profile.storage_key().as_str(),
            feed.storage_key().as_str()
        ))?)
    }

    fn legacy_feed_path(profile_id: &str, feed_id: &str) -> Result<StorePath> {
        let profile = EnvironmentProfileId::parse(profile_id)?;
        let feed = FeedId::parse(feed_id)?;
        Ok(StorePath::parse(&format!(
            "{}/{}.jsonl",
            profile.as_str(),
            feed.as_str()
        ))?)
    }

    fn cursor_path(profile_id: &str, feed_id: &str) -> Result<StorePath> {
        let profile = EnvironmentProfileId::parse(profile_id)?;
        let feed = FeedId::parse(feed_id)?;
        Ok(StorePath::parse(&format!(
            "{}/{}.cursor.json",
            profile.storage_key().as_str(),
            feed.storage_key().as_str()
        ))?)
    }

    async fn owner(&self, profile_id: &str, feed_id: &str) -> Result<Arc<FeedOwner>> {
        let profile = EnvironmentProfileId::parse(profile_id)?;
        let feed = FeedId::parse(feed_id)?;
        let key = FeedKey {
            profile_id: profile.as_str().to_string(),
            feed_id: feed.as_str().to_string(),
        };
        if let Some(owner) = self.owners.read().await.get(&key).cloned() {
            return Ok(owner);
        }

        let root = Arc::new(Self::store_at(&self.root_path)?);
        let candidate = Arc::new(FeedOwner {
            transaction: FileTransaction::new(Arc::clone(&root)),
            root,
            log_path: Self::feed_path(profile_id, feed_id)?,
            cursor_path: Self::cursor_path(profile_id, feed_id)?,
            legacy_path: Self::legacy_feed_path(profile_id, feed_id)?,
            state: AsyncMutex::new(FeedOwnerState::default()),
        });
        let mut owners = self.owners.write().await;
        Ok(owners.entry(key).or_insert(candidate).clone())
    }

    pub async fn append(
        &self,
        profile_id: &str,
        feed_id: &str,
        mut event: FeedEvent,
    ) -> Result<FeedAppendReceipt> {
        let owner = self.owner(profile_id, feed_id).await?;
        let mut state = owner.state.lock().await;
        owner.ensure_loaded(&mut state).await?;
        if state.legacy_loaded
            || state.log_records >= COMPACT_AFTER_RECORDS
            || state.log_bytes >= COMPACT_AFTER_BYTES
        {
            owner.compact(&mut state).await?;
        }

        let seq = state.next_seq;
        let generation = state.generation.saturating_add(1);
        event.id = new_feed_event_id(seq);
        let event_bytes = serde_json::to_vec(&event)?.len();
        if event_bytes > MAX_FEED_EVENT_BYTES {
            anyhow::bail!("feed event exceeds the {MAX_FEED_EVENT_BYTES}-byte persistence limit");
        }
        let record = serde_json::to_vec(&FeedLogRecord {
            version: 1,
            generation,
            event: event.clone(),
        })?;
        let written = owner
            .append_record(record, DurabilityLevel::Written)
            .await?;

        state.next_seq = seq.saturating_add(1);
        state.generation = generation;
        state.log_records = state.log_records.saturating_add(1);
        state.log_bytes = state.log_bytes.saturating_add(written);
        retain_event(&mut state, Arc::new(event), event_bytes);
        Ok(FeedAppendReceipt {
            seq,
            commit: CommitReceipt::new(
                StoreKind::Feed,
                format!("{profile_id}:{feed_id}"),
                generation,
                DurabilityLevel::Written,
                written,
            ),
        })
    }

    pub async fn tail(&self, profile_id: &str, feed_id: &str, limit: usize) -> Vec<FeedEvent> {
        let Ok(owner) = self.owner(profile_id, feed_id).await else {
            return Vec::new();
        };
        let mut state = owner.state.lock().await;
        if owner.ensure_loaded(&mut state).await.is_err() {
            return Vec::new();
        }
        let skip = state.events.len().saturating_sub(limit);
        state
            .events
            .iter()
            .skip(skip)
            .map(|event| event.as_ref().clone())
            .collect()
    }

    pub async fn list_feed_ids(&self, profile_id: &str) -> Vec<String> {
        let mut ids = Vec::new();
        if let (Ok(store), Ok(profile)) = (
            Self::store_at(&self.root_path),
            EnvironmentProfileId::parse(profile_id),
        ) {
            let opaque_profile = Self::profile_path(&profile);
            if let Ok(entries) = store.list_directory(&opaque_profile) {
                for entry in entries {
                    if entry.kind != StoreEntryKind::File
                        || !entry.path.file_name().ends_with(".jsonl")
                    {
                        continue;
                    }
                    let Ok(path) = opaque_profile.join(&entry.path) else {
                        continue;
                    };
                    if let Ok(raw) = store.read_limited(&path, MAX_FEED_LOG_BYTES)
                        && let Some(event) = first_feed_event(&raw)
                        && FeedId::parse(&event.feed_id).is_ok_and(|feed| {
                            format!("{}.jsonl", feed.storage_key().as_str())
                                == entry.path.file_name()
                        })
                    {
                        ids.push(event.feed_id);
                    }
                }
            }
            if let Ok(legacy_profile) = StorePath::parse(profile.as_str())
                && let Ok(entries) = store.list_directory(&legacy_profile)
            {
                for entry in entries {
                    if entry.kind == StoreEntryKind::File
                        && let Some(feed_id) = entry.path.file_name().strip_suffix(".jsonl")
                        && FeedId::parse(feed_id).is_ok()
                    {
                        ids.push(feed_id.to_string());
                    }
                }
            }
        }
        let owners = self.owners.read().await;
        for key in owners.keys().filter(|key| key.profile_id == profile_id) {
            if !ids.iter().any(|existing| existing == &key.feed_id) {
                ids.push(key.feed_id.clone());
            }
        }
        ids.sort();
        ids
    }

    pub async fn event_count(&self, profile_id: &str, feed_id: &str) -> u64 {
        let Ok(owner) = self.owner(profile_id, feed_id).await else {
            return 0;
        };
        let mut state = owner.state.lock().await;
        if owner.ensure_loaded(&mut state).await.is_err() {
            return 0;
        }
        state.events.len() as u64
    }

    pub async fn latest_good(
        &self,
        profile_id: &str,
        feed_id: &str,
    ) -> Option<FeedLatestGoodResponse> {
        let owner = self.owner(profile_id, feed_id).await.ok()?;
        let mut state = owner.state.lock().await;
        owner.ensure_loaded(&mut state).await.ok()?;
        state.latest_good.clone()
    }

    pub async fn set_read_cursor(
        &self,
        profile_id: &str,
        feed_id: &str,
        seq: u64,
    ) -> Result<CommitReceipt> {
        let owner = self.owner(profile_id, feed_id).await?;
        let mut state = owner.state.lock().await;
        owner.ensure_loaded(&mut state).await?;
        let committed_max = state.next_seq.saturating_sub(1);
        let read_cursor = seq.min(committed_max);
        let cursor_generation = state.cursor_generation.saturating_add(1);
        let body = serde_json::to_vec(&FeedCursorRecord {
            version: 1,
            generation: cursor_generation,
            event_generation: state.generation,
            read_cursor,
        })?;
        let bytes = owner.replace_cursor(body, DurabilityLevel::Synced).await?;
        state.read_cursor = read_cursor;
        state.cursor_generation = cursor_generation;
        Ok(CommitReceipt::new(
            StoreKind::Feed,
            format!("{profile_id}:{feed_id}:cursor"),
            cursor_generation,
            DurabilityLevel::Synced,
            bytes,
        ))
    }
}

impl FeedOwner {
    async fn ensure_loaded(&self, state: &mut FeedOwnerState) -> Result<()> {
        if state.loaded {
            return Ok(());
        }
        let root = Arc::clone(&self.root);
        let log_path = self.log_path.clone();
        let legacy_path = self.legacy_path.clone();
        let cursor_path = self.cursor_path.clone();
        let loaded =
            feed_io(move || load_owner_state(&root, &log_path, &legacy_path, &cursor_path)).await?;
        *state = loaded;
        Ok(())
    }

    async fn append_record(&self, record: Vec<u8>, durability: DurabilityLevel) -> Result<usize> {
        let transaction = self.transaction.clone();
        let path = self.log_path.clone();
        feed_io(move || Ok(transaction.append_record(&path, &record, durability)?)).await
    }

    async fn replace_cursor(&self, body: Vec<u8>, durability: DurabilityLevel) -> Result<usize> {
        let transaction = self.transaction.clone();
        let path = self.cursor_path.clone();
        feed_io(move || Ok(transaction.replace_snapshot(&path, &body, durability)?)).await
    }

    async fn compact(&self, state: &mut FeedOwnerState) -> Result<()> {
        let mut body = Vec::new();
        for event in &state.events {
            let generation = feed_event_seq(&event.id).unwrap_or(0).saturating_add(1);
            serde_json::to_writer(
                &mut body,
                &FeedLogRecord {
                    version: 1,
                    generation,
                    event: event.as_ref().clone(),
                },
            )?;
            body.push(b'\n');
        }
        let transaction = self.transaction.clone();
        let path = self.log_path.clone();
        let bytes = body.len();
        feed_io(move || {
            transaction.replace_snapshot(&path, &body, DurabilityLevel::Synced)?;
            Ok(())
        })
        .await?;
        state.log_records = state.events.len();
        state.log_bytes = bytes;
        state.legacy_loaded = false;
        Ok(())
    }
}

async fn feed_io<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    let permit = FEED_IO_PERMITS.acquire().await?;
    let result = tokio::task::spawn_blocking(operation).await?;
    drop(permit);
    result
}

fn load_owner_state(
    root: &StoreRoot,
    log_path: &StorePath,
    legacy_path: &StorePath,
    cursor_path: &StorePath,
) -> Result<FeedOwnerState> {
    let (raw, legacy_loaded) = match root.read_limited(log_path, MAX_FEED_LOG_BYTES) {
        Ok(raw) => (raw, false),
        Err(error) if error.is_not_found() => {
            match root.read_limited(legacy_path, MAX_FEED_LOG_BYTES) {
                Ok(raw) => (raw, true),
                Err(error) if error.is_not_found() => (Vec::new(), false),
                Err(error) => return Err(error.into()),
            }
        }
        Err(error) => return Err(error.into()),
    };

    let mut state = FeedOwnerState {
        loaded: true,
        log_bytes: raw.len(),
        legacy_loaded,
        ..FeedOwnerState::default()
    };
    for line in complete_jsonl_lines(&raw)? {
        let (generation, event) = match serde_json::from_slice::<FeedLogRecord>(line) {
            Ok(record) if record.version == 1 => (record.generation, record.event),
            Ok(_) => anyhow::bail!("unsupported feed log record version"),
            Err(_) => (0, serde_json::from_slice::<FeedEvent>(line)?),
        };
        let seq = feed_event_seq(&event.id).unwrap_or(state.next_seq);
        state.next_seq = state.next_seq.max(seq.saturating_add(1));
        state.generation = state.generation.max(generation.max(seq.saturating_add(1)));
        state.log_records = state.log_records.saturating_add(1);
        let event_bytes = line.len();
        retain_event(&mut state, Arc::new(event), event_bytes);
    }

    if let Ok(raw) = root.read_limited(cursor_path, 64 * 1024) {
        let cursor: FeedCursorRecord = serde_json::from_slice(&raw)?;
        if cursor.version != 1 || cursor.event_generation > state.generation {
            anyhow::bail!("feed cursor generation is inconsistent with the event log");
        }
        state.read_cursor = cursor.read_cursor.min(state.next_seq.saturating_sub(1));
        state.cursor_generation = cursor.generation;
    }
    Ok(state)
}

fn complete_jsonl_lines(raw: &[u8]) -> Result<Vec<&[u8]>> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (index, byte) in raw.iter().enumerate() {
        if *byte != b'\n' {
            continue;
        }
        let line = &raw[start..index];
        start = index + 1;
        if !line.iter().all(u8::is_ascii_whitespace) {
            lines.push(line);
        }
    }
    // A partial final record is an expected crash boundary. A complete final
    // JSON value without framing is intentionally not admitted.
    if start < raw.len() && raw[start..].iter().all(u8::is_ascii_whitespace) {
        return Ok(lines);
    }
    Ok(lines)
}

fn retain_event(state: &mut FeedOwnerState, event: Arc<FeedEvent>, bytes: usize) {
    if let Some(result) = extract_latest_good(&event) {
        state.latest_good = Some(FeedLatestGoodResponse {
            feed_id: event.feed_id.clone(),
            datatype: result.datatype,
            body: result.body,
            job_id: result.job_id,
            finished_at: result.finished_at,
        });
    }
    state.retained_bytes = state.retained_bytes.saturating_add(bytes);
    state.events.push_back(event);
    while state.events.len() > MAX_EVENTS_PER_FEED || state.retained_bytes > MAX_FEED_HOT_BYTES {
        let Some(evicted) = state.events.pop_front() else {
            break;
        };
        state.retained_bytes = state
            .retained_bytes
            .saturating_sub(serde_json::to_vec(evicted.as_ref()).map_or(0, |raw| raw.len()));
    }
}

fn feed_event_seq(id: &str) -> Option<u64> {
    id.strip_prefix("feed-")?.parse().ok()
}

static FEED_STORE: std::sync::OnceLock<FeedStore> = std::sync::OnceLock::new();

pub fn feed_store() -> &'static FeedStore {
    FEED_STORE.get_or_init(FeedStore::new)
}

pub fn new_feed_event_id(seq: u64) -> String {
    format!("feed-{seq}")
}

pub fn ensure_store_dir() -> Result<()> {
    FeedStore::store_at(&FeedStore::store_root())?;
    Ok(())
}

fn first_feed_event(raw: &[u8]) -> Option<FeedEvent> {
    raw.split(|byte| *byte == b'\n')
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        .find_map(|line| {
            serde_json::from_slice::<FeedLogRecord>(line)
                .map(|record| record.event)
                .or_else(|_| serde_json::from_slice::<FeedEvent>(line))
                .ok()
        })
}

#[derive(Debug, Clone)]
struct LatestGoodExtract {
    datatype: String,
    body: String,
    job_id: Option<String>,
    finished_at: Option<String>,
}

fn extract_latest_good(event: &FeedEvent) -> Option<LatestGoodExtract> {
    let payload = event.payload.as_ref()?;
    if !is_success_payload(payload) {
        return None;
    }

    let body = payload
        .get("body")
        .and_then(|value| value.as_str())
        .or_else(|| payload.get("excerpt").and_then(|value| value.as_str()))
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();

    let datatype = payload
        .get("datatype")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| infer_feed_datatype(&body));

    let job_id = payload
        .get("jobId")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            event
                .refs
                .iter()
                .find(|reference| reference.ref_type == "job")
                .map(|reference| reference.ref_id.clone())
        });

    let finished_at = payload
        .get("finishedAt")
        .or_else(|| payload.get("checkedAt"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| Some(event.emitted_at_utc.to_rfc3339()));

    Some(LatestGoodExtract {
        datatype,
        body,
        job_id,
        finished_at,
    })
}

fn is_success_payload(payload: &serde_json::Value) -> bool {
    let phase = payload
        .get("phase")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    match phase {
        "tick_succeeded" | "synthesis" => true,
        "started" | "working" | "wrapping_up" => false,
        _ => payload
            .get("status")
            .and_then(|value| value.as_str())
            .is_some_and(|status| status == "done"),
    }
}

fn infer_feed_datatype(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "text".to_string();
    }
    if trimmed.starts_with("data:image/") || looks_like_image_ref(trimmed) {
        return "image".to_string();
    }
    if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
        return "json".to_string();
    }
    if looks_like_csv(trimmed) {
        return "csv".to_string();
    }
    if trimmed.contains("# ")
        || trimmed.contains("**")
        || trimmed.contains("\n- ")
        || trimmed.contains("\n* ")
    {
        return "md".to_string();
    }
    "text".to_string()
}

fn looks_like_image_ref(value: &str) -> bool {
    let lower = value.to_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("vault/")
        || lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
        || lower.ends_with(".gif")
        || lower.ends_with(".svg")
}

fn looks_like_csv(text: &str) -> bool {
    let lines: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() < 2 {
        return false;
    }
    let comma_lines = lines.iter().filter(|line| line.contains(',')).count();
    comma_lines >= 2 && comma_lines * 2 >= lines.len()
}

#[cfg(test)]
mod latest_good_tests {
    use super::*;
    use chrono::Utc;
    use medousa_types::feed::{FeedRef, FeedSource};
    use serde_json::json;
    use std::collections::HashSet;

    fn sample_event(payload: serde_json::Value) -> FeedEvent {
        FeedEvent {
            id: "feed-1".to_string(),
            feed_id: "summer-ai-digest".to_string(),
            emitted_at_utc: Utc::now(),
            source: FeedSource::RecurringJob.as_str().to_string(),
            summary: "ok".to_string(),
            refs: vec![FeedRef {
                ref_type: "job".to_string(),
                ref_id: "job-1".to_string(),
            }],
            payload: Some(payload),
        }
    }

    fn test_store() -> (tempfile::TempDir, PathBuf, FeedStore) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap().join("feeds");
        let store = FeedStore::new_in(root.clone());
        (directory, root, store)
    }

    #[test]
    fn feed_paths_are_opaque_and_reject_authority_syntax() {
        let dotted = FeedStore::feed_path("personal", "workshop.pulse").unwrap();
        let underscored = FeedStore::feed_path("personal", "workshop_pulse").unwrap();
        assert_ne!(dotted, underscored);
        assert!(!dotted.file_name().contains("workshop"));
        assert!(FeedStore::feed_path("../../outside", "workshop.pulse").is_err());
        assert!(FeedStore::feed_path("personal", "../../outside").is_err());
    }

    #[test]
    fn extract_latest_good_prefers_body_and_datatype() {
        let event = sample_event(json!({
            "phase": "tick_succeeded",
            "body": "# Digest\nHello",
            "datatype": "md",
            "jobId": "job-1",
            "checkedAt": "2026-07-22T12:00:00Z"
        }));
        let result = extract_latest_good(&event).expect("good");
        assert_eq!(result.datatype, "md");
        assert!(result.body.contains("Digest"));
        assert_eq!(result.job_id.as_deref(), Some("job-1"));
    }

    #[test]
    fn extract_latest_good_skips_failed_ticks() {
        let event = sample_event(json!({
            "phase": "tick_failed",
            "body": "should ignore"
        }));
        assert!(extract_latest_good(&event).is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_appends_have_one_order_and_survive_reopen() {
        let (_directory, root, store) = test_store();
        let store = Arc::new(store);
        let mut tasks = Vec::new();
        for index in 0..64 {
            let store = Arc::clone(&store);
            tasks.push(tokio::spawn(async move {
                let mut event = sample_event(json!({"phase": "tick_succeeded", "body": index}));
                event.summary = format!("event-{index}");
                store
                    .append("personal", "summer-ai-digest", event)
                    .await
                    .unwrap()
            }));
        }
        let mut sequences = Vec::new();
        for task in tasks {
            sequences.push(task.await.unwrap().seq);
        }
        sequences.sort_unstable();
        assert_eq!(sequences, (0..64).collect::<Vec<_>>());

        drop(store);
        let reopened = FeedStore::new_in(root);
        let events = reopened.tail("personal", "summer-ai-digest", 100).await;
        assert_eq!(events.len(), 64);
        let ids = events
            .iter()
            .map(|event| event.id.clone())
            .collect::<HashSet<_>>();
        assert_eq!(ids.len(), 64);
    }

    #[tokio::test]
    async fn partial_tail_is_discarded_without_losing_committed_records() {
        let (_directory, root_path, store) = test_store();
        store
            .append(
                "personal",
                "summer-ai-digest",
                sample_event(json!({"phase": "tick_succeeded", "body": "complete"})),
            )
            .await
            .unwrap();
        drop(store);

        let root = FeedStore::store_at(&root_path).unwrap();
        let path = FeedStore::feed_path("personal", "summer-ai-digest").unwrap();
        root.append(&path, br#"{"version":1,"generation":2"#)
            .unwrap();

        let reopened = FeedStore::new_in(root_path);
        let events = reopened.tail("personal", "summer-ai-digest", 10).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "feed-0");
    }

    #[tokio::test]
    async fn cursor_is_clamped_and_persisted_with_event_generation() {
        let (_directory, root, store) = test_store();
        for index in 0..2 {
            let mut event = sample_event(json!({"phase": "tick_succeeded", "body": index}));
            event.summary = format!("event-{index}");
            store
                .append("personal", "summer-ai-digest", event)
                .await
                .unwrap();
        }
        let receipt = store
            .set_read_cursor("personal", "summer-ai-digest", 999)
            .await
            .unwrap();
        assert_eq!(receipt.durability, DurabilityLevel::Synced);
        drop(store);

        let reopened = FeedStore::new_in(root);
        let owner = reopened
            .owner("personal", "summer-ai-digest")
            .await
            .unwrap();
        let mut state = owner.state.lock().await;
        owner.ensure_loaded(&mut state).await.unwrap();
        assert_eq!(state.read_cursor, 1);
        assert_eq!(state.generation, 2);
    }

    #[tokio::test]
    async fn retention_uses_a_bounded_tail_without_rewriting_each_append() {
        let (_directory, root, store) = test_store();
        for index in 0..220 {
            let mut event = sample_event(json!({"phase": "tick_succeeded", "body": index}));
            event.summary = format!("event-{index}");
            store
                .append("personal", "summer-ai-digest", event)
                .await
                .unwrap();
        }
        assert_eq!(
            store
                .tail("personal", "summer-ai-digest", MAX_EVENTS_PER_FEED + 1)
                .await
                .len(),
            MAX_EVENTS_PER_FEED
        );
        drop(store);
        let reopened = FeedStore::new_in(root);
        let events = reopened
            .tail("personal", "summer-ai-digest", MAX_EVENTS_PER_FEED + 1)
            .await;
        assert_eq!(events.len(), MAX_EVENTS_PER_FEED);
        assert_eq!(events.first().unwrap().id, "feed-20");
    }

    #[tokio::test]
    async fn legacy_log_migrates_before_accepting_a_new_append() {
        let (_directory, root_path, store) = test_store();
        let root = FeedStore::store_at(&root_path).unwrap();
        let legacy = FeedStore::legacy_feed_path("personal", "summer-ai-digest").unwrap();
        let mut old = sample_event(json!({"phase": "tick_succeeded", "body": "old"}));
        old.id = "feed-7".to_string();
        let mut raw = serde_json::to_vec(&old).unwrap();
        raw.push(b'\n');
        root.append(&legacy, &raw).unwrap();

        let receipt = store
            .append(
                "personal",
                "summer-ai-digest",
                sample_event(json!({"phase": "tick_succeeded", "body": "new"})),
            )
            .await
            .unwrap();
        assert_eq!(receipt.seq, 8);
        drop(store);

        let reopened = FeedStore::new_in(root_path);
        let events = reopened.tail("personal", "summer-ai-digest", 10).await;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, "feed-7");
        assert_eq!(events[1].id, "feed-8");
    }
}
