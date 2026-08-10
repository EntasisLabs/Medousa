use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

use schemars::JsonSchema;
use schemars::schema::Schema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::ToolId;

#[derive(Debug, Clone, PartialEq)]
pub struct ToolContract {
    pub id: ToolId,
    pub description: &'static str,
    pub input_schema: Value,
    pub output_schema: Value,
}

/// Static contract generated for a typed first-party tool.
pub trait TypedTool: Send + Sync + 'static {
    type Input: DeserializeOwned + JsonSchema + Send + 'static;
    type Output: Serialize + JsonSchema + Send + 'static;

    fn tool_id() -> ToolId;
    fn description() -> &'static str;
    fn contract() -> &'static ToolContract;
}

pub fn build_contract<T: TypedTool>() -> Result<ToolContract, ContractError> {
    let id = T::tool_id();
    let description = T::description();
    if description.trim().is_empty() {
        return Err(ContractError::EmptyDescription { id });
    }

    let input_schema = normalize_input_schema::<T::Input>()
        .map_err(|source| ContractError::InputSchema { id, source })?;
    let output_schema = normalize_output_schema::<T::Output>()
        .map_err(|source| ContractError::OutputSchema { id, source })?;

    Ok(ToolContract {
        id,
        description,
        input_schema,
        output_schema,
    })
}

pub fn normalize_input_schema<T: JsonSchema>() -> Result<Value, SchemaNormalizationError> {
    let mut schema = normalize_schema::<T>()?;
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        return Err(SchemaNormalizationError::InputMustBeObject);
    }
    schema
        .as_object_mut()
        .expect("object input schema")
        .entry("properties")
        .or_insert_with(|| Value::Object(Map::new()));
    sort_object_keys(&mut schema);
    Ok(schema)
}

pub fn normalize_output_schema<T: JsonSchema>() -> Result<Value, SchemaNormalizationError> {
    normalize_schema::<T>()
}

fn normalize_schema<T: JsonSchema>() -> Result<Value, SchemaNormalizationError> {
    let root = schemars::schema_for!(T);
    let mut schema = serde_json::to_value(root)
        .map_err(|error| SchemaNormalizationError::Serialization(error.to_string()))?;

    let mut definitions = BTreeMap::new();
    collect_definitions(&schema, "definitions", &mut definitions)?;
    collect_definitions(&schema, "$defs", &mut definitions)?;

    let Some(root_object) = schema.as_object_mut() else {
        return Err(SchemaNormalizationError::RootMustBeObject);
    };
    root_object.remove("$schema");
    root_object.remove("definitions");
    root_object.remove("$defs");
    root_object.remove("title");

    inline_references(&mut schema, &definitions, &mut Vec::new())?;
    collapse_single_all_of(&mut schema);
    normalize_single_examples(&mut schema);
    normalize_numeric_schema(&mut schema);
    sort_object_keys(&mut schema);
    Ok(schema)
}

fn normalize_single_examples(value: &mut Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                normalize_single_examples(value);
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                normalize_single_examples(value);
            }

            let single_example = match object.get("examples") {
                Some(Value::Array(examples)) if examples.len() == 1 => examples.first().cloned(),
                _ => None,
            };
            if let Some(example) = single_example {
                object.remove("examples");
                object.insert("example".to_string(), example);
            }
        }
        _ => {}
    }
}

fn normalize_numeric_schema(value: &mut Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                normalize_numeric_schema(value);
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                normalize_numeric_schema(value);
            }

            if matches!(
                object.get("type").and_then(Value::as_str),
                Some("integer" | "number")
            ) {
                object.remove("format");
            }
        }
        Value::Number(number) => {
            let Some(float) = number.as_f64() else {
                return;
            };
            if float.fract() == 0.0 && float >= i64::MIN as f64 && float <= i64::MAX as f64 {
                *number = serde_json::Number::from(float as i64);
            }
        }
        _ => {}
    }
}

fn collect_definitions(
    schema: &Value,
    key: &'static str,
    definitions: &mut BTreeMap<String, Value>,
) -> Result<(), SchemaNormalizationError> {
    let Some(value) = schema.get(key) else {
        return Ok(());
    };
    let Some(object) = value.as_object() else {
        return Err(SchemaNormalizationError::DefinitionsMustBeObject { key });
    };

    for (name, definition) in object {
        if definitions
            .insert(name.clone(), definition.clone())
            .is_some()
        {
            return Err(SchemaNormalizationError::DuplicateDefinition(name.clone()));
        }
    }
    Ok(())
}

