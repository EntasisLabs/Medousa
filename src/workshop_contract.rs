//! Location-neutral workshop worker request contract.

use std::collections::BTreeSet;
use std::fmt;

use chrono::{DateTime, Utc};
use medousa_world::{
    WorldDriverCapability, WorldDriverKind, WorldDriverRegistration, WorldOwnership,
    WorldSurfaceKind,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use stasis::domain::runtime::placement::{PlacementConstraints, WorkerCapabilities};

use crate::public_api::COGNITION_WORKSHOP_MUTATE;
use crate::schema_api::{TypedActionSchema, typed_action_schema};
use crate::typed_tools::ToolId;

const WORKSHOP_MUTATE_ID: ToolId = ToolId::new(COGNITION_WORKSHOP_MUTATE);

pub use crate::daemon_api::{ExecutionTargetRequirements, ExecutionTargetSelection};

pub const UNKNOWN_EXECUTION_RUNTIME_ID: &str = "unknown";
pub const EXECUTION_TARGET_INVENTORY_SCHEMA_VERSION: u32 = 1;

/// Mechanical world-driver capabilities carried by the existing authorized
/// execution-target inventory. These strings describe what is colocated with
/// a workshop; they never grant a caller authority to use it.
pub fn world_driver_execution_capabilities(
    registrations: &[WorldDriverRegistration],
    isolated_browser_available: bool,
) -> BTreeSet<String> {
    let mut capabilities = BTreeSet::new();
    if isolated_browser_available {
        extend_world_driver_capabilities(
            &mut capabilities,
            WorldDriverKind::IsolatedBrowser,
            WorldSurfaceKind::Browser,
            WorldOwnership::Owned,
            &[
                WorldDriverCapability::SemanticObservation,
                WorldDriverCapability::PixelObservation,
                WorldDriverCapability::Navigation,
                WorldDriverCapability::Interaction,
                WorldDriverCapability::GuardedBatch,
                WorldDriverCapability::HumanTakeover,
                WorldDriverCapability::PersistentProfile,
            ]
            .into_iter()
            .collect(),
        );
    }
    for registration in registrations {
        extend_world_driver_capabilities(
            &mut capabilities,
            registration.kind,
            registration.surface,
            registration.ownership,
            &registration.capabilities,
        );
    }
    capabilities
}

fn extend_world_driver_capabilities(
    output: &mut BTreeSet<String>,
    kind: WorldDriverKind,
    surface: WorldSurfaceKind,
    ownership: WorldOwnership,
    capabilities: &BTreeSet<WorldDriverCapability>,
) {
    let family = match surface {
        WorldSurfaceKind::Browser => "browser",
        WorldSurfaceKind::Desktop
        | WorldSurfaceKind::Application
        | WorldSurfaceKind::Terminal
        | WorldSurfaceKind::Composite => "computer",
    };
    output.insert(format!("world.{family}"));
    output.insert(format!(
        "world.{family}.surface.{}",
        world_surface_name(surface)
    ));
    output.insert(format!(
        "world.{family}.ownership.{}",
        world_ownership_name(ownership)
    ));
    output.insert(format!(
        "world.{family}.driver.{}",
        world_driver_kind_name(kind)
    ));
    for capability in capabilities {
        output.insert(format!(
            "world.{family}.capability.{}",
            world_driver_capability_name(*capability)
        ));
    }
}

const fn world_surface_name(surface: WorldSurfaceKind) -> &'static str {
    match surface {
        WorldSurfaceKind::Browser => "browser",
        WorldSurfaceKind::Desktop => "desktop",
        WorldSurfaceKind::Application => "application",
        WorldSurfaceKind::Terminal => "terminal",
        WorldSurfaceKind::Composite => "composite",
    }
}

const fn world_ownership_name(ownership: WorldOwnership) -> &'static str {
    match ownership {
        WorldOwnership::Owned => "owned",
        WorldOwnership::Managed => "managed",
        WorldOwnership::Attached => "attached",
    }
}

