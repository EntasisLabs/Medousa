//! Host/worker calendar tools: list, create, update, delete, import, export.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use tokio::sync::mpsc;

use crate::calendar::CalendarService;
use crate::daemon_api::{
    CalendarExportResponse, CalendarImportRequest, CalendarListResponse, CalendarWriteRequest,
};
use crate::events::TuiEvent;
use crate::typed_tools::{ToolId, medousa_tool};

const COGNITION_CALENDAR_LIST_ID: ToolId = ToolId::new("cognition_calendar_list");
const COGNITION_CALENDAR_EXPORT_ID: ToolId = ToolId::new("cognition_calendar_export");

pub fn register_calendar_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    event_tx: mpsc::Sender<TuiEvent>,
) -> StasisResult<()> {
    registry.register_typed_tool(CognitionCalendarListTool::new(event_tx.clone()))?;
    registry.register_tool(CognitionCalendarCreateTool::new(event_tx.clone()))?;
    registry.register_tool(CognitionCalendarUpdateTool::new(event_tx.clone()))?;
    registry.register_tool(CognitionCalendarDeleteTool::new(event_tx.clone()))?;
    registry.register_tool(CognitionCalendarImportTool::new(event_tx.clone()))?;
    registry.register_typed_tool(CognitionCalendarExportTool::new(event_tx))?;
    Ok(())
}

fn emit_invoked(event_tx: &mpsc::Sender<TuiEvent>, tool_name: &str, summary: &str) {
    let _ = event_tx.try_send(TuiEvent::ToolInvoked {
        tool_name: tool_name.to_string(),
        input_summary: summary.to_string(),
    });
}

fn parse_rfc3339(value: Option<&Value>, field: &str) -> StasisResult<Option<DateTime<Utc>>> {
    parse_rfc3339_str(value.and_then(Value::as_str), field)
}

fn parse_rfc3339_str(raw: Option<&str>, field: &str) -> StasisResult<Option<DateTime<Utc>>> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| Some(dt.with_timezone(&Utc)))
        .map_err(|err| StasisError::PortFailure(format!("invalid {field}: {err}")))
}

fn require_rfc3339(value: Option<&Value>, field: &str) -> StasisResult<DateTime<Utc>> {
    parse_rfc3339(value, field)?
        .ok_or_else(|| StasisError::PortFailure(format!("{field} is required (RFC3339)")))
}