fn inline_references(
    value: &mut Value,
    definitions: &BTreeMap<String, Value>,
    stack: &mut Vec<String>,
) -> Result<(), SchemaNormalizationError> {
    match value {
        Value::Array(values) => {
            for value in values {
                inline_references(value, definitions, stack)?;
            }
        }
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                let name = definition_name(reference)?;
                if stack.iter().any(|entry| entry == &name) {
                    let mut cycle = stack.clone();
                    cycle.push(name);
                    return Err(SchemaNormalizationError::RecursiveReference(cycle));
                }

                let mut replacement = definitions
                    .get(&name)
                    .cloned()
                    .ok_or_else(|| SchemaNormalizationError::MissingDefinition(name.clone()))?;
                stack.push(name);
                inline_references(&mut replacement, definitions, stack)?;
                stack.pop();

                let siblings = object
                    .iter()
                    .filter(|(key, _)| key.as_str() != "$ref")
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect::<Vec<_>>();
                if !siblings.is_empty() {
                    let Some(replacement_object) = replacement.as_object_mut() else {
                        return Err(SchemaNormalizationError::ReferenceSiblingsOnBooleanSchema);
                    };
                    for (key, mut sibling) in siblings {
                        inline_references(&mut sibling, definitions, stack)?;
                        replacement_object.insert(key, sibling);
                    }
                }
                *value = replacement;
                return Ok(());
            }

            for child in object.values_mut() {
                inline_references(child, definitions, stack)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn definition_name(reference: &str) -> Result<String, SchemaNormalizationError> {
    let encoded = reference
        .strip_prefix("#/definitions/")
        .or_else(|| reference.strip_prefix("#/$defs/"))
        .ok_or_else(|| SchemaNormalizationError::UnsupportedReference(reference.to_string()))?;

    if encoded.is_empty() || encoded.contains('/') {
        return Err(SchemaNormalizationError::UnsupportedReference(
            reference.to_string(),
        ));
    }

    Ok(encoded.replace("~1", "/").replace("~0", "~"))
}

fn collapse_single_all_of(value: &mut Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                collapse_single_all_of(value);
            }
        }
        Value::Object(object) => {
            for child in object.values_mut() {
                collapse_single_all_of(child);
            }

            let Some(Value::Array(all_of)) = object.get("allOf") else {
                return;
            };
            let [only_schema] = all_of.as_slice() else {
                return;
            };
            let only_schema = only_schema.clone();
            let mut siblings = std::mem::take(object);
            siblings.remove("allOf");

            match only_schema {
                Value::Object(mut inner) => {
                    inner.extend(siblings);
                    *value = Value::Object(inner);
                }
                Value::Bool(true) => {
                    *value = Value::Object(siblings);
                }
                Value::Bool(false) => {
                    *value = Value::Bool(false);
                }
                other => {
                    siblings.insert("allOf".to_string(), Value::Array(vec![other]));
                    *value = Value::Object(siblings);
                }
            }
        }
        _ => {}
    }
}

fn sort_object_keys(value: &mut Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                sort_object_keys(value);
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                sort_object_keys(value);
            }
            let mut entries = std::mem::take(object).into_iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            *object = entries.into_iter().collect::<Map<_, _>>();
        }
        _ => {}
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    EmptyDescription {
        id: ToolId,
    },
    InputSchema {
        id: ToolId,
        source: SchemaNormalizationError,
    },
    OutputSchema {
        id: ToolId,
        source: SchemaNormalizationError,
    },
}

impl Display for ContractError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyDescription { id } => {
                write!(formatter, "typed tool `{id}` has an empty description")
            }
            Self::InputSchema { id, source } => {
                write!(
                    formatter,
                    "invalid input schema for typed tool `{id}`: {source}"
                )
            }
            Self::OutputSchema { id, source } => {
                write!(
                    formatter,
                    "invalid output schema for typed tool `{id}`: {source}"
                )
            }
        }
    }
}

impl std::error::Error for ContractError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaNormalizationError {
    Serialization(String),
    RootMustBeObject,
    InputMustBeObject,
    DefinitionsMustBeObject { key: &'static str },
    DuplicateDefinition(String),
    UnsupportedReference(String),
    MissingDefinition(String),
    RecursiveReference(Vec<String>),
    ReferenceSiblingsOnBooleanSchema,
}

impl Display for SchemaNormalizationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serialization(error) => write!(formatter, "could not serialize schema: {error}"),
            Self::RootMustBeObject => formatter.write_str("root schema must be a JSON object"),
            Self::InputMustBeObject => {
                formatter.write_str("tool input schema must have root type `object`")
            }
            Self::DefinitionsMustBeObject { key } => {
                write!(formatter, "schema `{key}` must be an object")
            }
            Self::DuplicateDefinition(name) => {
                write!(formatter, "schema definition `{name}` is declared twice")
            }
            Self::UnsupportedReference(reference) => {
                write!(formatter, "unsupported schema reference `{reference}`")
            }
            Self::MissingDefinition(name) => {
                write!(
                    formatter,
                    "schema reference targets missing definition `{name}`"
                )
            }
            Self::RecursiveReference(cycle) => {
                write!(
                    formatter,
                    "recursive schema reference: {}",
                    cycle.join(" -> ")
                )
            }
            Self::ReferenceSiblingsOnBooleanSchema => formatter
                .write_str("schema reference with sibling fields cannot target a boolean schema"),
        }
    }
}

impl std::error::Error for SchemaNormalizationError {}

/// Explicitly opaque JSON owned by an external system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExternalJson(Value);

impl ExternalJson {
    pub fn new(value: Value) -> Self {
        Self(value)
    }

    pub fn as_value(&self) -> &Value {
        &self.0
    }

    pub fn into_value(self) -> Value {
        self.0
    }
}

impl JsonSchema for ExternalJson {
    fn schema_name() -> String {
        "ExternalJson".to_string()
    }

    fn is_referenceable() -> bool {
        false
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        Schema::Bool(true)
    }
}

/// Explicitly opaque tool payload whose shape is supplied at runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OpaqueToolPayload(Value);

impl OpaqueToolPayload {
    pub fn new(value: Value) -> Self {
        Self(value)
    }

    pub fn as_value(&self) -> &Value {
        &self.0
    }

    pub fn into_value(self) -> Value {
        self.0
    }
}

impl JsonSchema for OpaqueToolPayload {
    fn schema_name() -> String {
        "OpaqueToolPayload".to_string()
    }

    fn is_referenceable() -> bool {
        false
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        Schema::Bool(true)
    }
}