const fn world_driver_kind_name(kind: WorldDriverKind) -> &'static str {
    match kind {
        WorldDriverKind::EmbeddedBrowser => "embedded_browser",
        WorldDriverKind::BrowserExtension => "browser_extension",
        WorldDriverKind::MobileBrowser => "mobile_browser",
        WorldDriverKind::IsolatedBrowser => "isolated_browser",
        WorldDriverKind::NativeDesktop => "native_desktop",
    }
}

const fn world_driver_capability_name(capability: WorldDriverCapability) -> &'static str {
    match capability {
        WorldDriverCapability::SemanticObservation => "semantic_observation",
        WorldDriverCapability::PixelObservation => "pixel_observation",
        WorldDriverCapability::Navigation => "navigation",
        WorldDriverCapability::Interaction => "interaction",
        WorldDriverCapability::GuardedBatch => "guarded_batch",
        WorldDriverCapability::HumanTakeover => "human_takeover",
        WorldDriverCapability::PersistentProfile => "persistent_profile",
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionResolutionReason {
    SameAsParent,
    ExactTarget,
    AutoMatch,
    IngressDefault,
    #[default]
    LegacyUnknown,
}

fn legacy_resolution_time() -> DateTime<Utc> {
    DateTime::UNIX_EPOCH
}

/// Immutable placement provenance captured before a worker is enqueued.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExecutionPlacementResolution {
    #[serde(default)]
    pub requested: ExecutionTargetSelection,
    #[serde(default = "default_unknown_runtime_id")]
    pub resolved_runtime_id: String,
    #[serde(default)]
    pub resolution_reason: ExecutionResolutionReason,
    #[serde(default = "legacy_resolution_time")]
    pub resolved_at: DateTime<Utc>,
}

impl Default for ExecutionPlacementResolution {
    fn default() -> Self {
        Self {
            requested: ExecutionTargetSelection::SameAsParent,
            resolved_runtime_id: default_unknown_runtime_id(),
            resolution_reason: ExecutionResolutionReason::LegacyUnknown,
            resolved_at: legacy_resolution_time(),
        }
    }
}

impl ExecutionPlacementResolution {
    pub fn resolved(
        requested: ExecutionTargetSelection,
        runtime_id: impl Into<String>,
        resolution_reason: ExecutionResolutionReason,
    ) -> Self {
        Self {
            requested,
            resolved_runtime_id: runtime_id.into(),
            resolution_reason,
            resolved_at: Utc::now(),
        }
    }
}

pub fn default_unknown_runtime_id() -> String {
    UNKNOWN_EXECUTION_RUNTIME_ID.to_string()
}

#[derive(Debug, Clone)]
pub struct ExecutionTargetCandidate {
    pub runtime_id: String,
    pub label: String,
    pub capabilities: WorkerCapabilities,
    /// Explicit user placement is admitted independently from model routing.
    pub user_selectable: bool,
    /// Automatic and model-selected placement require the destination owner to
    /// opt in through its directional peer policy.
    pub agent_selectable: bool,
}

impl ExecutionTargetCandidate {
    pub fn local(runtime_id: impl Into<String>, capabilities: WorkerCapabilities) -> Self {
        let runtime_id = runtime_id.into();
        Self {
            label: "This workshop".to_string(),
            runtime_id,
            capabilities,
            user_selectable: true,
            agent_selectable: true,
        }
    }

    pub fn inventory_entry(&self) -> ExecutionTargetInventoryEntry {
        ExecutionTargetInventoryEntry {
            runtime_id: self.runtime_id.clone(),
            label: self.label.clone(),
            capabilities: self
                .capabilities
                .capabilities
                .iter()
                .cloned()
                .collect(),
            platform: self.capabilities.platform.clone(),
            architecture: self.capabilities.architecture.clone(),
            region: self.capabilities.region.clone(),
            user_selectable: self.user_selectable,
            agent_selectable: self.agent_selectable,
        }
    }

    pub fn from_inventory_entry(entry: ExecutionTargetInventoryEntry) -> Self {
        let capabilities = WorkerCapabilities {
            capabilities: entry.capabilities,
            platform: entry.platform,
            architecture: entry.architecture,
            region: entry.region,
            node_id: Some(entry.runtime_id.clone()),
        };
        Self {
            runtime_id: entry.runtime_id,
            label: entry.label,
            capabilities,
            user_selectable: entry.user_selectable,
            agent_selectable: entry.agent_selectable,
        }
    }
}

