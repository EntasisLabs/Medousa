use super::*;
use fs2::FileExt;
use medousa_store::{StoreEntryKind, StorePath, StoreRoot, StoreRootError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

const HISTORY_BYTES: usize = 4 * 1024 * 1024;
const MAX_RECORD_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RECEIPTS: usize = 128;

pub struct CreativeStore {
    root: StoreRoot,
}
#[derive(Clone, Serialize, Deserialize)]
struct Patch {
    nodes: Vec<(String, Option<CreativeNode>)>,
    order: Vec<String>,
    title: String,
    pinned: bool,
    background: String,
}
impl Patch {
    fn between(from: &CreativeDocument, to: &CreativeDocument) -> Self {
        let previous: BTreeMap<_, _> = from.scene.nodes.iter().map(|n| (&n.id, n)).collect();
        let current: BTreeMap<_, _> = to.scene.nodes.iter().map(|n| (&n.id, n)).collect();
        let ids: BTreeSet<_> = previous.keys().chain(current.keys()).copied().collect();
        let nodes = ids
            .into_iter()
            .filter(|id| previous.get(id) != current.get(id))
            .map(|id| (id.clone(), current.get(id).map(|n| (*n).clone())))
            .collect();
        Self {
            nodes,
            order: to.scene.nodes.iter().map(|n| n.id.clone()).collect(),
            title: to.title.clone(),
            pinned: to.pinned,
            background: to.scene.background.clone(),
        }
    }
    fn apply(&self, doc: &mut CreativeDocument) -> Result<()> {
        let mut nodes: BTreeMap<_, _> = doc
            .scene
            .nodes
            .drain(..)
            .map(|n| (n.id.clone(), n))
            .collect();
        for (id, node) in &self.nodes {
            if let Some(node) = node {
                nodes.insert(id.clone(), node.clone());
            } else {
                nodes.remove(id);
            }
        }
        doc.scene.nodes = self
            .order
            .iter()
            .map(|id| {
                nodes
                    .remove(id)
                    .ok_or_else(|| invalid("corrupt history order"))
            })
            .collect::<Result<_>>()?;
        if !nodes.is_empty() {
            return Err(invalid("corrupt history nodes"));
        }
        doc.title = self.title.clone();
        doc.pinned = self.pinned;
        doc.scene.background = self.background.clone();
        validate_document(doc)
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct History {
    forward: Patch,
    backward: Patch,
    label: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Receipt {
    id: String,
    hash: String,
    revision: u64,
}
#[derive(Clone, Serialize, Deserialize)]
struct Record {
    document: CreativeDocument,
    create_hash: String,
    deleted: bool,
    undo: Vec<History>,
    redo: Vec<History>,
    receipts: Vec<Receipt>,
    proposals: Vec<CreativeProposal>,
}
fn storage(error: impl std::fmt::Display) -> CreativeError {
    CreativeError::Storage(error.to_string())
}
fn path(value: &str) -> Result<StorePath> {
    StorePath::parse(value).map_err(storage)
}
fn hash<T: Serialize>(value: &T) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).map_err(storage)?)
    ))
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
impl CreativeStore {
    pub fn open(root: &Path) -> Result<Self> {
        let root = StoreRoot::open_or_create_nofollow(root).map_err(storage)?;
        root.create_dir_all(&path("projects")?).map_err(storage)?;
        root.create_dir_all(&path("assets")?).map_err(storage)?;
        Ok(Self { root })
    }
    fn file(id: &str) -> Result<StorePath> {
        validate_id(id)?;
        path(&format!("projects/{id}.json"))
    }
    fn lock(&self, id: &str) -> Result<std::fs::File> {
        validate_id(id)?;
        let file = self
            .root
            .open_lock_file(&path(&format!("projects/{id}.lock"))?)
            .map_err(storage)?;
        file.lock_exclusive().map_err(storage)?;
        Ok(file)
    }
    fn read(&self, id: &str) -> Result<Record> {
        self.read_record(id, false)
    }
    fn read_record(&self, id: &str, include_deleted: bool) -> Result<Record> {
        let bytes = self
            .root
            .read_limited(&Self::file(id)?, MAX_RECORD_BYTES)
            .map_err(|e| {
                if e.is_not_found() {
                    CreativeError::NotFound
                } else {
                    storage(e)
                }
            })?;
        let raw: serde_json::Value = serde_json::from_slice(&bytes).map_err(storage)?;
        if raw
            .pointer("/document/scene/schema")
            .and_then(|v| v.as_str())
            != Some(CREATIVE_SCHEMA)
            || raw
                .pointer("/document/scene/version")
                .and_then(|v| v.as_u64())
                != Some(CREATIVE_VERSION as u64)
        {
            return Err(CreativeError::Unsupported);
        }
        let record: Record = serde_json::from_value(raw).map_err(storage)?;
        if record.deleted && !include_deleted {
            return Err(CreativeError::NotFound);
        }
        if record.document.id != id {
            return Err(invalid("stored project identity mismatch"));
        }
        validate_document(&record.document)?;
        Ok(record)
    }
    fn write(&self, record: &Record) -> Result<()> {
        let bytes = serde_json::to_vec(record).map_err(storage)?;
        if bytes.len() as u64 > MAX_RECORD_BYTES {
            return Err(invalid("project record capacity exceeded"));
        }
        self.root
            .atomic_write(&Self::file(&record.document.id)?, &bytes)
            .map_err(storage)
    }
    fn response(record: &Record, revision: u64, duplicate: bool) -> CreativeMutationResponse {
        CreativeMutationResponse {
            document: record.document.clone(),
            applied_revision: revision,
            duplicate,
            can_undo: !record.undo.is_empty(),
            can_redo: !record.redo.is_empty(),
        }
    }
    fn receipt(record: &Record, id: &str, hash: &str) -> Result<Option<CreativeMutationResponse>> {
        if let Some(r) = record.receipts.iter().find(|r| r.id == id) {
            if r.hash != hash {
                return Err(invalid("transaction id reused with different input"));
            }
            return Ok(Some(Self::response(record, r.revision, true)));
        }
        Ok(None)
    }
    fn bump(record: &mut Record, id: &str, hash: String) -> Result<()> {
        record.document.revision = record
            .document
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("revision exhausted"))?;
        record.document.updated_at = now();
        record.receipts.push(Receipt {
            id: id.into(),
            hash,
            revision: record.document.revision,
        });
        if record.receipts.len() > MAX_RECEIPTS {
            record.receipts.remove(0);
        }
        Ok(())
    }
    fn check_revision(record: &Record, expected: u64) -> Result<()> {
        if record.document.revision != expected {
            Err(CreativeError::Conflict(record.document.revision))
        } else {
            Ok(())
        }
    }
    pub fn create(&self, request: CreativeCreateRequest) -> Result<CreativeDocument> {
        validate_id(&request.id)?;
        let _lock = self.lock(&request.id)?;
        let create_hash = hash(&request)?;
        if self
            .root
            .is_file(&Self::file(&request.id)?)
            .map_err(storage)?
        {
            let record = self.read(&request.id)?;
            if record.create_hash == create_hash {
                return Ok(record.document);
            }
            return Err(invalid("project id is already in use"));
        }
        let time = now();
        let document = CreativeDocument {
            id: request.id,
            revision: 0,
            title: request.title,
            pinned: false,
            created_at: time.clone(),
            updated_at: time,
            scene: request.scene,
            assets: BTreeMap::new(),
        };
        validate_document(&document)?;
        let record = Record {
            document: document.clone(),
            create_hash,
            deleted: false,
            undo: vec![],
            redo: vec![],
            receipts: vec![],
            proposals: vec![],
        };
        self.write(&record)?;
        Ok(document)
    }
    pub fn get(&self, id: &str) -> Result<CreativeMutationResponse> {
        let record = self.read(id)?;
        Ok(Self::response(&record, record.document.revision, false))
    }
    pub fn list(&self, query: CreativeListQuery) -> Result<CreativeListResponse> {
        let limit = query.limit.unwrap_or(50);
        if !(1..=100).contains(&limit) {
            return Err(invalid("list limit must be 1–100"));
        }
        if let Some(cursor) = &query.cursor {
            validate_id(cursor)?;
        }
        let mut ids: Vec<_> = self
            .root
            .list_directory(&path("projects")?)
            .map_err(storage)?
            .into_iter()
            .filter(|e| e.kind == StoreEntryKind::File)
            .filter_map(|e| e.path.file_name().strip_suffix(".json").map(str::to_owned))
            .collect();
        ids.sort();
        let mut projects = Vec::new();
        let mut next_cursor = None;
        for id in ids
            .into_iter()
            .filter(|id| query.cursor.as_ref().is_none_or(|cursor| id > cursor))
        {
            let record = match self.read(&id) {
                Ok(r) => r,
                Err(CreativeError::NotFound) => continue,
                Err(e) => return Err(e),
            };
            if projects.len() == limit {
                next_cursor = projects
                    .last()
                    .map(|p: &CreativeProjectSummary| p.id.clone());
                break;
            }
            let doc = record.document;
            projects.push(CreativeProjectSummary {
                id: doc.id,
                revision: doc.revision,
                title: doc.title,
                pinned: doc.pinned,
                updated_at: doc.updated_at,
            });
        }
        Ok(CreativeListResponse {
            projects,
            next_cursor,
        })
    }
    pub fn transact(
        &self,
        id: &str,
        request: CreativeTransaction,
    ) -> Result<CreativeMutationResponse> {
        let _lock = self.lock(id)?;
        let mut record = self.read(id)?;
        let digest = hash(&request)?;
        if let Some(response) = Self::receipt(&record, &request.id, &digest)? {
            return Ok(response);
        }
        Self::edit(&mut record, &request, digest)?;
        self.write(&record)?;
        Ok(Self::response(&record, record.document.revision, false))
    }
    fn edit(record: &mut Record, request: &CreativeTransaction, digest: String) -> Result<()> {
        validate_id(&request.id)?;
        bounded_text(&request.label, 256, "transaction label")?;
        Self::check_revision(record, request.expected_revision)?;
        let next = apply_operations(&record.document, &request.operations)?;
        let entry = History {
            forward: Patch::between(&record.document, &next),
            backward: Patch::between(&next, &record.document),
            label: request.label.clone(),
        };
        if serde_json::to_vec(&entry).map_err(storage)?.len() > HISTORY_BYTES {
            return Err(invalid(
                "single edit exceeds undo capacity; split it into smaller gestures",
            ));
        }
        record.document = next;
        record.redo.clear();
        record.undo.push(entry);
        while record.undo.len() > 100
            || serde_json::to_vec(&record.undo).map_err(storage)?.len() > HISTORY_BYTES
        {
            record.undo.remove(0);
        }
        Self::bump(record, &request.id, digest)
    }
    pub fn history(
        &self,
        id: &str,
        request: CreativeHistoryRequest,
    ) -> Result<CreativeMutationResponse> {
        validate_id(&request.id)?;
        let _lock = self.lock(id)?;
        let mut record = self.read(id)?;
        let digest = hash(&("history", &request))?;
        if let Some(response) = Self::receipt(&record, &request.id, &digest)? {
            return Ok(response);
        }
        Self::check_revision(&record, request.expected_revision)?;
        let entry = if request.redo {
            record.redo.pop()
        } else {
            record.undo.pop()
        }
        .ok_or_else(|| invalid("no history in that direction"))?;
        if request.redo {
            entry.forward.apply(&mut record.document)?;
            record.undo.push(entry);
        } else {
            entry.backward.apply(&mut record.document)?;
            record.redo.push(entry);
        }
        Self::bump(&mut record, &request.id, digest)?;
        self.write(&record)?;
        Ok(Self::response(&record, record.document.revision, false))
    }
    pub fn delete(&self, id: &str, revision: u64) -> Result<()> {
        let _lock = self.lock(id)?;
        let mut record = self.read_record(id, true)?;
        Self::check_revision(&record, revision)?;
        record.deleted = true;
        self.write(&record)
    }
    pub fn import_asset(&self, id: &str, bytes: &[u8]) -> Result<CreativeAsset> {
        if bytes.is_empty() || bytes.len() > MAX_CREATIVE_ASSET_BYTES {
            return Err(invalid("asset must contain 1–25 MiB"));
        }
        let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            "image/png"
        } else if bytes.starts_with(b"\xff\xd8\xff") {
            "image/jpeg"
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            "image/gif"
        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            "image/webp"
        } else {
            return Err(invalid("asset must be a PNG, JPEG, GIF or WebP bitmap"));
        };
        let _lock = self.lock(id)?;
        let mut record = self.read(id)?;
        let sha256 = format!("{:x}", Sha256::digest(bytes));
        let asset = CreativeAsset {
            id: format!("a_{sha256}"),
            mime: mime.into(),
            byte_size: bytes.len() as u64,
            sha256,
        };
        let existing = record.document.assets.get(&asset.id).cloned();
        record
            .document
            .assets
            .insert(asset.id.clone(), asset.clone());
        validate_document(&record.document)?;
        let asset_path = path(&format!("assets/{id}/{}", asset.id))?;
        // Retry after a crash between blob and manifest publication reuses the
        // exact immutable bytes. Reimport also repairs an absent payload.
        if self.root.is_file(&asset_path).map_err(storage)? {
            let held = self
                .root
                .read_limited(&asset_path, MAX_CREATIVE_ASSET_BYTES as u64)
                .map_err(storage)?;
            if Sha256::digest(&held) != Sha256::digest(bytes) {
                return Err(storage("immutable asset content mismatch"));
            }
        } else {
            self.root
                .atomic_create(&asset_path, bytes)
                .map_err(storage)?;
        }
        if let Some(existing) = existing {
            return Ok(existing);
        }
        Self::bump(
            &mut record,
            &format!("asset_{}", uuid::Uuid::new_v4().simple()),
            hash(&asset)?,
        )?;
        self.write(&record)?;
        Ok(asset)
    }
    pub fn asset(&self, id: &str, asset_id: &str) -> Result<(CreativeAsset, Vec<u8>)> {
        validate_id(asset_id)?;
        let record = self.read(id)?;
        let asset = record
            .document
            .assets
            .get(asset_id)
            .ok_or(CreativeError::NotFound)?
            .clone();
        let bytes = self
            .root
            .read_limited(
                &path(&format!("assets/{id}/{asset_id}"))?,
                MAX_CREATIVE_ASSET_BYTES as u64,
            )
            .map_err(storage)?;
        if format!("{:x}", Sha256::digest(&bytes)) != asset.sha256 {
            return Err(storage("asset digest mismatch"));
        }
        Ok((asset, bytes))
    }
    pub fn propose(&self, id: &str, request: CreativeProposalRequest) -> Result<CreativeProposal> {
        validate_id(&request.id)?;
        let _lock = self.lock(id)?;
        let mut record = self.read(id)?;
        if let Some(existing) = record.proposals.iter().find(|p| p.request.id == request.id) {
            if hash(&existing.request)? == hash(&request)? {
                return Ok(existing.clone());
            }
            return Err(invalid("proposal id reused with different input"));
        }
        Self::check_revision(&record, request.expected_revision)?;
        if request.selected_ids.is_empty() || request.selected_ids.len() > 1000 {
            return Err(invalid("proposal requires 1–1000 selected objects"));
        }
        for selected in &request.selected_ids {
            if !record
                .document
                .scene
                .nodes
                .iter()
                .any(|n| &n.id == selected)
            {
                return Err(invalid("proposal selection does not exist"));
            }
        }
        let captured = Self::selection(&record.document, &request.selected_ids)?;
        let selected: BTreeSet<_> = captured.iter().map(|n| n.id.as_str()).collect();
        let new_ids: BTreeSet<_> = request
            .operations
            .iter()
            .filter_map(|op| match op {
                CreativeOperation::PutNode { node }
                    if !record.document.scene.nodes.iter().any(|n| n.id == node.id) =>
                {
                    Some(node.id.as_str())
                }
                _ => None,
            })
            .collect();
        for op in &request.operations {
            let target = match op {
                CreativeOperation::PutNode { node } => node
                    .parent_id
                    .as_deref()
                    .filter(|_| new_ids.contains(node.id.as_str()))
                    .unwrap_or(&node.id),
                CreativeOperation::RemoveNode { id } => id.as_str(),
                _ => "",
            };
            if Self::locked(&record.document, target) {
                return Err(invalid("unlock selected objects before proposing changes"));
            }
            let allowed = match op {
                CreativeOperation::PutNode { node } => {
                    selected.contains(node.id.as_str())
                        || (new_ids.contains(node.id.as_str())
                            && node
                                .parent_id
                                .as_deref()
                                .is_some_and(|p| selected.contains(p) || new_ids.contains(p)))
                }
                CreativeOperation::RemoveNode { id } => selected.contains(id.as_str()),
                _ => false,
            };
            if !allowed {
                return Err(invalid("proposal operation escapes its captured selection"));
            }
        }
        bounded_text(&request.label, 256, "proposal label")?;
        apply_operations(&record.document, &request.operations)?;
        while record.proposals.len() >= 32 {
            let terminal = record
                .proposals
                .iter()
                .position(|p| p.status != CreativeProposalStatus::Ready)
                .ok_or_else(|| invalid("proposal capacity reached"))?;
            record.proposals.remove(terminal);
        }
        let proposal = CreativeProposal {
            request,
            status: CreativeProposalStatus::Ready,
            created_at: now(),
            applied_revision: None,
        };
        record.proposals.push(proposal.clone());
        if serde_json::to_vec(&record.proposals)
            .map_err(storage)?
            .len()
            > 2 * 1024 * 1024
        {
            return Err(invalid("proposal byte capacity reached"));
        }
        self.write(&record)?;
        Ok(proposal)
    }
    fn selection(doc: &CreativeDocument, ids: &[String]) -> Result<Vec<CreativeNode>> {
        if ids.is_empty() || ids.len() > 1000 {
            return Err(invalid("context requires 1–1000 selected objects"));
        }
        let all: BTreeSet<_> = doc.scene.nodes.iter().map(|n| n.id.as_str()).collect();
        let mut included: BTreeSet<_> = ids.iter().map(String::as_str).collect();
        if included.len() != ids.len() {
            return Err(invalid("duplicate selected object"));
        }
        if !included.is_subset(&all) {
            return Err(invalid("context selection does not exist"));
        }
        let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for node in &doc.scene.nodes {
            if let Some(parent) = node.parent_id.as_deref() {
                children.entry(parent).or_default().push(&node.id);
            }
        }
        let mut queue: Vec<_> = included.iter().copied().collect();
        while let Some(id) = queue.pop() {
            if let Some(descendants) = children.get(id) {
                for child in descendants {
                    if included.insert(child) {
                        queue.push(child);
                    }
                }
            }
            if included.len() > 1000 {
                return Err(invalid(
                    "selection exceeds 1000 objects; narrow the selection",
                ));
            }
        }
        Ok(doc
            .scene
            .nodes
            .iter()
            .filter(|n| included.contains(n.id.as_str()))
            .cloned()
            .collect())
    }
    fn locked(doc: &CreativeDocument, id: &str) -> bool {
        let nodes: BTreeMap<_, _> = doc.scene.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let mut cursor = Some(id);
        while let Some(id) = cursor {
            let Some(node) = nodes.get(id) else {
                break;
            };
            if node.locked {
                return true;
            }
            cursor = node.parent_id.as_deref();
        }
        false
    }
    pub fn context(
        &self,
        id: &str,
        request: CreativeContextRequest,
    ) -> Result<CreativeSelectionContext> {
        let record = self.read(id)?;
        let nodes = Self::selection(&record.document, &request.selected_ids)?;
        let asset_ids: BTreeSet<_> = nodes
            .iter()
            .filter_map(|n| match &n.content {
                CreativeContent::Image { asset_id, .. } => Some(asset_id),
                _ => None,
            })
            .collect();
        let assets = asset_ids
            .into_iter()
            .map(|id| record.document.assets[id].clone())
            .collect();
        let context = CreativeSelectionContext {
            document_id: id.into(),
            revision: record.document.revision,
            selected_ids: nodes.iter().map(|n| n.id.clone()).collect(),
            fingerprint: hash(&nodes)?,
            nodes,
            assets,
        };
        if serde_json::to_vec(&context).map_err(storage)?.len() > 512 * 1024 {
            return Err(invalid(
                "selection context exceeds 512 KiB; narrow the selection",
            ));
        }
        Ok(context)
    }
    pub fn proposals(&self, id: &str) -> Result<CreativeProposalListResponse> {
        Ok(CreativeProposalListResponse {
            proposals: self.read(id)?.proposals,
        })
    }
    pub fn decide(
        &self,
        id: &str,
        proposal_id: &str,
        request: CreativeProposalDecision,
    ) -> Result<CreativeMutationResponse> {
        let _lock = self.lock(id)?;
        let mut record = self.read(id)?;
        let index = record
            .proposals
            .iter()
            .position(|p| p.request.id == proposal_id)
            .ok_or(CreativeError::NotFound)?;
        let proposal = record.proposals[index].clone();
        if proposal.status == CreativeProposalStatus::Accepted && request.accept {
            return Ok(Self::response(
                &record,
                proposal
                    .applied_revision
                    .ok_or_else(|| storage("accepted proposal has no revision"))?,
                true,
            ));
        }
        if proposal.status == CreativeProposalStatus::Cancelled && !request.accept {
            return Ok(Self::response(
                &record,
                proposal
                    .applied_revision
                    .unwrap_or(record.document.revision),
                true,
            ));
        }
        if proposal.status != CreativeProposalStatus::Ready {
            return Err(invalid("proposal is no longer ready"));
        }
        Self::check_revision(&record, request.expected_revision)?;
        if request.accept {
            Self::check_revision(&record, proposal.request.expected_revision)?;
            let transaction = CreativeTransaction {
                id: format!("proposal_{}", hash(&proposal_id)?),
                expected_revision: request.expected_revision,
                label: proposal.request.label,
                operations: proposal.request.operations,
            };
            let digest = hash(&transaction)?;
            Self::edit(&mut record, &transaction, digest)?;
            record.proposals[index].status = CreativeProposalStatus::Accepted;
            record.proposals[index].applied_revision = Some(record.document.revision);
        } else {
            // Advance the fence even for cancellation: after terminal proposals
            // are evicted, an old request must not resurrect cancelled work.
            Self::bump(
                &mut record,
                &format!("cancel_{}", hash(&proposal_id)?),
                hash(&("cancel", proposal_id, request.expected_revision))?,
            )?;
            record.proposals[index].status = CreativeProposalStatus::Cancelled;
            record.proposals[index].applied_revision = Some(record.document.revision);
        }
        self.write(&record)?;
        Ok(Self::response(&record, record.document.revision, false))
    }
}
impl From<StoreRootError> for CreativeError {
    fn from(value: StoreRootError) -> Self {
        storage(value)
    }
}