fn optional_string(input: &Value, field: &str) -> Option<String> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn write_request_from_input(input: &Value, require_summary: bool) -> StasisResult<CalendarWriteRequest> {
    let summary = optional_string(input, "summary").unwrap_or_default();
    if require_summary && summary.is_empty() {
        return Err(StasisError::PortFailure("summary is required".to_string()));
    }
    let alarms = input
        .get("alarms")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let minutes = item
                        .get("trigger_minutes_before")
                        .and_then(Value::as_i64)
                        .or_else(|| {
                            item.get("trigger_minutes_before")
                                .and_then(Value::as_u64)
                                .map(|v| v as i64)
                        })?;
                    if minutes <= 0 {
                        return None;
                    }
                    Some(medousa_types::CalendarAlarm {
                        trigger_minutes_before: minutes.min(i32::MAX as i64) as i32,
                        action: item
                            .get("action")
                            .and_then(Value::as_str)
                            .unwrap_or("display")
                            .to_string(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(CalendarWriteRequest {
        uid: optional_string(input, "uid"),
        summary,
        description: optional_string(input, "description"),
        location: optional_string(input, "location"),
        dtstart: require_rfc3339(input.get("dtstart"), "dtstart")?,
        dtend: parse_rfc3339(input.get("dtend"), "dtend")?,
        all_day: input
            .get("all_day")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        rrule: optional_string(input, "rrule"),
        calendar_path: optional_string(input, "path").or_else(|| optional_string(input, "calendar_path")),
        note_path: optional_string(input, "note_path"),
        alarms,
    })
}

const WRITE_SCHEMA_PROPERTIES: &str = r#"{
  "summary": { "type": "string", "description": "Event title" },
  "description": { "type": "string" },
  "location": { "type": "string" },
  "dtstart": { "type": "string", "description": "RFC3339 start. All-day: YYYY-MM-DDT00:00:00Z for that calendar date." },
  "dtend": { "type": "string", "description": "RFC3339 end. All-day: exclusive next-day YYYY-MM-DDT00:00:00Z." },
  "all_day": { "type": "boolean", "description": "True for DATE (calendar-day) events; use UTC midnights for dtstart/dtend." },
  "rrule": { "type": "string", "description": "Optional RRULE body (without RRULE: prefix)" },
  "note_path": { "type": "string", "description": "Optional vault-relative markdown note linked to this event" },
  "alarms": {
    "type": "array",
    "description": "Display alerts before start",
    "items": {
      "type": "object",
      "properties": {
        "trigger_minutes_before": { "type": "integer", "description": "Minutes before dtstart" },
        "action": { "type": "string", "description": "VALARM action (display)" }
      },
      "required": ["trigger_minutes_before"]
    }
  },
  "path": { "type": "string", "description": "Vault-relative .ics path (default calendar/personal.ics)" },
  "calendar_path": { "type": "string", "description": "Alias for path" }
}"#;

pub struct CognitionCalendarListTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionCalendarListTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CalendarListInput {
    /// RFC3339 range start (inclusive)
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    from: Option<String>,
    /// RFC3339 range end (exclusive)
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    to: Option<String>,
    /// Vault-relative .ics path (default calendar/personal.ics)
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    path: Option<String>,
}

#[medousa_tool(id = COGNITION_CALENDAR_LIST_ID)]
impl CognitionCalendarListTool {
    /// List personal calendar events in a time range (RRULE expanded). Default store: calendar/personal.ics.
    async fn invoke_typed(
        &self,
        input: CalendarListInput,
    ) -> stasis::prelude::Result<CalendarListResponse> {
        let from = parse_rfc3339_str(input.from.as_deref(), "from")?;
        let to = parse_rfc3339_str(input.to.as_deref(), "to")?;
        let path = input
            .path
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        emit_invoked(
            &self.event_tx,
            COGNITION_CALENDAR_LIST_ID.as_str(),
            path.as_deref().unwrap_or("calendar/personal.ics"),
        );
        CalendarService::list_events(path.as_deref(), from, to)
            .map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}

pub struct CognitionCalendarCreateTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionCalendarCreateTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionCalendarCreateTool {
    fn name(&self) -> &'static str {
        "cognition_calendar_create"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Create a calendar event in the vault .ics store. For all-day events set all_day=true and use UTC midnights for the calendar date.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        let mut schema = json!({
            "type": "object",
            "required": ["summary", "dtstart"],
            "properties": {}
        });
        let props: Value = serde_json::from_str(WRITE_SCHEMA_PROPERTIES)
            .unwrap_or_else(|_| json!({}));
        schema["properties"] = props;
        Some(schema)
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let request = write_request_from_input(&input, true)?;
        emit_invoked(&self.event_tx, self.name(), &request.summary);
        let response = CalendarService::create_event(&request)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        serde_json::to_value(response).map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}

pub struct CognitionCalendarUpdateTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionCalendarUpdateTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionCalendarUpdateTool {
    fn name(&self) -> &'static str {
        "cognition_calendar_update"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Update an existing calendar event by uid (full replace of mutable fields).")
    }

    fn input_schema(&self) -> Option<Value> {
        let mut schema = json!({
            "type": "object",
            "required": ["uid", "summary", "dtstart"],
            "properties": {
                "uid": { "type": "string", "description": "Event UID to update" }
            }
        });
        let mut props: serde_json::Map<String, Value> = serde_json::from_str(WRITE_SCHEMA_PROPERTIES)
            .unwrap_or_default();
        props.insert(
            "uid".to_string(),
            json!({ "type": "string", "description": "Event UID to update" }),
        );
        schema["properties"] = Value::Object(props);
        Some(schema)
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let uid = optional_string(&input, "uid")
            .ok_or_else(|| StasisError::PortFailure("uid is required".to_string()))?;
        let request = write_request_from_input(&input, true)?;
        emit_invoked(&self.event_tx, self.name(), &uid);
        let response = CalendarService::update_event(&uid, &request)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        serde_json::to_value(response).map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}

pub struct CognitionCalendarDeleteTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionCalendarDeleteTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionCalendarDeleteTool {
    fn name(&self) -> &'static str {
        "cognition_calendar_delete"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Delete a calendar event by uid from the vault .ics store.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["uid"],
            "properties": {
                "uid": { "type": "string" },
                "path": { "type": "string", "description": "Vault-relative .ics path (default calendar/personal.ics)" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let uid = optional_string(&input, "uid")
            .ok_or_else(|| StasisError::PortFailure("uid is required".to_string()))?;
        let path = optional_string(&input, "path");
        emit_invoked(&self.event_tx, self.name(), &uid);
        let response = CalendarService::delete_event(&uid, path.as_deref())
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        serde_json::to_value(response).map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}

pub struct CognitionCalendarImportTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionCalendarImportTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl StasisTool for CognitionCalendarImportTool {
    fn name(&self) -> &'static str {
        "cognition_calendar_import"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Merge VEVENT components from raw ICS text into the vault calendar (UID upsert).")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "required": ["ics"],
            "properties": {
                "ics": { "type": "string", "description": "Raw RFC 5545 text" },
                "path": { "type": "string", "description": "Vault-relative .ics path (default calendar/personal.ics)" },
                "calendar_path": { "type": "string", "description": "Alias for path" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let ics = optional_string(&input, "ics")
            .ok_or_else(|| StasisError::PortFailure("ics is required".to_string()))?;
        let path = optional_string(&input, "path").or_else(|| optional_string(&input, "calendar_path"));
        emit_invoked(
            &self.event_tx,
            self.name(),
            path.as_deref().unwrap_or("calendar/personal.ics"),
        );
        let request = CalendarImportRequest {
            ics,
            calendar_path: path,
        };
        let response = CalendarService::import(&request)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        serde_json::to_value(response).map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}

pub struct CognitionCalendarExportTool {
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionCalendarExportTool {
    pub fn new(event_tx: mpsc::Sender<TuiEvent>) -> Self {
        Self { event_tx }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CalendarExportInput {
    /// Vault-relative .ics path (default calendar/personal.ics)
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    path: Option<String>,
}

#[medousa_tool(id = COGNITION_CALENDAR_EXPORT_ID)]
impl CognitionCalendarExportTool {
    /// Export the vault calendar as raw ICS text.
    async fn invoke_typed(
        &self,
        input: CalendarExportInput,
    ) -> stasis::prelude::Result<CalendarExportResponse> {
        let path = input
            .path
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        emit_invoked(
            &self.event_tx,
            COGNITION_CALENDAR_EXPORT_ID.as_str(),
            path.as_deref().unwrap_or("calendar/personal.ics"),
        );
        CalendarService::export(path.as_deref())
            .map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}