/// Sanitized execution authority advertised to Home or an admitted agent.
/// Runtime ids are opaque identities; connection URLs and credentials never
/// cross this contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExecutionTargetInventoryEntry {
    pub runtime_id: String,
    pub label: String,
    #[serde(
        default,
        skip_serializing_if = "std::collections::BTreeSet::is_empty"
    )]
    pub capabilities: std::collections::BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    pub user_selectable: bool,
    pub agent_selectable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExecutionTargetInventory {
    pub schema_version: u32,
    pub parent_runtime_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_runtime_id: Option<String>,
    pub targets: Vec<ExecutionTargetInventoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionTargetProbeRequest {
    pub schema_version: u32,
}

impl Default for ExecutionTargetProbeRequest {
    fn default() -> Self {
        Self {
            schema_version: EXECUTION_TARGET_INVENTORY_SCHEMA_VERSION,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionTargetProbeResponse {
    pub schema_version: u32,
    pub target: ExecutionTargetInventoryEntry,
    pub policy_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionTargetResolutionError {
    InvalidRuntimeId,
    ParentUnavailable { runtime_id: String },
    ExactUnavailable { runtime_id: String },
    NoCapableTarget,
    UnsupportedTarget { detail: String },
}

impl ExecutionTargetResolutionError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidRuntimeId => "execution_target_invalid",
            Self::ParentUnavailable { .. } | Self::ExactUnavailable { .. } => {
                "execution_target_unavailable"
            }
            Self::NoCapableTarget => "execution_target_no_capable_runtime",
            Self::UnsupportedTarget { .. } => "execution_target_unsupported",
        }
    }
}

impl fmt::Display for ExecutionTargetResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let detail = match self {
            Self::InvalidRuntimeId => "exact runtime_id is empty or invalid".to_string(),
            Self::ParentUnavailable { runtime_id } => {
                format!("parent runtime '{runtime_id}' is unavailable")
            }
            Self::ExactUnavailable { runtime_id } => {
                format!("exact runtime '{runtime_id}' is unavailable")
            }
            Self::NoCapableTarget => {
                "no available runtime satisfies the placement requirements".to_string()
            }
            Self::UnsupportedTarget { detail } => detail.clone(),
        };
        write!(formatter, "{}: {detail}", self.code())
    }
}

impl std::error::Error for ExecutionTargetResolutionError {}

/// Resolve against an already-authorized candidate inventory. Candidate order
/// is preference order for `auto`; exact and parent selection never fall back.
pub fn resolve_execution_target(
    requested: ExecutionTargetSelection,
    parent_runtime_id: &str,
    candidates: &[ExecutionTargetCandidate],
) -> Result<ExecutionPlacementResolution, ExecutionTargetResolutionError> {
    let parent_runtime_id = parent_runtime_id.trim();
    let (runtime_id, reason, requested) = match requested {
        ExecutionTargetSelection::SameAsParent => {
            let candidate = candidates
                .iter()
                .find(|candidate| candidate.runtime_id == parent_runtime_id)
                .ok_or_else(|| ExecutionTargetResolutionError::ParentUnavailable {
                    runtime_id: parent_runtime_id.to_string(),
                })?;
            (
                candidate.runtime_id.clone(),
                ExecutionResolutionReason::SameAsParent,
                ExecutionTargetSelection::SameAsParent,
            )
        }
        ExecutionTargetSelection::Exact { runtime_id } => {
            let runtime_id = runtime_id.trim();
            if runtime_id.is_empty()
                || runtime_id.len() > 256
                || !runtime_id
                    .bytes()
                    .all(|byte| matches!(byte, 0x21..=0x7e))
            {
                return Err(ExecutionTargetResolutionError::InvalidRuntimeId);
            }
            let placement = PlacementConstraints::unrestricted().target_node(runtime_id);
            let candidate = candidates
                .iter()
                .find(|candidate| placement.matches(&candidate.capabilities))
                .ok_or_else(|| ExecutionTargetResolutionError::ExactUnavailable {
                    runtime_id: runtime_id.to_string(),
                })?;
            let runtime_id = candidate.runtime_id.clone();
            (
                runtime_id.clone(),
                ExecutionResolutionReason::ExactTarget,
                ExecutionTargetSelection::Exact { runtime_id },
            )
        }
        ExecutionTargetSelection::Auto { requirements } => {
            let placement = PlacementConstraints {
                required_capabilities: requirements.required_capabilities.clone(),
                platform: requirements.platform.clone(),
                architecture: requirements.architecture.clone(),
                region: requirements.region.clone(),
                target_node: None,
            };
            let mut eligible = candidates
                .iter()
                .filter(|candidate| placement.matches(&candidate.capabilities))
                .collect::<Vec<_>>();
            eligible.sort_by(|left, right| left.runtime_id.cmp(&right.runtime_id));
            let index = deterministic_auto_index(
                requirements.selection_key.as_deref(),
                eligible.iter().map(|candidate| candidate.runtime_id.as_str()),
                eligible.len(),
            );
            let candidate = eligible
                .get(index)
                .copied()
                .ok_or(ExecutionTargetResolutionError::NoCapableTarget)?;
            (
                candidate.runtime_id.clone(),
                ExecutionResolutionReason::AutoMatch,
                ExecutionTargetSelection::Auto { requirements },
            )
        }
    };
    Ok(ExecutionPlacementResolution::resolved(
        requested, runtime_id, reason,
    ))
}

