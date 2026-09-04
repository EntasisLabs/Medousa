//! Directional, daemon-owned authority for work requested by paired peers.
//!
//! Pairing and mesh grants authenticate a caller and admit a protocol message.
//! They do not grant shell, project, secret, or agent-routing authority. This
//! store is owned and enforced by the destination workshop.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PEER_EXECUTION_POLICY_SCHEMA_VERSION: u32 = 1;
pub const TASK_EXECUTION_GRANT_SCHEMA_VERSION: u32 = 1;
const POLICY_FILE_SCHEMA_VERSION: u32 = 1;
const MAX_AUDIT_EVENTS: usize = 512;
const MAX_POLICY_FILE_BYTES: u64 = 4 * 1024 * 1024;
const SAFE_ASSISTANT_TOOL_DOMAINS: [&str; 3] = ["turn", "utility", "web"];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerExecutionPolicyPreset {
    #[default]
    ConnectedOnly,
    AssistantWork,
    SandboxedWork,
    ApprovedProjects,
    Custom,
}

impl PeerExecutionPolicyPreset {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConnectedOnly => "connected_only",
            Self::AssistantWork => "assistant_work",
            Self::SandboxedWork => "sandboxed_work",
            Self::ApprovedProjects => "approved_projects",
            Self::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerNetworkPolicy {
    #[default]
    Deny,
    WebOnly,
    Unrestricted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerExecutionPolicy {
    pub schema_version: u32,
    pub peer_device_id: String,
    pub peer_pairing_id: String,
    pub preset: PeerExecutionPolicyPreset,
    pub enabled: bool,
    pub assistant_work: bool,
    pub sandbox_execution: bool,
    pub host_shell: bool,
    pub coder_work: bool,
    pub work_environment_materialization: bool,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed_project_ids: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed_root_refs: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed_tool_domains: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed_mcp_server_ids: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed_secret_refs: BTreeSet<String>,
    pub network_policy: PeerNetworkPolicy,
    pub allow_agent_targeting: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    pub revision: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PeerExecutionPolicy {
    fn connected_only(peer_device_id: &str, peer_pairing_id: &str, now: DateTime<Utc>) -> Self {
        Self {
            schema_version: PEER_EXECUTION_POLICY_SCHEMA_VERSION,
            peer_device_id: peer_device_id.to_string(),
            peer_pairing_id: peer_pairing_id.to_string(),
            preset: PeerExecutionPolicyPreset::ConnectedOnly,
            enabled: true,
            assistant_work: false,
            sandbox_execution: false,
            host_shell: false,
            coder_work: false,
            work_environment_materialization: false,
            allowed_project_ids: BTreeSet::new(),
            allowed_root_refs: BTreeSet::new(),
            allowed_tool_domains: BTreeSet::new(),
            allowed_mcp_server_ids: BTreeSet::new(),
            allowed_secret_refs: BTreeSet::new(),
            network_policy: PeerNetworkPolicy::Deny,
            allow_agent_targeting: false,
            expires_at: None,
            revision: 0,
            created_at: now,
            updated_at: now,
        }
    }

    fn legacy_assistant(
        peer_device_id: &str,
        peer_pairing_id: &str,
        now: DateTime<Utc>,
    ) -> Self {
        let mut policy = Self::connected_only(peer_device_id, peer_pairing_id, now);
        policy.preset = PeerExecutionPolicyPreset::AssistantWork;
        policy.assistant_work = true;
        policy.allowed_tool_domains = safe_assistant_tool_domains();
        policy.network_policy = PeerNetworkPolicy::WebOnly;
        policy
    }

    pub fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_some_and(|expires_at| expires_at <= now)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerExecutionPolicySource {
    Stored,
    LegacyTaskRequest,
    DefaultDeny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerExecutionPolicyView {
    pub policy: PeerExecutionPolicy,
    pub source: PeerExecutionPolicySource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerExecutionPolicyUpdate {
    #[serde(default)]
    pub preset: PeerExecutionPolicyPreset,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assistant_work: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_execution: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_shell: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coder_work: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_environment_materialization: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_project_ids: Option<BTreeSet<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_root_refs: Option<BTreeSet<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tool_domains: Option<BTreeSet<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_mcp_server_ids: Option<BTreeSet<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_secret_refs: Option<BTreeSet<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_policy: Option<PeerNetworkPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_agent_targeting: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

impl Default for PeerExecutionPolicyUpdate {
    fn default() -> Self {
        Self {
            preset: PeerExecutionPolicyPreset::ConnectedOnly,
            enabled: None,
            assistant_work: None,
            sandbox_execution: None,
            host_shell: None,
            coder_work: None,
            work_environment_materialization: None,
            allowed_project_ids: None,
            allowed_root_refs: None,
            allowed_tool_domains: None,
            allowed_mcp_server_ids: None,
            allowed_secret_refs: None,
            network_policy: None,
            allow_agent_targeting: None,
            expires_at: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskExecutionGrant {
    pub schema_version: u32,
    pub grant_id: String,
    pub peer_device_id: String,
    pub peer_pairing_id: String,
    pub origin_runtime_id: String,
    pub destination_runtime_id: String,
    pub parent_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_id: Option<String>,
    pub work_id: String,
    pub correlation_id: String,
    pub worker_intent: String,
    pub policy_revision: u64,
    pub policy_source: PeerExecutionPolicySource,
    pub requested_tool_domains: Vec<String>,
    pub effective_tool_domains: Vec<String>,
    pub network_policy: PeerNetworkPolicy,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerExecutionDenialReason {
    PolicyDisabled,
    PolicyExpired,
    AssistantWorkDenied,
    ToolDomainDenied,
    RequestExpired,
}

impl PeerExecutionDenialReason {
    pub const fn code(self) -> &'static str {
        match self {
            Self::PolicyDisabled => "peer_execution_policy_disabled",
            Self::PolicyExpired => "peer_execution_policy_expired",
            Self::AssistantWorkDenied => "peer_execution_assistant_denied",
            Self::ToolDomainDenied => "peer_execution_tool_domain_denied",
            Self::RequestExpired => "peer_execution_request_expired",
        }
    }
}

impl fmt::Display for PeerExecutionDenialReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

#[derive(Debug, Clone)]
pub struct AssistantWorkAdmission<'a> {
    pub peer_device_id: &'a str,
    pub peer_pairing_id: &'a str,
    pub origin_runtime_id: &'a str,
    pub destination_runtime_id: &'a str,
    pub parent_session_id: &'a str,
    pub bot_id: Option<&'a str>,
    pub work_id: &'a str,
    pub correlation_id: &'a str,
    pub worker_intent: &'a str,
    pub requested_tool_domains: &'a [&'a str],
    pub request_expires_at: DateTime<Utc>,
    pub legacy_task_request_granted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerExecutionAuditEvent {
    pub event_id: String,
    pub occurred_at: DateTime<Utc>,
    pub action: String,
    pub peer_device_id: String,
    pub scope: String,
    pub policy_revision: u64,
    pub decision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_id: Option<String>,
    pub reason: String,
    pub actor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PeerExecutionPolicyFile {
    #[serde(default = "policy_file_schema_version")]
    schema_version: u32,
    #[serde(default)]
    policies: BTreeMap<String, PeerExecutionPolicy>,
    #[serde(default)]
    audit_events: Vec<PeerExecutionAuditEvent>,
}

impl Default for PeerExecutionPolicyFile {
    fn default() -> Self {
        Self {
            schema_version: POLICY_FILE_SCHEMA_VERSION,
            policies: BTreeMap::new(),
            audit_events: Vec::new(),
        }
    }
}

const fn policy_file_schema_version() -> u32 {
    POLICY_FILE_SCHEMA_VERSION
}

#[derive(Clone)]
pub struct PeerExecutionPolicyStore {
    path: PathBuf,
    io: Arc<Mutex<()>>,
}

impl Default for PeerExecutionPolicyStore {
    fn default() -> Self {
        Self::new(crate::paths::medousa_data_dir().join("mesh/execution-policies.json"))
    }
}

impl PeerExecutionPolicyStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            io: Arc::new(Mutex::new(())),
        }
    }

    pub fn policy_for_peer(
        &self,
        peer_device_id: &str,
        peer_pairing_id: &str,
        legacy_task_request_granted: bool,
    ) -> Result<PeerExecutionPolicyView> {
        let peer_device_id = validate_identity("peer device id", peer_device_id)?;
        let peer_pairing_id = validate_identity("peer pairing id", peer_pairing_id)?;
        let _guard = self.io.lock().expect("peer execution policy lock");
        let file = self.load()?;
        Ok(resolve_policy(
            &file,
            peer_device_id,
            peer_pairing_id,
            legacy_task_request_granted,
            Utc::now(),
        ))
    }

    pub fn update_policy(
        &self,
        peer_device_id: &str,
        peer_pairing_id: &str,
        update: PeerExecutionPolicyUpdate,
        actor: &str,
    ) -> Result<PeerExecutionPolicy> {
        let peer_device_id = validate_identity("peer device id", peer_device_id)?;
        let peer_pairing_id = validate_identity("peer pairing id", peer_pairing_id)?;
        let actor = validate_identity("policy actor", actor)?;
        let _guard = self.io.lock().expect("peer execution policy lock");
        let mut file = self.load()?;
        let now = Utc::now();
        let previous = file
            .policies
            .get(peer_device_id)
            .filter(|policy| policy.peer_pairing_id == peer_pairing_id);
        let revision = previous
            .map(|policy| policy.revision)
            .unwrap_or(0)
            .checked_add(1)
            .context("peer execution policy revision exhausted")?;
        let created_at = previous.map(|policy| policy.created_at).unwrap_or(now);
        let policy = compile_policy(
            peer_device_id,
            peer_pairing_id,
            update,
            revision,
            created_at,
            now,
        )?;
        file.policies
            .insert(peer_device_id.to_string(), policy.clone());
        append_audit(
            &mut file,
            PeerExecutionAuditEvent {
                event_id: format!("pea_{}", uuid::Uuid::new_v4().simple()),
                occurred_at: now,
                action: "policy.updated".to_string(),
                peer_device_id: peer_device_id.to_string(),
                scope: "policy".to_string(),
                policy_revision: revision,
                decision: "updated".to_string(),
                work_id: None,
                reason: format!("preset:{}", policy.preset.as_str()),
                actor: actor.to_string(),
            },
        );
        self.save(&file)?;
        Ok(policy)
    }

    pub fn admit_assistant_work(
        &self,
        admission: AssistantWorkAdmission<'_>,
    ) -> Result<Result<TaskExecutionGrant, PeerExecutionDenialReason>> {
        let peer_device_id = validate_identity("peer device id", admission.peer_device_id)?;
        let peer_pairing_id = validate_identity("peer pairing id", admission.peer_pairing_id)?;
        let work_id = validate_identity("work id", admission.work_id)?;
        validate_identity("origin runtime id", admission.origin_runtime_id)?;
        validate_identity("destination runtime id", admission.destination_runtime_id)?;
        validate_identity("parent session id", admission.parent_session_id)?;
        validate_identity("correlation id", admission.correlation_id)?;
        validate_identity("worker intent", admission.worker_intent)?;
        if let Some(bot_id) = admission.bot_id {
            validate_identity("bot id", bot_id)?;
        }
        if admission.requested_tool_domains.len() > 64 {
            bail!("too many requested tool domains");
        }
        for domain in admission.requested_tool_domains {
            validate_identity("requested tool domain", domain)?;
        }
        let _guard = self.io.lock().expect("peer execution policy lock");
        let mut file = self.load()?;
        let now = Utc::now();
        let view = resolve_policy(
            &file,
            peer_device_id,
            peer_pairing_id,
            admission.legacy_task_request_granted,
            now,
        );
        let decision = evaluate_assistant_work(&view.policy, &admission, now);
        let (decision_label, reason) = match &decision {
            Ok(_) => ("allowed", "assistant_work_allowed".to_string()),
            Err(reason) => ("denied", reason.code().to_string()),
        };
        append_audit(
            &mut file,
            PeerExecutionAuditEvent {
                event_id: format!("pea_{}", uuid::Uuid::new_v4().simple()),
                occurred_at: now,
                action: "task.admission".to_string(),
                peer_device_id: peer_device_id.to_string(),
                scope: "assistant_work".to_string(),
                policy_revision: view.policy.revision,
                decision: decision_label.to_string(),
                work_id: Some(work_id.to_string()),
                reason,
                actor: format!("peer:{peer_device_id}"),
            },
        );
        self.save(&file)?;
        Ok(decision.map(|mut grant| {
            grant.policy_source = view.source;
            grant
        }))
    }

    pub fn remove_policy(
        &self,
        peer_device_id: &str,
        peer_pairing_id: &str,
        actor: &str,
    ) -> Result<bool> {
        let peer_device_id = validate_identity("peer device id", peer_device_id)?;
        let peer_pairing_id = validate_identity("peer pairing id", peer_pairing_id)?;
        let actor = validate_identity("policy actor", actor)?;
        let _guard = self.io.lock().expect("peer execution policy lock");
        let mut file = self.load()?;
        let Some(policy) = file
            .policies
            .get(peer_device_id)
            .filter(|policy| policy.peer_pairing_id == peer_pairing_id)
            .cloned()
        else {
            return Ok(false);
        };
        file.policies.remove(peer_device_id);
        append_audit(
            &mut file,
            PeerExecutionAuditEvent {
                event_id: format!("pea_{}", uuid::Uuid::new_v4().simple()),
                occurred_at: Utc::now(),
                action: "policy.removed".to_string(),
                peer_device_id: peer_device_id.to_string(),
                scope: "policy".to_string(),
                policy_revision: policy.revision,
                decision: "removed".to_string(),
                work_id: None,
                reason: "pairing_removed".to_string(),
                actor: actor.to_string(),
            },
        );
        self.save(&file)?;
        Ok(true)
    }

    pub fn audit_events(&self, limit: usize) -> Result<Vec<PeerExecutionAuditEvent>> {
        let _guard = self.io.lock().expect("peer execution policy lock");
        let file = self.load()?;
        let start = file.audit_events.len().saturating_sub(limit.min(MAX_AUDIT_EVENTS));
        Ok(file.audit_events[start..].to_vec())
    }

    fn load(&self) -> Result<PeerExecutionPolicyFile> {
        if !self.path.is_file() {
            return Ok(PeerExecutionPolicyFile::default());
        }
        let metadata = std::fs::metadata(&self.path)
            .with_context(|| format!("inspect {}", self.path.display()))?;
        if metadata.len() > MAX_POLICY_FILE_BYTES {
            bail!("peer execution policy store exceeds {MAX_POLICY_FILE_BYTES} bytes");
        }
        let raw = std::fs::read(&self.path)
            .with_context(|| format!("read {}", self.path.display()))?;
        if raw.is_empty() {
            return Ok(PeerExecutionPolicyFile::default());
        }
        let file: PeerExecutionPolicyFile = serde_json::from_slice(&raw)
            .with_context(|| format!("parse {}", self.path.display()))?;
        if file.schema_version != POLICY_FILE_SCHEMA_VERSION {
            bail!(
                "unsupported peer execution policy file version {}",
                file.schema_version
            );
        }
        Ok(file)
    }

    fn save(&self, file: &PeerExecutionPolicyFile) -> Result<()> {
        let raw = serde_json::to_vec_pretty(file).context("serialize peer execution policies")?;
        if raw.len() as u64 > MAX_POLICY_FILE_BYTES {
            bail!("peer execution policy store exceeds {MAX_POLICY_FILE_BYTES} bytes");
        }
        crate::session::atomic_write(&self.path, &raw)
            .with_context(|| format!("write {}", self.path.display()))
    }
}

fn resolve_policy(
    file: &PeerExecutionPolicyFile,
    peer_device_id: &str,
    peer_pairing_id: &str,
    legacy_task_request_granted: bool,
    now: DateTime<Utc>,
) -> PeerExecutionPolicyView {
    if let Some(policy) = file
        .policies
        .get(peer_device_id)
        .filter(|policy| policy.peer_pairing_id == peer_pairing_id)
    {
        return PeerExecutionPolicyView {
            policy: policy.clone(),
            source: PeerExecutionPolicySource::Stored,
        };
    }
    if legacy_task_request_granted {
        return PeerExecutionPolicyView {
            policy: PeerExecutionPolicy::legacy_assistant(peer_device_id, peer_pairing_id, now),
            source: PeerExecutionPolicySource::LegacyTaskRequest,
        };
    }
    PeerExecutionPolicyView {
        policy: PeerExecutionPolicy::connected_only(peer_device_id, peer_pairing_id, now),
        source: PeerExecutionPolicySource::DefaultDeny,
    }
}

fn compile_policy(
    peer_device_id: &str,
    peer_pairing_id: &str,
    update: PeerExecutionPolicyUpdate,
    revision: u64,
    created_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<PeerExecutionPolicy> {
    if update.expires_at.is_some_and(|expires_at| expires_at <= now) {
        bail!("execution policy expiry must be in the future");
    }
    let mut policy = match update.preset {
        PeerExecutionPolicyPreset::ConnectedOnly => {
            PeerExecutionPolicy::connected_only(peer_device_id, peer_pairing_id, now)
        }
        PeerExecutionPolicyPreset::AssistantWork => {
            PeerExecutionPolicy::legacy_assistant(peer_device_id, peer_pairing_id, now)
        }
        PeerExecutionPolicyPreset::SandboxedWork => {
            let mut policy =
                PeerExecutionPolicy::legacy_assistant(peer_device_id, peer_pairing_id, now);
            policy.preset = PeerExecutionPolicyPreset::SandboxedWork;
            policy.sandbox_execution = true;
            policy
        }
        PeerExecutionPolicyPreset::ApprovedProjects => {
            let mut policy =
                PeerExecutionPolicy::connected_only(peer_device_id, peer_pairing_id, now);
            policy.preset = PeerExecutionPolicyPreset::ApprovedProjects;
            policy.coder_work = true;
            policy.work_environment_materialization = true;
            policy
        }
        PeerExecutionPolicyPreset::Custom => {
            let mut policy =
                PeerExecutionPolicy::connected_only(peer_device_id, peer_pairing_id, now);
            policy.preset = PeerExecutionPolicyPreset::Custom;
            policy
        }
    };
    policy.enabled = update.enabled.unwrap_or(policy.enabled);
    policy.assistant_work = update.assistant_work.unwrap_or(policy.assistant_work);
    policy.sandbox_execution = update
        .sandbox_execution
        .unwrap_or(policy.sandbox_execution);
    policy.host_shell = update.host_shell.unwrap_or(policy.host_shell);
    policy.coder_work = update.coder_work.unwrap_or(policy.coder_work);
    policy.work_environment_materialization = update
        .work_environment_materialization
        .unwrap_or(policy.work_environment_materialization);
    if let Some(values) = update.allowed_project_ids {
        policy.allowed_project_ids = validate_set("project id", values)?;
    }
    if let Some(values) = update.allowed_root_refs {
        policy.allowed_root_refs = validate_set("root reference", values)?;
    }
    if let Some(values) = update.allowed_tool_domains {
        policy.allowed_tool_domains = validate_set("tool domain", values)?;
    }
    if let Some(values) = update.allowed_mcp_server_ids {
        policy.allowed_mcp_server_ids = validate_set("MCP server id", values)?;
    }
    if let Some(values) = update.allowed_secret_refs {
        policy.allowed_secret_refs = validate_set("secret reference", values)?;
    }
    policy.network_policy = update.network_policy.unwrap_or(policy.network_policy);
    policy.allow_agent_targeting = update
        .allow_agent_targeting
        .unwrap_or(policy.allow_agent_targeting);
    policy.expires_at = update.expires_at;
    policy.revision = revision;
    policy.created_at = created_at;
    policy.updated_at = now;
    if policy.assistant_work {
        policy.allowed_tool_domains.insert("turn".to_string());
    }
    if policy.coder_work && policy.allowed_project_ids.is_empty() {
        bail!("approved-project or custom Coder policy requires at least one project id");
    }
    Ok(policy)
}

fn evaluate_assistant_work(
    policy: &PeerExecutionPolicy,
    admission: &AssistantWorkAdmission<'_>,
    now: DateTime<Utc>,
) -> Result<TaskExecutionGrant, PeerExecutionDenialReason> {
    if !policy.enabled {
        return Err(PeerExecutionDenialReason::PolicyDisabled);
    }
    if policy.is_expired_at(now) {
        return Err(PeerExecutionDenialReason::PolicyExpired);
    }
    if admission.request_expires_at <= now {
        return Err(PeerExecutionDenialReason::RequestExpired);
    }
    if !policy.assistant_work {
        return Err(PeerExecutionDenialReason::AssistantWorkDenied);
    }
    let requested = admission
        .requested_tool_domains
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>();
    let mut effective = requested
        .intersection(&policy.allowed_tool_domains)
        .cloned()
        .collect::<BTreeSet<_>>();
    if policy.network_policy == PeerNetworkPolicy::Deny {
        effective.remove("web");
    }
    if !effective.contains("turn") {
        return Err(PeerExecutionDenialReason::ToolDomainDenied);
    }
    let expires_at = policy
        .expires_at
        .map(|policy_expiry| policy_expiry.min(admission.request_expires_at))
        .unwrap_or(admission.request_expires_at);
    let grant_id = task_grant_id(admission.peer_device_id, admission.work_id, policy.revision);
    Ok(TaskExecutionGrant {
        schema_version: TASK_EXECUTION_GRANT_SCHEMA_VERSION,
        grant_id,
        peer_device_id: admission.peer_device_id.to_string(),
        peer_pairing_id: admission.peer_pairing_id.to_string(),
        origin_runtime_id: admission.origin_runtime_id.to_string(),
        destination_runtime_id: admission.destination_runtime_id.to_string(),
        parent_session_id: admission.parent_session_id.to_string(),
        bot_id: admission.bot_id.map(str::to_string),
        work_id: admission.work_id.to_string(),
        correlation_id: admission.correlation_id.to_string(),
        worker_intent: admission.worker_intent.to_string(),
        policy_revision: policy.revision,
        policy_source: PeerExecutionPolicySource::Stored,
        requested_tool_domains: requested.into_iter().collect(),
        effective_tool_domains: effective.into_iter().collect(),
        network_policy: policy.network_policy,
        issued_at: now,
        expires_at,
    })
}

fn validate_identity<'a>(label: &str, value: &'a str) -> Result<&'a str> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed != value
        || value.len() > 512
        || value.chars().any(char::is_control)
    {
        bail!("{label} is missing or invalid");
    }
    Ok(trimmed)
}

fn validate_set(label: &str, values: BTreeSet<String>) -> Result<BTreeSet<String>> {
    if values.len() > 256 {
        bail!("too many {label} values");
    }
    values
        .into_iter()
        .map(|value| validate_identity(label, &value).map(str::to_string))
        .collect()
}

fn safe_assistant_tool_domains() -> BTreeSet<String> {
    SAFE_ASSISTANT_TOOL_DOMAINS
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn task_grant_id(peer_device_id: &str, work_id: &str, policy_revision: u64) -> String {
    let mut digest = Sha256::new();
    digest.update(b"medousa/task-execution-grant/v1\0");
    digest.update(peer_device_id.as_bytes());
    digest.update([0]);
    digest.update(work_id.as_bytes());
    digest.update(policy_revision.to_be_bytes());
    format!("teg_{:x}", digest.finalize())
}

fn append_audit(file: &mut PeerExecutionPolicyFile, event: PeerExecutionAuditEvent) {
    let duplicate = event.work_id.as_ref().is_some_and(|work_id| {
        file.audit_events.iter().rev().take(32).any(|existing| {
            existing.action == event.action
                && existing.peer_device_id == event.peer_device_id
                && existing.policy_revision == event.policy_revision
                && existing.decision == event.decision
                && existing.work_id.as_ref() == Some(work_id)
        })
    });
    if duplicate {
        return;
    }
    file.audit_events.push(event);
    if file.audit_events.len() > MAX_AUDIT_EVENTS {
        let remove = file.audit_events.len() - MAX_AUDIT_EVENTS;
        file.audit_events.drain(0..remove);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> (PeerExecutionPolicyStore, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "medousa-peer-execution-policy-{}",
            uuid::Uuid::new_v4().simple()
        ));
        (PeerExecutionPolicyStore::new(root.join("policies.json")), root)
    }

    fn admission<'a>(peer: &'a str, work: &'a str) -> AssistantWorkAdmission<'a> {
        AssistantWorkAdmission {
            peer_device_id: peer,
            peer_pairing_id: "pairing-1",
            origin_runtime_id: peer,
            destination_runtime_id: "runtime-local",
            parent_session_id: "session-1",
            bot_id: None,
            work_id: work,
            correlation_id: "correlation-1",
            worker_intent: "research",
            requested_tool_domains: &SAFE_ASSISTANT_TOOL_DOMAINS,
            request_expires_at: Utc::now() + chrono::Duration::minutes(5),
            legacy_task_request_granted: false,
        }
    }

    #[test]
    fn policy_is_directional_and_survives_reopen() {
        let (store, root) = test_store();
        let policy = store
            .update_policy(
                "peer-a",
                "pairing-1",
                PeerExecutionPolicyUpdate {
                    preset: PeerExecutionPolicyPreset::AssistantWork,
                    ..Default::default()
                },
                "local:operator",
            )
            .unwrap();
        assert!(policy.assistant_work);
        assert_eq!(policy.revision, 1);
        assert!(
            !store
                .policy_for_peer("peer-b", "pairing-2", false)
                .unwrap()
                .policy
                .assistant_work
        );

        let reopened = PeerExecutionPolicyStore::new(store.path.clone());
        assert!(
            reopened
                .policy_for_peer("peer-a", "pairing-1", false)
                .unwrap()
                .policy
                .assistant_work
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn removing_and_repairing_a_device_does_not_restore_old_authority() {
        let (store, root) = test_store();
        store
            .update_policy(
                "peer-a",
                "pairing-old",
                PeerExecutionPolicyUpdate {
                    preset: PeerExecutionPolicyPreset::AssistantWork,
                    ..Default::default()
                },
                "local:operator",
            )
            .unwrap();

        let repaired = store
            .policy_for_peer("peer-a", "pairing-new", false)
            .unwrap();
        assert_eq!(repaired.source, PeerExecutionPolicySource::DefaultDeny);
        assert!(!repaired.policy.assistant_work);
        assert_eq!(repaired.policy.peer_pairing_id, "pairing-new");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn pairing_alone_cannot_admit_assistant_work() {
        let (store, root) = test_store();
        let denial = store
            .admit_assistant_work(admission("peer-a", "work-a"))
            .unwrap()
            .unwrap_err();
        assert_eq!(denial, PeerExecutionDenialReason::AssistantWorkDenied);
        assert!(store.audit_events(10).unwrap()[0].reason.contains("denied"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_task_request_maps_only_to_safe_assistant_work() {
        let (store, root) = test_store();
        let view = store
            .policy_for_peer("peer-a", "pairing-1", true)
            .unwrap();
        assert_eq!(view.source, PeerExecutionPolicySource::LegacyTaskRequest);
        assert!(view.policy.assistant_work);
        assert!(!view.policy.sandbox_execution);
        assert!(!view.policy.host_shell);
        assert!(!view.policy.coder_work);
        assert!(view.policy.allowed_secret_refs.is_empty());
        assert!(!view.policy.allow_agent_targeting);

        let mut request = admission("peer-a", "work-a");
        request.legacy_task_request_granted = true;
        let grant = store.admit_assistant_work(request).unwrap().unwrap();
        assert_eq!(
            grant.policy_source,
            PeerExecutionPolicySource::LegacyTaskRequest
        );
        assert_eq!(
            grant.effective_tool_domains,
            vec![
                "turn".to_string(),
                "utility".to_string(),
                "web".to_string()
            ]
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn stored_policy_intersects_requested_domains_and_stamps_revision() {
        let (store, root) = test_store();
        store
            .update_policy(
                "peer-a",
                "pairing-1",
                PeerExecutionPolicyUpdate {
                    preset: PeerExecutionPolicyPreset::Custom,
                    assistant_work: Some(true),
                    allowed_tool_domains: Some(BTreeSet::from(["turn".to_string()])),
                    ..Default::default()
                },
                "local:operator",
            )
            .unwrap();

        let grant = store
            .admit_assistant_work(admission("peer-a", "work-a"))
            .unwrap()
            .unwrap();
        assert_eq!(grant.policy_revision, 1);
        assert_eq!(grant.policy_source, PeerExecutionPolicySource::Stored);
        assert_eq!(grant.effective_tool_domains, vec!["turn".to_string()]);
        let _ = std::fs::remove_dir_all(root);
    }
}
