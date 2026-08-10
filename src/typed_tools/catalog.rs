use std::borrow::Borrow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};
use std::sync::{Arc, OnceLock};

use genai::chat::Tool;
use serde::{Serialize, Serializer};
use serde_json::Value;
use stasis::application::orchestration::tool_registry::{InMemoryToolRegistry, StasisTool};
use stasis::domain::errors::{Result as StasisResult, StasisError};

use super::TypedTool;

/// Validated identity for a statically known first-party tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ToolId(&'static str);

impl ToolId {
    /// Creates a tool id and fails compilation when a typed constant is invalid.
    pub const fn new(value: &'static str) -> Self {
        match Self::try_new(value) {
            Ok(tool_id) => tool_id,
            Err(_) => panic!("invalid tool id"),
        }
    }

    pub const fn try_new(value: &'static str) -> Result<Self, ToolIdError> {
        if is_valid_tool_id(value.as_bytes()) {
            Ok(Self(value))
        } else {
            Err(ToolIdError { value })
        }
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl AsRef<str> for ToolId {
    fn as_ref(&self) -> &str {
        self.0
    }
}

impl Borrow<str> for ToolId {
    fn borrow(&self) -> &str {
        self.0
    }
}

impl Display for ToolId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Serialize for ToolId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolIdError {
    value: &'static str,
}

impl ToolIdError {
    pub const fn value(self) -> &'static str {
        self.value
    }
}

impl Display for ToolIdError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "tool id `{}` must start with an ASCII letter and contain only ASCII letters, digits, `_`, or `-`",
            self.value
        )
    }
}

impl std::error::Error for ToolIdError {}

/// Compatibility input accepted by the macro while legacy name constants are
/// progressively converted to [`ToolId`].
pub trait ToolIdSource {
    fn resolve_tool_id(self) -> ToolId;
}

impl ToolIdSource for ToolId {
    fn resolve_tool_id(self) -> ToolId {
        self
    }
}

impl ToolIdSource for &'static str {
    fn resolve_tool_id(self) -> ToolId {
        ToolId::try_new(self).unwrap_or_else(|error| panic!("{error}"))
    }
}

pub fn resolve_tool_id(source: impl ToolIdSource) -> ToolId {
    source.resolve_tool_id()
}

macro_rules! typed_catalog_id {
    ($name:ident, $kind:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(&'static str);

        impl $name {
            pub const fn new(value: &'static str) -> Self {
                if !is_valid_tool_id(value.as_bytes()) {
                    panic!(concat!("invalid ", $kind));
                }
                Self(value)
            }

            pub const fn as_str(self) -> &'static str {
                self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.0)
            }
        }
    };
}

