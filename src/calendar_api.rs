//! Public calendar primitives: one query tool and one mutate tool.
//!
//! The model-facing entry is a tagged action enum. Parameter schemas live on
//! each variant type — `cognition_schema` reads those types, not a parallel catalog.

use schemars::JsonSchema;
use schemars::schema::Schema;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::calendar_tools::{
    CalendarAlarmInput, CalendarCreateInput, CalendarDeleteInput, CalendarExportInput,
    CalendarImportInput, CalendarListInput, CalendarUpdateInput, CalendarWriteFieldsInput,
    CognitionCalendarCreateTool, CognitionCalendarDeleteTool, CognitionCalendarExportTool,
    CognitionCalendarImportTool, CognitionCalendarListTool, CognitionCalendarUpdateTool,
};
use crate::events::TuiEvent;
use crate::public_api::{COGNITION_CALENDAR_MUTATE, COGNITION_CALENDAR_QUERY};
use crate::schema_api::{
    TypedActionSchema, advertised_object_schema, string_enum_schema, typed_action_schema,
};
use crate::typed_tools::{
    CompatOption, ExternalJson, ToolId, TypedTool, medousa_tool, serialize_output,
};

const CALENDAR_QUERY_ID: ToolId = ToolId::new(COGNITION_CALENDAR_QUERY);
const CALENDAR_MUTATE_ID: ToolId = ToolId::new(COGNITION_CALENDAR_MUTATE);

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum CalendarQueryAction {
    #[serde(rename = "calendar.list")]
    List(CalendarList),
    #[serde(rename = "calendar.export")]
    Export(CalendarExport),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action")]
pub enum CalendarMutateAction {
    #[serde(rename = "calendar.create")]
    Create(CalendarCreate),
    #[serde(rename = "calendar.update")]
    Update(CalendarUpdate),
    #[serde(rename = "calendar.delete")]
    Delete(CalendarDelete),
    #[serde(rename = "calendar.import")]
    Import(CalendarImport),
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct CalendarList {
    /// RFC3339 range start (inclusive)
    #[serde(default)]
    from: Option<String>,
    /// RFC3339 range end (exclusive)
    #[serde(default)]
    to: Option<String>,
    /// Vault-relative .ics path (default calendar/personal.ics)
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct CalendarExport {
    /// Vault-relative .ics path (default calendar/personal.ics)
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CalendarAlarm {
    /// Minutes before dtstart
    trigger_minutes_before: i64,
    /// VALARM action (display)
    #[serde(default)]
    action: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CalendarCreate {
    /// Event title
    summary: String,
    /// RFC3339 start. All-day: YYYY-MM-DDT00:00:00Z for that calendar date.
    dtstart: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    location: Option<String>,
    /// RFC3339 end. All-day: exclusive next-day YYYY-MM-DDT00:00:00Z.
    #[serde(default)]
    dtend: Option<String>,
    /// True for DATE (calendar-day) events; use UTC midnights for dtstart/dtend.
    #[serde(default)]
    all_day: Option<bool>,
    /// Optional RRULE body (without RRULE: prefix)
    #[serde(default)]
    rrule: Option<String>,
    /// Optional vault-relative markdown note linked to this event
    #[serde(default)]
    note_path: Option<String>,
    #[serde(default)]
    alarms: Option<Vec<CalendarAlarm>>,
    /// Vault-relative .ics path (default calendar/personal.ics)
    #[serde(default)]
    path: Option<String>,
    /// Alias for path
    #[serde(default)]
    calendar_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CalendarUpdate {
    /// Event UID to update
    uid: String,
    /// Event title
    summary: String,
    /// RFC3339 start. All-day: YYYY-MM-DDT00:00:00Z for that calendar date.
    dtstart: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    dtend: Option<String>,
    #[serde(default)]
    all_day: Option<bool>,
    #[serde(default)]
    rrule: Option<String>,
    #[serde(default)]
    note_path: Option<String>,
    #[serde(default)]
    alarms: Option<Vec<CalendarAlarm>>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    calendar_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CalendarDelete {
    uid: String,
    /// Vault-relative .ics path (default calendar/personal.ics)
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CalendarImport {
    /// Raw RFC 5545 text
    ics: String,
    /// Vault-relative .ics path (default calendar/personal.ics)
    #[serde(default)]
    path: Option<String>,
    /// Alias for path
    #[serde(default)]
    calendar_path: Option<String>,
}

impl JsonSchema for CalendarQueryAction {
    fn schema_name() -> String {
        "CalendarQueryAction".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[(
            "action",
            string_enum_schema(&["calendar.list", "calendar.export"]),
            true,
        )])
    }
}

impl JsonSchema for CalendarMutateAction {
    fn schema_name() -> String {
        "CalendarMutateAction".to_string()
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        advertised_object_schema(&[(
            "action",
            string_enum_schema(&[
                "calendar.create",
                "calendar.update",
                "calendar.delete",
                "calendar.import",
            ]),
            true,
        )])
    }
}

pub fn calendar_type_schemas() -> Vec<TypedActionSchema> {
    vec![
        typed_action_schema::<CalendarList>(
            CALENDAR_QUERY_ID,
            "calendar.list",
            "List personal calendar events in a time range (RRULE expanded)",
        ),
        typed_action_schema::<CalendarExport>(
            CALENDAR_QUERY_ID,
            "calendar.export",
            "Export the vault calendar as raw ICS text",
        ),
        typed_action_schema::<CalendarCreate>(
            CALENDAR_MUTATE_ID,
            "calendar.create",
            "Create a calendar event in the vault .ics store",
        ),
        typed_action_schema::<CalendarUpdate>(
            CALENDAR_MUTATE_ID,
            "calendar.update",
            "Update an existing calendar event by uid",
        ),
        typed_action_schema::<CalendarDelete>(
            CALENDAR_MUTATE_ID,
            "calendar.delete",
            "Delete a calendar event by uid",
        ),
        typed_action_schema::<CalendarImport>(
            CALENDAR_MUTATE_ID,
            "calendar.import",
            "Merge VEVENT components from raw ICS text (UID upsert)",
        ),
    ]
}

pub struct CognitionCalendarQueryTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

pub struct CognitionCalendarMutateTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

pub fn register_calendar_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    event_tx: mpsc::Sender<TuiEvent>,
) -> stasis::prelude::Result<()> {
    registry.register_typed_tool(CognitionCalendarQueryTool {
        event_tx: event_tx.clone(),
    })?;
    registry.register_typed_tool(CognitionCalendarMutateTool { event_tx })?;
    Ok(())
}

#[medousa_tool(id = CALENDAR_QUERY_ID)]
impl CognitionCalendarQueryTool {
    /// Read personal calendar: list events or export ICS. action is a typed name (calendar.list, calendar.export). Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(
        &self,
        action: CalendarQueryAction,
    ) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch_query(self, action).await?))
    }
}

#[medousa_tool(id = CALENDAR_MUTATE_ID)]
impl CognitionCalendarMutateTool {
    /// Write personal calendar: create, update, delete, or import ICS. action is a typed name (calendar.create, calendar.update, calendar.delete, calendar.import). Fetch fields with cognition_schema types=[...].
    async fn invoke_typed(
        &self,
        action: CalendarMutateAction,
    ) -> stasis::prelude::Result<ExternalJson> {
        Ok(ExternalJson::new(dispatch_mutate(self, action).await?))
    }
}

async fn dispatch_query(
    tool: &CognitionCalendarQueryTool,
    action: CalendarQueryAction,
) -> stasis::prelude::Result<Value> {
    match action {
        CalendarQueryAction::List(params) => params.execute(tool).await,
        CalendarQueryAction::Export(params) => params.execute(tool).await,
    }
}

async fn dispatch_mutate(
    tool: &CognitionCalendarMutateTool,
    action: CalendarMutateAction,
) -> stasis::prelude::Result<Value> {
    match action {
        CalendarMutateAction::Create(params) => params.execute(tool).await,
        CalendarMutateAction::Update(params) => params.execute(tool).await,
        CalendarMutateAction::Delete(params) => params.execute(tool).await,
        CalendarMutateAction::Import(params) => params.execute(tool).await,
    }
}

fn alarm_inputs(alarms: Option<Vec<CalendarAlarm>>) -> Option<Vec<CalendarAlarmInput>> {
    alarms.map(|alarms| {
        alarms
            .into_iter()
            .map(|alarm| CalendarAlarmInput {
                trigger_minutes_before: alarm.trigger_minutes_before,
                action: alarm.action,
            })
            .collect()
    })
}

#[allow(clippy::too_many_arguments)]
fn write_fields(
    summary: String,
    dtstart: String,
    description: Option<String>,
    location: Option<String>,
    dtend: Option<String>,
    all_day: Option<bool>,
    rrule: Option<String>,
    note_path: Option<String>,
    alarms: Option<Vec<CalendarAlarm>>,
    path: Option<String>,
    calendar_path: Option<String>,
) -> CalendarWriteFieldsInput {
    CalendarWriteFieldsInput {
        summary: Some(summary),
        dtstart: Some(dtstart),
        description,
        location,
        dtend,
        all_day,
        rrule,
        note_path,
        alarms: alarm_inputs(alarms),
        path,
        calendar_path,
    }
}

impl CalendarList {
    async fn execute(self, tool: &CognitionCalendarQueryTool) -> stasis::prelude::Result<Value> {
        let output = CognitionCalendarListTool::new(tool.event_tx.clone())
            .invoke_typed(CalendarListInput {
                from: CompatOption::from(self.from),
                to: CompatOption::from(self.to),
                path: CompatOption::from(self.path),
            })
            .await?;
        serialize_output(CognitionCalendarListTool::tool_id(), output)
    }
}

impl CalendarExport {
    async fn execute(self, tool: &CognitionCalendarQueryTool) -> stasis::prelude::Result<Value> {
        let output = CognitionCalendarExportTool::new(tool.event_tx.clone())
            .invoke_typed(CalendarExportInput {
                path: CompatOption::from(self.path),
            })
            .await?;
        serialize_output(CognitionCalendarExportTool::tool_id(), output)
    }
}

impl CalendarCreate {
    async fn execute(self, tool: &CognitionCalendarMutateTool) -> stasis::prelude::Result<Value> {
        let output = CognitionCalendarCreateTool::new(tool.event_tx.clone())
            .invoke_typed(CalendarCreateInput {
                fields: write_fields(
                    self.summary,
                    self.dtstart,
                    self.description,
                    self.location,
                    self.dtend,
                    self.all_day,
                    self.rrule,
                    self.note_path,
                    self.alarms,
                    self.path,
                    self.calendar_path,
                ),
                hidden_uid: None,
            })
            .await?;
        serialize_output(CognitionCalendarCreateTool::tool_id(), output)
    }
}

impl CalendarUpdate {
    async fn execute(self, tool: &CognitionCalendarMutateTool) -> stasis::prelude::Result<Value> {
        let output = CognitionCalendarUpdateTool::new(tool.event_tx.clone())
            .invoke_typed(CalendarUpdateInput {
                uid: Some(self.uid),
                fields: write_fields(
                    self.summary,
                    self.dtstart,
                    self.description,
                    self.location,
                    self.dtend,
                    self.all_day,
                    self.rrule,
                    self.note_path,
                    self.alarms,
                    self.path,
                    self.calendar_path,
                ),
            })
            .await?;
        serialize_output(CognitionCalendarUpdateTool::tool_id(), output)
    }
}

impl CalendarDelete {
    async fn execute(self, tool: &CognitionCalendarMutateTool) -> stasis::prelude::Result<Value> {
        let output = CognitionCalendarDeleteTool::new(tool.event_tx.clone())
            .invoke_typed(CalendarDeleteInput {
                uid: Some(self.uid),
                path: self.path,
            })
            .await?;
        serialize_output(CognitionCalendarDeleteTool::tool_id(), output)
    }
}

impl CalendarImport {
    async fn execute(self, tool: &CognitionCalendarMutateTool) -> stasis::prelude::Result<Value> {
        let output = CognitionCalendarImportTool::new(tool.event_tx.clone())
            .invoke_typed(CalendarImportInput {
                ics: Some(self.ics),
                path: self.path,
                calendar_path: self.calendar_path,
            })
            .await?;
        serialize_output(CognitionCalendarImportTool::tool_id(), output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn calendar_actions_carry_their_params() {
        let query: CalendarQueryAction = serde_json::from_value(json!({
            "action": "calendar.list",
            "from": "2026-08-18T00:00:00Z"
        }))
        .expect("list");
        match query {
            CalendarQueryAction::List(CalendarList { from, .. }) => {
                assert_eq!(from.as_deref(), Some("2026-08-18T00:00:00Z"));
            }
            other => panic!("expected calendar.list, got {other:?}"),
        }
        let mutate: CalendarMutateAction = serde_json::from_value(json!({
            "action": "calendar.create",
            "summary": "Standup",
            "dtstart": "2026-08-18T17:00:00Z"
        }))
        .expect("create");
        match mutate {
            CalendarMutateAction::Create(CalendarCreate {
                summary, dtstart, ..
            }) => {
                assert_eq!(summary, "Standup");
                assert_eq!(dtstart, "2026-08-18T17:00:00Z");
            }
            other => panic!("expected calendar.create, got {other:?}"),
        }
    }

    #[test]
    fn advertised_schemas_are_action_enums_only() {
        let query =
            serde_json::to_value(schemars::schema_for!(CalendarQueryAction)).expect("query");
        let mutate =
            serde_json::to_value(schemars::schema_for!(CalendarMutateAction)).expect("mutate");
        for schema in [&query, &mutate] {
            let props = schema["properties"].as_object().expect("properties");
            assert_eq!(props.len(), 1);
            assert!(
                props["action"]["enum"]
                    .as_array()
                    .is_some_and(|values| !values.is_empty())
            );
            assert_eq!(schema["additionalProperties"], true);
        }
    }
}
