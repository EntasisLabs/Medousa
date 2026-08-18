//! Typed action schemas: one public lookup, batched type names.

use schemars::JsonSchema;
use schemars::schema::{InstanceType, ObjectValidation, Schema, SchemaObject};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use stasis::prelude::StasisError;

use crate::public_api::{
    COGNITION_CAPABILITY, COGNITION_SCHEMA, COGNITION_STORE_READ, COGNITION_STORE_WRITE,
};
use crate::runtime_api::runtime_type_schemas;
use crate::typed_tools::{ExternalJson, ToolId, medousa_tool};

const SCHEMA_ID: ToolId = ToolId::new(COGNITION_SCHEMA);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SchemaDomain {
    Runtime,
    Store,
    Capability,
}

impl SchemaDomain {
    fn as_str(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Store => "store",
            Self::Capability => "capability",
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
                string_enum_schema(&["runtime", "store", "capability"]),
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
        let action = catalog.iter().find(|action| action.name == name).ok_or_else(|| {
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

fn discriminator_object(pairs: &[(&'static str, &'static str)]) -> Value {
    let mut map = Map::new();
    for (key, value) in pairs {
        map.insert((*key).to_string(), Value::String((*value).to_string()));
    }
    Value::Object(map)
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
    let mut items: Vec<CatalogItem> = runtime_type_schemas()
        .into_iter()
        .map(|item| CatalogItem {
            name: item.name,
            domain: SchemaDomain::Runtime,
            tool: item.tool.as_str(),
            summary: item.summary,
            call: json!({ "action": item.name }),
            parameters: item.parameters,
        })
        .collect();
    items.extend(STORE.iter().map(static_item));
    items.extend(CAPABILITY.iter().map(static_item));
    items
}

fn static_item(action: &ActionType) -> CatalogItem {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for (key, value) in action.discriminators {
        properties.insert(
            (*key).to_string(),
            json!({
                "type": "string",
                "const": value,
                "description": format!("Pass {value}")
            }),
        );
        required.push((*key).to_string());
    }
    for field in action.fields {
        let mut spec = json!({
            "type": field.kind,
            "description": field.description,
        });
        if !field.enum_values.is_empty() {
            spec["enum"] = json!(field.enum_values);
        }
        properties.insert(field.name.to_string(), spec);
        if field.required {
            required.push(field.name.to_string());
        }
    }
    CatalogItem {
        name: action.name,
        domain: action.domain,
        tool: action.tool,
        summary: action.summary,
        call: discriminator_object(action.discriminators),
        parameters: json!({
            "type": "object",
            "required": required,
            "properties": properties,
        }),
    }
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

struct Field {
    name: &'static str,
    kind: &'static str,
    required: bool,
    description: &'static str,
    enum_values: &'static [&'static str],
}

impl Field {
    const fn req(name: &'static str, kind: &'static str, description: &'static str) -> Self {
        Self {
            name,
            kind,
            required: true,
            description,
            enum_values: &[],
        }
    }

    const fn opt(name: &'static str, kind: &'static str, description: &'static str) -> Self {
        Self {
            name,
            kind,
            required: false,
            description,
            enum_values: &[],
        }
    }

    const fn opt_enum(
        name: &'static str,
        description: &'static str,
        enum_values: &'static [&'static str],
    ) -> Self {
        Self {
            name,
            kind: "string",
            required: false,
            description,
            enum_values,
        }
    }
}

struct ActionType {
    name: &'static str,
    domain: SchemaDomain,
    tool: &'static str,
    summary: &'static str,
    discriminators: &'static [(&'static str, &'static str)],
    fields: &'static [Field],
}

const STORE: &[ActionType] = &[
    ActionType {
        name: "vault.list",
        domain: SchemaDomain::Store,
        tool: COGNITION_STORE_READ,
        summary: "List vault notes",
        discriminators: &[("store", "vault"), ("op", "list")],
        fields: &[
            Field::opt("prefix", "string", "Path prefix"),
            Field::opt("semantic_tags", "array", "Tag filter"),
            Field::opt("tag_prefix", "string", "Tag prefix; facet=tags lists tags"),
            Field::opt("facet", "string", "Set tags to list tag names"),
            Field::opt("limit", "integer", "Max rows"),
        ],
    },
    ActionType {
        name: "vault.read",
        domain: SchemaDomain::Store,
        tool: COGNITION_STORE_READ,
        summary: "Read a vault note",
        discriminators: &[("store", "vault"), ("op", "read")],
        fields: &[
            Field::req("path", "string", "Note path"),
            Field::opt("max_chars", "integer", "Truncate body"),
            Field::opt("line_start", "integer", "1-based start line"),
            Field::opt("line_end", "integer", "1-based end line"),
        ],
    },
    ActionType {
        name: "vault.search",
        domain: SchemaDomain::Store,
        tool: COGNITION_STORE_READ,
        summary: "Search vault notes; path set = in-file grep",
        discriminators: &[("store", "vault"), ("op", "search")],
        fields: &[
            Field::req("query", "string", "Search text or grep pattern"),
            Field::opt("path", "string", "If set, grep this file"),
            Field::opt("semantic_tags", "array", "Tag filter for corpus search"),
            Field::opt("context_lines", "integer", "Grep context"),
            Field::opt("limit", "integer", "Max hits"),
        ],
    },
    ActionType {
        name: "vault.write",
        domain: SchemaDomain::Store,
        tool: COGNITION_STORE_WRITE,
        summary: "Write a vault note",
        discriminators: &[("store", "vault"), ("op", "write")],
        fields: &[
            Field::req("path", "string", "Note path"),
            Field::req("content", "string", "Markdown body"),
            Field::opt("semantic_tags", "array", "Tags"),
            Field::opt("if_match", "string", "Optimistic concurrency token"),
        ],
    },
    ActionType {
        name: "vault.delete",
        domain: SchemaDomain::Store,
        tool: COGNITION_STORE_WRITE,
        summary: "Delete a vault note",
        discriminators: &[("store", "vault"), ("op", "delete")],
        fields: &[Field::req("path", "string", "Note path")],
    },
    ActionType {
        name: "vault.move",
        domain: SchemaDomain::Store,
        tool: COGNITION_STORE_WRITE,
        summary: "Move a vault note",
        discriminators: &[("store", "vault"), ("op", "move")],
        fields: &[
            Field::req("path", "string", "From path"),
            Field::req("to_path", "string", "To path"),
        ],
    },
    ActionType {
        name: "artifacts.list",
        domain: SchemaDomain::Store,
        tool: COGNITION_STORE_READ,
        summary: "List HTML artifacts",
        discriminators: &[("store", "artifacts"), ("op", "list")],
        fields: &[
            Field::opt("query", "string", "Title/id substring"),
            Field::opt("limit", "integer", "Max rows"),
        ],
    },
    ActionType {
        name: "artifacts.read",
        domain: SchemaDomain::Store,
        tool: COGNITION_STORE_READ,
        summary: "Read an HTML artifact",
        discriminators: &[("store", "artifacts"), ("op", "read")],
        fields: &[
            Field::req("path", "string", "Artifact id"),
            Field::opt("max_chars", "integer", "Truncate body"),
            Field::opt("line_start", "integer", "1-based start line"),
            Field::opt("line_end", "integer", "1-based end line"),
        ],
    },
    ActionType {
        name: "artifacts.search",
        domain: SchemaDomain::Store,
        tool: COGNITION_STORE_READ,
        summary: "Grep one HTML artifact",
        discriminators: &[("store", "artifacts"), ("op", "search")],
        fields: &[
            Field::req("path", "string", "Artifact id"),
            Field::req("query", "string", "Grep pattern"),
            Field::opt("context_lines", "integer", "Grep context"),
            Field::opt("limit", "integer", "Max hits"),
        ],
    },
    ActionType {
        name: "artifacts.write",
        domain: SchemaDomain::Store,
        tool: COGNITION_STORE_WRITE,
        summary: "Create or revise an HTML artifact",
        discriminators: &[("store", "artifacts"), ("op", "write")],
        fields: &[
            Field::req("content", "string", "HTML body"),
            Field::opt("title", "string", "Artifact title"),
            Field::opt("path", "string", "Existing artifact id to revise"),
            Field::opt("presentation", "string", "inline, panel, or fullscreen"),
            Field::opt("height", "integer", "Preferred height"),
        ],
    },
    ActionType {
        name: "artifacts.delete",
        domain: SchemaDomain::Store,
        tool: COGNITION_STORE_WRITE,
        summary: "Delete an HTML artifact",
        discriminators: &[("store", "artifacts"), ("op", "delete")],
        fields: &[Field::req("path", "string", "Artifact id")],
    },
    ActionType {
        name: "code.read",
        domain: SchemaDomain::Store,
        tool: COGNITION_STORE_READ,
        summary: "Read a file in the bound worktree",
        discriminators: &[("store", "code"), ("op", "read")],
        fields: &[
            Field::req("path", "string", "Path relative to root"),
            Field::opt("root", "string", "Worktree root; Coder binds this"),
            Field::opt("line_start", "integer", "1-based start line"),
            Field::opt("line_end", "integer", "1-based end line"),
        ],
    },
    ActionType {
        name: "code.search",
        domain: SchemaDomain::Store,
        tool: COGNITION_STORE_READ,
        summary: "Search the bound worktree",
        discriminators: &[("store", "code"), ("op", "search")],
        fields: &[
            Field::req("query", "string", "Search text"),
            Field::opt("root", "string", "Worktree root; Coder binds this"),
            Field::opt("max_results", "integer", "Hit cap"),
        ],
    },
    ActionType {
        name: "code.write",
        domain: SchemaDomain::Store,
        tool: COGNITION_STORE_WRITE,
        summary: "Write or patch a file in the bound worktree",
        discriminators: &[("store", "code"), ("op", "write")],
        fields: &[
            Field::req("path", "string", "Path relative to root"),
            Field::req(
                "expected_sha256",
                "string",
                "Hash from read, or missing for a new file",
            ),
            Field::opt("content", "string", "Full file contents"),
            Field::opt("find", "string", "Patch find text"),
            Field::opt("replace", "string", "Patch replace text"),
            Field::opt("root", "string", "Worktree root; Coder binds this"),
        ],
    },
    ActionType {
        name: "scripts.list",
        domain: SchemaDomain::Store,
        tool: COGNITION_STORE_READ,
        summary: "List saved Grapheme scripts",
        discriminators: &[("store", "scripts"), ("op", "list")],
        fields: &[
            Field::opt("module", "string", "Module filter"),
            Field::opt("tag", "string", "Tag filter"),
            Field::opt("limit", "integer", "Max rows"),
        ],
    },
    ActionType {
        name: "scripts.read",
        domain: SchemaDomain::Store,
        tool: COGNITION_STORE_READ,
        summary: "Load a saved Grapheme script",
        discriminators: &[("store", "scripts"), ("op", "read")],
        fields: &[Field::req("path", "string", "Script id")],
    },
    ActionType {
        name: "scripts.search",
        domain: SchemaDomain::Store,
        tool: COGNITION_STORE_READ,
        summary: "Search saved Grapheme scripts",
        discriminators: &[("store", "scripts"), ("op", "search")],
        fields: &[
            Field::opt("query", "string", "Search text"),
            Field::opt("module", "string", "Module filter"),
            Field::opt("tag", "string", "Tag filter"),
            Field::opt("limit", "integer", "Max rows"),
        ],
    },
    ActionType {
        name: "scripts.write",
        domain: SchemaDomain::Store,
        tool: COGNITION_STORE_WRITE,
        summary: "Save a Grapheme script",
        discriminators: &[("store", "scripts"), ("op", "write")],
        fields: &[
            Field::req("content", "string", "Grapheme source"),
            Field::opt("path", "string", "Script id"),
            Field::opt("name", "string", "Display name"),
            Field::opt("modules", "array", "Module names"),
            Field::opt("tags", "array", "Tags"),
            Field::opt("script_intent", "string", "Why this script exists"),
        ],
    },
];

const CAPABILITY: &[ActionType] = &[
    ActionType {
        name: "capability.find",
        domain: SchemaDomain::Capability,
        tool: COGNITION_CAPABILITY,
        summary: "Search or resolve the capability catalog",
        discriminators: &[("op", "find"), ("source", "auto")],
        fields: &[
            Field::opt("capability", "string", "Resolve this catalog id"),
            Field::opt("query", "string", "Search text if capability is omitted"),
            Field::opt("limit", "integer", "Search hit cap"),
        ],
    },
    ActionType {
        name: "mcp.find",
        domain: SchemaDomain::Capability,
        tool: COGNITION_CAPABILITY,
        summary: "List MCP servers or discover tools",
        discriminators: &[("op", "find"), ("source", "mcp")],
        fields: &[
            Field::opt("query", "string", "Tool search; omit to list servers"),
            Field::opt("server_id", "string", "Limit discover to this server"),
            Field::opt("limit", "integer", "Hit cap"),
        ],
    },
    ActionType {
        name: "grapheme.find",
        domain: SchemaDomain::Capability,
        tool: COGNITION_CAPABILITY,
        summary: "Grapheme modules, ops, or examples",
        discriminators: &[("op", "find"), ("source", "grapheme")],
        fields: &[
            Field::opt(
                "module",
                "string",
                "Module id — returns info and ops when detail=full",
            ),
            Field::opt("name", "string", "Example name"),
            Field::opt("query", "string", "Search modules"),
            Field::opt_enum(
                "detail",
                "full (default) includes ops; summary is metadata only",
                &["summary", "full"],
            ),
        ],
    },
    ActionType {
        name: "capability.invoke",
        domain: SchemaDomain::Capability,
        tool: COGNITION_CAPABILITY,
        summary: "Run a catalog capability (auto-picks Grapheme or MCP binding)",
        discriminators: &[("op", "invoke"), ("source", "auto")],
        fields: &[
            Field::opt("capability", "string", "Catalog id"),
            Field::opt("query", "string", "Resolve by search if capability omitted"),
            Field::opt(
                "script",
                "string",
                "Inline Grapheme if the binding is a script",
            ),
            Field::opt("params", "object", "Template params"),
            Field::opt("input", "object", "MCP arguments when the binding is MCP"),
        ],
    },
    ActionType {
        name: "mcp.invoke",
        domain: SchemaDomain::Capability,
        tool: COGNITION_CAPABILITY,
        summary: "Invoke one MCP tool",
        discriminators: &[("op", "invoke"), ("source", "mcp")],
        fields: &[
            Field::req("server_id", "string", "MCP server id"),
            Field::req("tool_name", "string", "MCP tool name"),
            Field::opt("input", "object", "Tool arguments"),
            Field::opt_enum(
                "effect_class",
                "external_read is parallel-safe",
                &["external_read", "external_side_effect"],
            ),
        ],
    },
    ActionType {
        name: "grapheme.invoke",
        domain: SchemaDomain::Capability,
        tool: COGNITION_CAPABILITY,
        summary: "Run a Grapheme template or inline script",
        discriminators: &[("op", "invoke"), ("source", "grapheme")],
        fields: &[
            Field::opt(
                "template",
                "string",
                "Named template (e.g. http_poll, web_research)",
            ),
            Field::opt("params", "object", "Template params"),
            Field::opt("script", "string", "Inline Grapheme source if no template"),
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public_api::COGNITION_RUNTIME_MUTATE;

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
        assert_eq!(
            types[0]["parameters"]["required"],
            json!(["store", "op", "path"])
        );
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
    }
}
