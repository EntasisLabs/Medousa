//! Typed action schemas: one public lookup, batched type names.

use schemars::JsonSchema;
use schemars::schema::{InstanceType, ObjectValidation, Schema, SchemaObject};
use serde::Deserialize;
use serde_json::{Value, json};
use stasis::prelude::StasisError;

use crate::capability_tools::capability_type_schemas;
use crate::memory_api::memory_type_schemas;
use crate::public_api::COGNITION_SCHEMA;
use crate::runtime_api::runtime_type_schemas;
use crate::store_tools::store_type_schemas;
use crate::turn_api::turn_type_schemas;
use crate::typed_tools::{ExternalJson, ToolId, medousa_tool};

const SCHEMA_ID: ToolId = ToolId::new(COGNITION_SCHEMA);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SchemaDomain {
    Runtime,
    Store,
    Capability,
    Turn,
    Memory,
}

impl SchemaDomain {
    fn as_str(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Store => "store",
            Self::Capability => "capability",
            Self::Turn => "turn",
            Self::Memory => "memory",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SchemaInput {
    #[serde(default)]
    domain: Option<SchemaDomain>,
    #[serde(default)]
    types: Vec<String>,
}

pub struct CognitionSchemaTool;

pub fn register_schema_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionSchemaTool)?;
    Ok(())
}

#[medousa_tool(id = SCHEMA_ID)]
impl CognitionSchemaTool {
    /// Fetch typed action parameter schemas. `types` is a list — batch several actions in one call (read then execute). Omit `types` and set `domain` to list action names.
    async fn invoke_typed(&self, input: SchemaInput) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch(input)?))
    }
}

impl JsonSchema for SchemaInput {
    fn schema_name() -> String {
        "SchemaInput".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[
            (
                "domain",
                string_enum_schema(&["runtime", "store", "capability", "turn", "memory"]),
                false,
            ),
            ("types", type_name_array_schema(), false),
        ])
    }
}