fn deterministic_auto_index<'a>(
    selection_key: Option<&str>,
    runtime_ids: impl Iterator<Item = &'a str>,
    candidate_count: usize,
) -> usize {
    if candidate_count <= 1 {
        return 0;
    }
    let Some(selection_key) = selection_key.map(str::trim).filter(|key| !key.is_empty()) else {
        return 0;
    };
    let mut digest = Sha256::new();
    digest.update(selection_key.as_bytes());
    for runtime_id in runtime_ids {
        digest.update([0]);
        digest.update(runtime_id.as_bytes());
    }
    let bytes: [u8; 8] = digest.finalize()[..8]
        .try_into()
        .expect("sha256 prefix has a fixed length");
    (u64::from_be_bytes(bytes) as usize) % candidate_count
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WorkshopSpawn {
    /// Worker profile: memory.avec_calibrate | memory.context | research | general
    #[serde(default)]
    pub(crate) intent: Option<String>,
    /// Focused task for the worker
    pub(crate) task: String,
    /// Short message for the user while the worker runs
    pub(crate) user_ack: String,
    /// Optional YAML specialty
    #[serde(default)]
    pub(crate) manuscript_id: Option<String>,
    /// Optional StageRoutingMatrix role
    #[serde(default)]
    pub(crate) stage_role: Option<String>,
    /// Prefer omit or auto; only set provider:model when explicitly requested
    #[serde(default)]
    pub(crate) model_hint: Option<String>,
    /// Optional execution placement. Omitted means same as the parent turn.
    #[serde(default)]
    pub(crate) execution_target: Option<ExecutionTargetSelection>,
    /// Optional subset of opaque world ids from MEDOUSA_ELIGIBLE_WORLDS. The
    /// daemon resolves them against exact worker placement before spawning.
    #[serde(default)]
    pub(crate) world_ids: Vec<String>,
}

pub fn workshop_spawn_type_schema() -> TypedActionSchema {
    typed_action_schema::<WorkshopSpawn>(
        WORKSHOP_MUTATE_ID,
        "workshop.spawn",
        "Delegate heavy work to a background turn worker",
    )
}

#[cfg(all(feature = "embedded-daemon", not(feature = "full-daemon")))]
pub fn workshop_type_schemas() -> Vec<TypedActionSchema> {
    vec![workshop_spawn_type_schema()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn candidate(runtime_id: &str) -> ExecutionTargetCandidate {
        ExecutionTargetCandidate {
            runtime_id: runtime_id.to_string(),
            label: runtime_id.to_string(),
            capabilities: WorkerCapabilities::any()
                .node_id(runtime_id)
                .platform("macos")
                .architecture("aarch64")
                .with_capability("assistant.work"),
            user_selectable: true,
            agent_selectable: true,
        }
    }

    #[test]
    fn omitted_spawn_target_remains_same_as_parent() {
        let spawn: WorkshopSpawn = serde_json::from_value(serde_json::json!({
            "task": "research",
            "user_ack": "On it"
        }))
        .expect("spawn");
        assert!(spawn.execution_target.is_none());
        assert!(spawn.world_ids.is_empty());
    }

    #[test]
    fn spawn_accepts_only_opaque_world_identifiers() {
        let spawn: WorkshopSpawn = serde_json::from_value(serde_json::json!({
            "task": "research",
            "user_ack": "On it",
            "world_ids": ["world:browser:research"]
        }))
        .expect("spawn");
        assert_eq!(spawn.world_ids, ["world:browser:research"]);
    }

    #[test]
    fn world_driver_capabilities_are_scoped_to_surface_and_mechanics() {
        let native = WorldDriverRegistration {
            driver_id: medousa_world::WorldDriverId::new("driver:computer:test"),
            kind: WorldDriverKind::NativeDesktop,
            surface: WorldSurfaceKind::Desktop,
            ownership: WorldOwnership::Attached,
            transport: medousa_world::WorldDriverTransport::LocalSidecar,
            capabilities: [
                WorldDriverCapability::SemanticObservation,
                WorldDriverCapability::Interaction,
                WorldDriverCapability::HumanTakeover,
            ]
            .into_iter()
            .collect(),
            display_name: Some("Test desktop".to_string()),
        };

        let capabilities = world_driver_execution_capabilities(&[native], true);

        assert!(capabilities.contains("world.browser"));
        assert!(capabilities.contains("world.browser.driver.isolated_browser"));
        assert!(capabilities.contains("world.browser.capability.persistent_profile"));
        assert!(capabilities.contains("world.computer"));
        assert!(capabilities.contains("world.computer.surface.desktop"));
        assert!(capabilities.contains("world.computer.ownership.attached"));
        assert!(capabilities.contains("world.computer.driver.native_desktop"));
        assert!(capabilities.contains("world.computer.capability.human_takeover"));
        assert!(!capabilities.contains("world.computer.capability.navigation"));
    }

    #[test]
    fn exact_target_never_falls_back() {
        let candidates = vec![candidate("runtime-local")];
        let error = resolve_execution_target(
            ExecutionTargetSelection::Exact {
                runtime_id: "runtime-offline".to_string(),
            },
            "runtime-local",
            &candidates,
        )
        .expect_err("exact offline target");
        assert_eq!(error.code(), "execution_target_unavailable");
    }

    #[test]
    fn auto_uses_stasis_capability_matching() {
        let candidates = vec![candidate("runtime-local")];
        let resolution = resolve_execution_target(
            ExecutionTargetSelection::Auto {
                requirements: ExecutionTargetRequirements {
                    required_capabilities: BTreeSet::from(["assistant.work".to_string()]),
                    platform: Some("macos".to_string()),
                    architecture: Some("aarch64".to_string()),
                    region: None,
                    selection_key: Some("turn-42".to_string()),
                },
            },
            "runtime-parent",
            &candidates,
        )
        .expect("auto match");
        assert_eq!(resolution.resolved_runtime_id, "runtime-local");
        assert_eq!(
            resolution.resolution_reason,
            ExecutionResolutionReason::AutoMatch
        );
    }

    #[test]
    fn auto_is_deterministic_for_the_same_key_and_candidate_set() {
        let requested = ExecutionTargetSelection::Auto {
            requirements: ExecutionTargetRequirements {
                required_capabilities: BTreeSet::from(["assistant.work".to_string()]),
                selection_key: Some("session-42".to_string()),
                ..ExecutionTargetRequirements::default()
            },
        };
        let forward = vec![candidate("runtime-c"), candidate("runtime-a"), candidate("runtime-b")];
        let reverse = vec![candidate("runtime-b"), candidate("runtime-c"), candidate("runtime-a")];
        let first = resolve_execution_target(requested.clone(), "runtime-parent", &forward)
            .expect("forward auto");
        let second = resolve_execution_target(requested, "runtime-parent", &reverse)
            .expect("reverse auto");
        assert_eq!(first.resolved_runtime_id, second.resolved_runtime_id);
    }
}