typed_catalog_id!(ToolDomainId, "tool domain id");
typed_catalog_id!(ToolModeId, "tool mode id");
typed_catalog_id!(ToolSurfaceId, "tool surface id");
typed_catalog_id!(ToolPolicyId, "tool policy id");
typed_catalog_id!(ToolCapabilityId, "tool capability id");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ToolEffect {
    Observe,
    Mutate,
    Execute,
    Coordinate,
    Present,
    /// Compatibility marker removed as legacy contracts migrate.
    LegacyUnclassified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ToolCapabilityRequirement {
    pub capability: ToolCapabilityId,
}

impl ToolCapabilityRequirement {
    pub const fn new(capability: ToolCapabilityId) -> Self {
        Self { capability }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ToolExposureQualifier {
    Domain(ToolDomainId),
    Policy(ToolPolicyId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ToolExposureRef {
    pub mode: ToolModeId,
    pub surface: ToolSurfaceId,
    pub qualifier: Option<ToolExposureQualifier>,
}

impl ToolExposureRef {
    pub const fn new(mode: ToolModeId, surface: ToolSurfaceId) -> Self {
        Self {
            mode,
            surface,
            qualifier: None,
        }
    }

    pub const fn domain(mode: ToolModeId, surface: ToolSurfaceId, domain: ToolDomainId) -> Self {
        Self {
            mode,
            surface,
            qualifier: Some(ToolExposureQualifier::Domain(domain)),
        }
    }

    pub const fn policy(mode: ToolModeId, surface: ToolSurfaceId, policy: ToolPolicyId) -> Self {
        Self {
            mode,
            surface,
            qualifier: Some(ToolExposureQualifier::Policy(policy)),
        }
    }

    pub fn label(self) -> String {
        let mut label = format!("{}:{}", self.mode, self.surface);
        if let Some(qualifier) = self.qualifier {
            let value = match qualifier {
                ToolExposureQualifier::Domain(value) => value.as_str(),
                ToolExposureQualifier::Policy(value) => value.as_str(),
            };
            label.push(':');
            label.push_str(value);
        }
        label
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPlacement {
    pub effect: ToolEffect,
    pub capability_requirements: BTreeSet<ToolCapabilityRequirement>,
    pub exposures: BTreeSet<ToolExposureRef>,
    pub presentation_summary: Option<&'static str>,
}

impl Default for ToolPlacement {
    fn default() -> Self {
        Self {
            effect: ToolEffect::LegacyUnclassified,
            capability_requirements: BTreeSet::new(),
            exposures: BTreeSet::new(),
            presentation_summary: None,
        }
    }
}

impl ToolPlacement {
    pub fn exposes(&self, exposure: ToolExposureRef) -> bool {
        self.exposures.contains(&exposure)
    }

    pub fn exposes_mode(&self, mode: ToolModeId) -> bool {
        self.exposures.iter().any(|exposure| exposure.mode == mode)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ToolPlacementIndex {
    placements: BTreeMap<ToolId, ToolPlacement>,
}

impl ToolPlacementIndex {
    pub fn placement_mut(&mut self, id: ToolId) -> &mut ToolPlacement {
        self.placements.entry(id).or_default()
    }

    pub fn placement(&self, id: ToolId) -> ToolPlacement {
        self.placements.get(&id).cloned().unwrap_or_default()
    }

    pub fn add_exposure(&mut self, id: ToolId, exposure: ToolExposureRef) {
        self.placement_mut(id).exposures.insert(exposure);
    }

    pub fn set_effect(&mut self, id: ToolId, effect: ToolEffect) {
        self.placement_mut(id).effect = effect;
    }

    pub fn require_capability(&mut self, id: ToolId, capability: ToolCapabilityId) {
        self.placement_mut(id)
            .capability_requirements
            .insert(ToolCapabilityRequirement::new(capability));
    }

    pub fn set_presentation_summary(&mut self, id: ToolId, summary: &'static str) {
        self.placement_mut(id).presentation_summary = Some(summary);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisteredToolKind {
    Typed,
    Legacy,
    RuntimeAdapter,
}

#[derive(Debug, Clone)]
pub struct RegisteredToolContract {
    pub definition: Tool,
    pub output_schema: Option<Value>,
    pub kind: RegisteredToolKind,
}

#[derive(Debug, Clone)]
pub struct ToolCatalogEntry {
    pub id: ToolId,
    pub contract: RegisteredToolContract,
    pub placement: ToolPlacement,
}

#[derive(Debug, Clone, Default)]
pub struct ToolCatalog {
    entries: BTreeMap<ToolId, ToolCatalogEntry>,
}

impl ToolCatalog {
    pub fn entries(&self) -> impl Iterator<Item = &ToolCatalogEntry> {
        self.entries.values()
    }

    pub fn get(&self, id: ToolId) -> Option<&ToolCatalogEntry> {
        self.entries.get(&id)
    }

    /// Resolve a provider/client string exactly once at the registry boundary.
    pub fn resolve_wire_id(&self, wire_name: &str) -> Result<ToolId, ToolCatalogError> {
        self.entries
            .get_key_value(wire_name)
            .map(|(id, _)| *id)
            .ok_or_else(|| ToolCatalogError::UnknownTool(wire_name.to_string()))
    }

    pub fn definitions_matching(
        &self,
        mut predicate: impl FnMut(&ToolCatalogEntry) -> bool,
    ) -> Vec<Tool> {
        self.entries
            .values()
            .filter(|entry| predicate(entry))
            .map(|entry| entry.contract.definition.clone())
            .collect()
    }

    pub fn presentation_summary(&self, id: ToolId) -> String {
        let Some(entry) = self.entries.get(&id) else {
            return "Session-unlocked tool — see cognition_tools_discover catalog".to_string();
        };
        if let Some(summary) = entry.placement.presentation_summary {
            return summary.to_string();
        }
        entry
            .contract
            .definition
            .description
            .as_deref()
            .and_then(first_sentence)
            .unwrap_or("Session-unlocked tool — see cognition_tools_discover catalog")
            .to_string()
    }

    pub fn presentation_summary_for_wire(&self, wire_name: &str) -> String {
        self.resolve_wire_id(wire_name)
            .map(|id| self.presentation_summary(id))
            .unwrap_or_else(|_| {
                "Session-unlocked tool — see cognition_tools_discover catalog".to_string()
            })
    }
}

fn first_sentence(description: &str) -> Option<&str> {
    let description = description.trim();
    if description.is_empty() {
        return None;
    }
    let end = description
        .char_indices()
        .find_map(|(index, character)| (character == '.').then_some(index + 1))
        .unwrap_or(description.len());
    Some(description[..end].trim())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCatalogError {
    DuplicateTool(ToolId),
    InvalidToolId(String),
    UnknownTool(String),
    ContractDrift { id: ToolId, field: &'static str },
    CatalogAlreadyInitialized,
}

impl Display for ToolCatalogError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateTool(id) => write!(formatter, "duplicate registered tool id: {id}"),
            Self::InvalidToolId(name) => write!(formatter, "invalid registered tool id: {name}"),
            Self::UnknownTool(name) => {
                write!(formatter, "tool is not in the assembled catalog: {name}")
            }
            Self::ContractDrift { id, field } => {
                write!(formatter, "typed tool contract drift for {id}: {field}")
            }
            Self::CatalogAlreadyInitialized => {
                formatter.write_str("tool catalog already initialized")
            }
        }
    }
}

impl std::error::Error for ToolCatalogError {}

/// Registration port used by both the real catalog registrar and legacy tests
/// that still assemble a bare Stasis registry.
pub trait ToolRegistration {
    fn register_tool<T: StasisTool + 'static>(&mut self, tool: T) -> StasisResult<()>;

    fn register_typed_tool<T: StasisTool + TypedTool + 'static>(
        &mut self,
        tool: T,
    ) -> StasisResult<()>;
}

impl ToolRegistration for InMemoryToolRegistry {
    fn register_tool<T: StasisTool + 'static>(&mut self, tool: T) -> StasisResult<()> {
        InMemoryToolRegistry::register_tool(self, tool)
    }

    fn register_typed_tool<T: StasisTool + TypedTool + 'static>(
        &mut self,
        tool: T,
    ) -> StasisResult<()> {
        InMemoryToolRegistry::register_tool(self, tool)
    }
}

pub struct ToolRegistrar {
    registry: InMemoryToolRegistry,
    placements: ToolPlacementIndex,
    catalog: ToolCatalog,
}

impl ToolRegistrar {
    pub fn new(placements: ToolPlacementIndex) -> Self {
        Self {
            registry: InMemoryToolRegistry::default(),
            placements,
            catalog: ToolCatalog::default(),
        }
    }

    pub fn register_runtime_adapter(
        &mut self,
        id: ToolId,
        definition: Tool,
        output_schema: Option<Value>,
    ) -> Result<(), ToolCatalogError> {
        if definition.name.as_str() != id.as_str() {
            return Err(ToolCatalogError::ContractDrift {
                id,
                field: "runtime adapter name",
            });
        }
        self.insert_catalog_entry(
            id,
            RegisteredToolContract {
                definition,
                output_schema,
                kind: RegisteredToolKind::RuntimeAdapter,
            },
        )
    }

    pub fn finish(self) -> (InMemoryToolRegistry, Arc<ToolCatalog>) {
        (self.registry, Arc::new(self.catalog))
    }

    fn insert_catalog_entry(
        &mut self,
        id: ToolId,
        contract: RegisteredToolContract,
    ) -> Result<(), ToolCatalogError> {
        if self.catalog.entries.contains_key(&id) {
            return Err(ToolCatalogError::DuplicateTool(id));
        }
        self.catalog.entries.insert(
            id,
            ToolCatalogEntry {
                id,
                contract,
                placement: self.placements.placement(id),
            },
        );
        Ok(())
    }

    fn capture<T: StasisTool>(
        &self,
        tool: &T,
        kind: RegisteredToolKind,
    ) -> Result<(ToolId, RegisteredToolContract), ToolCatalogError> {
        let id = ToolId::try_new(tool.name())
            .map_err(|error| ToolCatalogError::InvalidToolId(error.value().to_string()))?;
        let mut definition = Tool::new(id.as_str());
        if let Some(description) = tool.description() {
            definition = definition.with_description(description);
        }
        if let Some(schema) = tool.input_schema() {
            definition = definition.with_schema(schema);
        }
        Ok((
            id,
            RegisteredToolContract {
                definition,
                output_schema: tool.output_schema(),
                kind,
            },
        ))
    }

    fn register_captured<T: StasisTool + 'static>(
        &mut self,
        tool: T,
        id: ToolId,
        contract: RegisteredToolContract,
    ) -> StasisResult<()> {
        if self.catalog.entries.contains_key(&id) {
            return Err(catalog_stasis_error(ToolCatalogError::DuplicateTool(id)));
        }
        self.registry.register_tool(tool)?;
        self.insert_catalog_entry(id, contract)
            .map_err(catalog_stasis_error)
    }
}

impl ToolRegistration for ToolRegistrar {
    fn register_tool<T: StasisTool + 'static>(&mut self, tool: T) -> StasisResult<()> {
        let (id, contract) = self
            .capture(&tool, RegisteredToolKind::Legacy)
            .map_err(catalog_stasis_error)?;
        self.register_captured(tool, id, contract)
    }

    fn register_typed_tool<T: StasisTool + TypedTool + 'static>(
        &mut self,
        tool: T,
    ) -> StasisResult<()> {
        let (id, mut contract) = self
            .capture(&tool, RegisteredToolKind::Typed)
            .map_err(catalog_stasis_error)?;
        let typed = T::contract();
        if typed.id != id {
            return Err(catalog_stasis_error(ToolCatalogError::ContractDrift {
                id,
                field: "id",
            }));
        }
        if contract.definition.description.as_deref() != Some(typed.description) {
            return Err(catalog_stasis_error(ToolCatalogError::ContractDrift {
                id,
                field: "description",
            }));
        }
        if contract.definition.schema.as_ref() != Some(&typed.input_schema) {
            return Err(catalog_stasis_error(ToolCatalogError::ContractDrift {
                id,
                field: "input schema",
            }));
        }
        if contract.output_schema.as_ref() != Some(&typed.output_schema) {
            return Err(catalog_stasis_error(ToolCatalogError::ContractDrift {
                id,
                field: "output schema",
            }));
        }
        contract.kind = RegisteredToolKind::Typed;
        self.register_captured(tool, id, contract)
    }
}

fn catalog_stasis_error(error: ToolCatalogError) -> StasisError {
    StasisError::PortFailure(format!("tool catalog registration failed: {error}"))
}

#[derive(Clone, Default)]
pub struct ToolCatalogHandle(Arc<OnceLock<Arc<ToolCatalog>>>);

impl ToolCatalogHandle {
    pub fn initialize(&self, catalog: Arc<ToolCatalog>) -> Result<(), ToolCatalogError> {
        self.0
            .set(catalog)
            .map_err(|_| ToolCatalogError::CatalogAlreadyInitialized)
    }

    pub fn get(&self) -> Option<Arc<ToolCatalog>> {
        self.0.get().cloned()
    }

    pub fn presentation_summary_for_wire(&self, wire_name: &str) -> String {
        self.get()
            .map(|catalog| catalog.presentation_summary_for_wire(wire_name))
            .unwrap_or_else(|| {
                "Session-unlocked tool — see cognition_tools_discover catalog".to_string()
            })
    }
}

const fn is_valid_tool_id(value: &[u8]) -> bool {
    if value.is_empty() || !value[0].is_ascii_alphabetic() {
        return false;
    }

    let mut index = 1;
    while index < value.len() {
        let byte = value[index];
        if !(byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-') {
            return false;
        }
        index += 1;
    }
    true
}