fn dispatch(input: SchemaInput) -> stasis::prelude::Result<Value> {
    let catalog = catalog();
    let requested: Vec<String> = input
        .types
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    if requested.is_empty() {
        let domain = input.domain.ok_or_else(|| {
            StasisError::PortFailure(
                "cognition_schema: set domain to list types, or pass types=[...] to fetch schemas"
                    .to_string(),
            )
        })?;
        return Ok(json!({
            "domain": domain.as_str(),
            "types": catalog
                .iter()
                .filter(|action| action.domain == domain)
                .map(catalog_entry)
                .collect::<Vec<_>>(),
        }));
    }

    let mut types = Vec::new();
    for name in &requested {
        let action = catalog
            .iter()
            .find(|action| action.name == name)
            .ok_or_else(|| {
                StasisError::PortFailure(format!(
                    "cognition_schema: unknown type '{name}'. valid types: {}",
                    catalog
                        .iter()
                        .map(|action| action.name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })?;
        if let Some(domain) = input.domain
            && action.domain != domain
        {
            return Err(StasisError::PortFailure(format!(
                "cognition_schema: type '{name}' is domain={}, not {}",
                action.domain.as_str(),
                domain.as_str()
            )));
        }
        types.push(full_entry(action));
    }
    Ok(json!({
        "domain": input.domain.map(SchemaDomain::as_str),
        "types": types,
    }))
}

fn catalog_entry(action: &CatalogItem) -> Value {
    json!({
        "name": action.name,
        "tool": action.tool,
        "summary": action.summary,
        "call": action.call.clone(),
    })
}

fn full_entry(action: &CatalogItem) -> Value {
    json!({
        "name": action.name,
        "tool": action.tool,
        "summary": action.summary,
        "call": action.call.clone(),
        "parameters": action.parameters.clone(),
    })
}

struct CatalogItem {
    name: &'static str,
    domain: SchemaDomain,
    tool: &'static str,
    summary: &'static str,
    call: Value,
    parameters: Value,
}

fn catalog() -> Vec<CatalogItem> {
    generated_items(SchemaDomain::Runtime, runtime_type_schemas())
        .chain(generated_items(SchemaDomain::Store, store_type_schemas()))
        .chain(generated_items(
            SchemaDomain::Capability,
            capability_type_schemas(),
        ))
        .chain(generated_items(SchemaDomain::Turn, turn_type_schemas()))
        .chain(generated_items(SchemaDomain::Memory, memory_type_schemas()))
        .collect()
}

fn generated_items(
    domain: SchemaDomain,
    items: Vec<TypedActionSchema>,
) -> impl Iterator<Item = CatalogItem> {
    items.into_iter().map(move |item| CatalogItem {
        name: item.name,
        domain,
        tool: item.tool.as_str(),
        summary: item.summary,
        call: json!({ "action": item.name }),
        parameters: item.parameters,
    })
}

pub struct TypedActionSchema {
    pub name: &'static str,
    pub tool: ToolId,
    pub summary: &'static str,
    pub parameters: Value,
}

pub fn typed_action_schema<T: JsonSchema>(
    tool: ToolId,
    name: &'static str,
    summary: &'static str,
) -> TypedActionSchema {
    TypedActionSchema {
        name,
        tool,
        summary,
        parameters: with_action_const(schema_object::<T>(), name),
    }
}

fn schema_object<T: JsonSchema>() -> Value {
    let mut root = serde_json::to_value(schemars::schema_for!(T)).expect("action schema");
    if root.get("properties").is_some() {
        return root;
    }
    let Some(reference) = root.get("$ref").and_then(Value::as_str).map(str::to_string) else {
        return root;
    };
    let key = reference.trim_start_matches("#/definitions/").to_string();
    let Some(mut definition) = root
        .get("definitions")
        .and_then(Value::as_object)
        .and_then(|definitions| definitions.get(&key))
        .cloned()
    else {
        return root;
    };
    if let Some(Value::Object(definitions)) = root.get_mut("definitions") {
        definitions.remove(&key);
        if !definitions.is_empty()
            && let Some(object) = definition.as_object_mut()
        {
            object.insert(
                "definitions".to_string(),
                Value::Object(definitions.clone()),
            );
        }
    }
    definition
}

fn with_action_const(mut schema: Value, action: &'static str) -> Value {
    let properties = schema
        .as_object_mut()
        .map(|object| object.entry("properties").or_insert_with(|| json!({})));
    if let Some(Value::Object(properties)) = properties {
        properties.insert(
            "action".to_string(),
            json!({
                "type": "string",
                "const": action,
                "description": format!("Pass {action}")
            }),
        );
    }
    match schema.get_mut("required") {
        Some(Value::Array(required)) => {
            if !required.iter().any(|value| value == "action") {
                required.insert(0, json!("action"));
            }
        }
        _ => {
            schema
                .as_object_mut()
                .expect("object schema")
                .insert("required".to_string(), json!(["action"]));
        }
    }
    schema
}

pub fn advertised_object_schema(fields: &[(&str, Schema, bool)]) -> Schema {
    let mut properties = schemars::Map::new();
    let mut required = std::collections::BTreeSet::new();
    for (name, schema, is_required) in fields {
        properties.insert((*name).to_string(), schema.clone());
        if *is_required {
            required.insert((*name).to_string());
        }
    }
    Schema::Object(SchemaObject {
        instance_type: Some(InstanceType::Object.into()),
        object: Some(Box::new(ObjectValidation {
            properties,
            required,
            additional_properties: Some(Box::new(Schema::Bool(true))),
            ..ObjectValidation::default()
        })),
        ..SchemaObject::default()
    })
}

pub fn string_enum_schema(values: &[&str]) -> Schema {
    Schema::Object(SchemaObject {
        instance_type: Some(InstanceType::String.into()),
        enum_values: Some(values.iter().map(|value| json!(value)).collect()),
        ..SchemaObject::default()
    })
}

fn type_name_array_schema() -> Schema {
    let names: Vec<Value> = catalog().iter().map(|action| json!(action.name)).collect();
    Schema::Object(SchemaObject {
        instance_type: Some(InstanceType::Array.into()),
        array: Some(Box::new(schemars::schema::ArrayValidation {
            items: Some(schemars::schema::SingleOrVec::Single(Box::new(
                Schema::Object(SchemaObject {
                    instance_type: Some(InstanceType::String.into()),
                    enum_values: Some(names),
                    ..SchemaObject::default()
                }),
            ))),
            ..schemars::schema::ArrayValidation::default()
        })),
        ..SchemaObject::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public_api::{COGNITION_RUNTIME_MUTATE, COGNITION_STORE_READ};

    #[test]
    fn lists_runtime_types_without_parameter_bodies() {
        let listed = dispatch(SchemaInput {
            domain: Some(SchemaDomain::Runtime),
            types: Vec::new(),
        })
        .expect("list");
        let types = listed["types"].as_array().expect("types");
        assert!(types.iter().any(|entry| entry["name"] == "job.enqueue"));
        assert!(types.iter().any(|entry| entry["name"] == "workflow.plan"));
        assert!(types[0].get("parameters").is_none());
    }

    #[test]
    fn batches_read_and_execute_schemas() {
        let fetched = dispatch(SchemaInput {
            domain: None,
            types: vec!["vault.read".to_string(), "job.enqueue".to_string()],
        })
        .expect("batch");
        let types = fetched["types"].as_array().expect("types");
        assert_eq!(types.len(), 2);
        assert_eq!(types[0]["tool"], COGNITION_STORE_READ);
        let required = types[0]["parameters"]["required"]
            .as_array()
            .expect("vault.read required");
        assert!(required.iter().any(|value| value == "action"));
        assert!(required.iter().any(|value| value == "path"));
        assert!(types[0]["parameters"]["properties"]["path"].is_object());
        assert_eq!(types[1]["tool"], COGNITION_RUNTIME_MUTATE);
        assert!(types[1]["parameters"]["properties"]["script"].is_object());
    }

    #[test]
    fn rejects_unknown_type() {
        let error = dispatch(SchemaInput {
            domain: Some(SchemaDomain::Runtime),
            types: vec!["nope".to_string()],
        })
        .expect_err("unknown");
        assert!(error.to_string().contains("unknown type"));
    }

    #[test]
    fn advertised_schema_lists_type_names_for_batching() {
        let schema = serde_json::to_value(schemars::schema_for!(SchemaInput)).expect("schema");
        let names = schema["properties"]["types"]["items"]["enum"]
            .as_array()
            .expect("type name enum");
        assert!(names.iter().any(|value| value == "vault.read"));
        assert!(names.iter().any(|value| value == "job.enqueue"));
        assert!(names.iter().any(|value| value == "grapheme.invoke"));
        assert!(names.iter().any(|value| value == "turn.finish"));
        assert!(names.iter().any(|value| value == "memory.store"));
    }
}
