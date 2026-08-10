use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use std::marker::PhantomData;

use genai::chat::Tool;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use super::{ToolCatalog, ToolCatalogEntry, ToolId, ToolModeId, normalize_input_schema};

/// General mode's intentionally empty model-authored call metadata.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct EmptyCallMetadata {}

/// Explicit mode-owned projection for a base field whose wire name is reused
/// by mode metadata. The base contract remains unchanged in the catalog.
#[derive(Debug, Clone, Default)]
pub struct ModeInputProjection {
    replaced_base_fields: BTreeSet<&'static str>,
}

impl ModeInputProjection {
    pub fn replacing(fields: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            replaced_base_fields: fields.into_iter().collect(),
        }
    }
}

/// Composes and parses one mode's flat, model-visible call metadata without
/// modifying the shared base tool contract.
#[derive(Debug, Clone)]
pub struct ModeToolAdapter<M> {
    mode: ToolModeId,
    metadata_schema: Value,
    metadata_fields: BTreeSet<String>,
    _metadata: PhantomData<fn() -> M>,
}

impl<M> ModeToolAdapter<M>
where
    M: DeserializeOwned + JsonSchema,
{
    pub fn new(mode: ToolModeId) -> Result<Self, ModeToolAdapterError> {
        let metadata_schema = normalize_input_schema::<M>()
            .map_err(|error| ModeToolAdapterError::InvalidMetadataSchema(error.to_string()))?;
        let metadata_fields = metadata_schema
            .get("properties")
            .and_then(Value::as_object)
            .expect("normalized input schema properties")
            .keys()
            .cloned()
            .collect();
        Ok(Self {
            mode,
            metadata_schema,
            metadata_fields,
            _metadata: PhantomData,
        })
    }

    pub const fn mode(&self) -> ToolModeId {
        self.mode
    }

    pub fn reserved_fields(&self) -> &BTreeSet<String> {
        &self.metadata_fields
    }

    pub fn compose_tool(&self, tool: Tool) -> Result<Tool, ModeToolAdapterError> {
        self.compose_tool_with_projection(tool, &ModeInputProjection::default())
    }

    pub fn compose_tool_with_projection(
        &self,
        mut tool: Tool,
        projection: &ModeInputProjection,
    ) -> Result<Tool, ModeToolAdapterError> {
        if self.metadata_fields.is_empty() {
            return Ok(tool);
        }

        let id = tool.name.as_str().to_string();
        let schema = tool.schema.get_or_insert_with(|| {
            json!({
                "type": "object",
                "properties": {},
                "required": []
            })
        });
        let object = schema
            .as_object_mut()
            .ok_or_else(|| ModeToolAdapterError::BaseInputMustBeObject { id: id.clone() })?;
        match object.get("type").and_then(Value::as_str) {
            Some("object") | None => {
                object
                    .entry("type")
                    .or_insert_with(|| Value::String("object".to_string()));
            }
            Some(_) => {
                return Err(ModeToolAdapterError::BaseInputMustBeObject { id });
            }
        }

        let properties = object
            .entry("properties")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| ModeToolAdapterError::InvalidBaseProperties { id: id.clone() })?;
        let metadata_properties = self
            .metadata_schema
            .get("properties")
            .and_then(Value::as_object)
            .expect("normalized metadata properties");
        for field in &projection.replaced_base_fields {
            if !self.metadata_fields.contains(*field) {
                return Err(ModeToolAdapterError::ProjectionFieldIsNotMetadata {
                    id,
                    field: (*field).to_string(),
                });
            }
            if properties.remove(*field).is_none() {
                return Err(ModeToolAdapterError::ProjectionFieldMissingFromBase {
                    id,
                    field: (*field).to_string(),
                });
            }
        }
        for (field, field_schema) in metadata_properties {
            if properties.contains_key(field) {
                return Err(ModeToolAdapterError::ReservedFieldCollision {
                    id,
                    field: field.clone(),
                });
            }
            properties.insert(field.clone(), field_schema.clone());
        }

        let required = object
            .entry("required")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| ModeToolAdapterError::InvalidBaseRequired { id: id.clone() })?;
        required.retain(|field| {
            field
                .as_str()
                .is_none_or(|field| !projection.replaced_base_fields.contains(field))
        });
        if let Some(metadata_required) = self
            .metadata_schema
            .get("required")
            .and_then(Value::as_array)
        {
            for field in metadata_required {
                if !required.contains(field) {
                    required.push(field.clone());
                }
            }
        }
        Ok(tool)
    }

    pub fn split_call(&self, input: Value) -> Result<(M, Value), ModeToolAdapterError> {
        let mut base = input
            .as_object()
            .cloned()
            .ok_or(ModeToolAdapterError::CallInputMustBeObject)?;
        let mut metadata = Map::new();
        for field in &self.metadata_fields {
            if let Some(value) = base.remove(field) {
                metadata.insert(field.clone(), value);
            }
        }
        let metadata = serde_json::from_value(Value::Object(metadata))
            .map_err(|error| ModeToolAdapterError::InvalidMetadata(error.to_string()))?;
        Ok((metadata, Value::Object(base)))
    }

    pub fn compile_surface(
        &self,
        catalog: &ToolCatalog,
        mut select: impl FnMut(&ToolCatalogEntry) -> bool,
    ) -> Result<Vec<Tool>, ModeToolAdapterError> {
        catalog
            .entries()
            .filter(|entry| select(entry))
            .map(|entry| self.compose_tool(entry.contract.definition.clone()))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModeToolAdapterError {
    InvalidMetadataSchema(String),
    BaseInputMustBeObject { id: String },
    InvalidBaseProperties { id: String },
    InvalidBaseRequired { id: String },
    ReservedFieldCollision { id: String, field: String },
    ProjectionFieldIsNotMetadata { id: String, field: String },
    ProjectionFieldMissingFromBase { id: String, field: String },
    CallInputMustBeObject,
    InvalidMetadata(String),
    UnknownTool(ToolId),
}

impl Display for ModeToolAdapterError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMetadataSchema(error) => {
                write!(formatter, "invalid mode metadata schema: {error}")
            }
            Self::BaseInputMustBeObject { id } => {
                write!(formatter, "base input schema for {id} must be an object")
            }
            Self::InvalidBaseProperties { id } => {
                write!(
                    formatter,
                    "base input schema properties for {id} must be an object"
                )
            }
            Self::InvalidBaseRequired { id } => {
                write!(
                    formatter,
                    "base input schema required fields for {id} must be an array"
                )
            }
            Self::ReservedFieldCollision { id, field } => write!(
                formatter,
                "base input schema for {id} collides with reserved mode field `{field}`"
            ),
            Self::ProjectionFieldIsNotMetadata { id, field } => write!(
                formatter,
                "mode projection for {id} replaces `{field}`, but the field is not mode metadata"
            ),
            Self::ProjectionFieldMissingFromBase { id, field } => write!(
                formatter,
                "mode projection for {id} replaces `{field}`, but the base contract has no such field"
            ),
            Self::CallInputMustBeObject => formatter.write_str("mode tool input must be an object"),
            Self::InvalidMetadata(error) => {
                write!(formatter, "invalid mode call metadata: {error}")
            }
            Self::UnknownTool(id) => write!(formatter, "tool is absent from mode catalog: {id}"),
        }
    }
}

impl std::error::Error for ModeToolAdapterError {}
