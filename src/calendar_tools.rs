//! Host/worker calendar tools: list, create, update, delete, import, export.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use stasis::application::orchestration::tool_registry::StasisTool;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use tokio::sync::mpsc;

use crate::calendar::CalendarService;
use crate::daemon_api::{CalendarImportRequest, CalendarWriteRequest};
use crate::events::TuiEvent;

pub fn register_calendar_tools(
    registry: &mut stasis::application::orchestration::tool_registry::InMemoryToolRegistry,
    event_tx: mpsc::Sender<TuiEvent>,
) -> StasisResult<()> {
    registry.register_tool(CognitionCalendarListTool::new(event_tx.clone()))?;
    registry.register_tool(CognitionCalendarCreateTool::new(event_tx.clone()))?;
    registry.register_tool(CognitionCalendarUpdateTool::new(event_tx.clone()))?;
    registry.register_tool(CognitionCalendarDeleteTool::new(event_tx.clone()))?;
    registry.register_tool(CognitionCalendarImportTool::new(event_tx.clone()))?;
    registry.register_tool(CognitionCalendarExportTool::new(event_tx))?;
    Ok(())
}

fn emit_invoked(event_tx: &mpsc::Sender<TuiEvent>, tool_name: &str, summary: &str) {
    let _ = event_tx.try_send(TuiEvent::ToolInvoked {
        tool_name: tool_name.to_string(),
        input_summary: summary.to_string(),
    });
}

fn parse_rfc3339(value: Option<&Value>, field: &str) -> StasisResult<Option<DateTime<Utc>>> {
    let Some(raw) = value.and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()) else {
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

#[async_trait]
impl StasisTool for CognitionCalendarListTool {
    fn name(&self) -> &'static str {
        "cognition_calendar_list"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "List personal calendar events in a time range (RRULE expanded). Default store: calendar/personal.ics.",
        )
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "from": { "type": "string", "description": "RFC3339 range start (inclusive)" },
                "to": { "type": "string", "description": "RFC3339 range end (exclusive)" },
                "path": { "type": "string", "description": "Vault-relative .ics path (default calendar/personal.ics)" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let from = parse_rfc3339(input.get("from"), "from")?;
        let to = parse_rfc3339(input.get("to"), "to")?;
        let path = optional_string(&input, "path");
        emit_invoked(
            &self.event_tx,
            self.name(),
            path.as_deref().unwrap_or("calendar/personal.ics"),
        );
        let response = CalendarService::list_events(path.as_deref(), from, to)
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        serde_json::to_value(response).map_err(|err| StasisError::PortFailure(err.to_string()))
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

#[async_trait]
impl StasisTool for CognitionCalendarExportTool {
    fn name(&self) -> &'static str {
        "cognition_calendar_export"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Export the vault calendar as raw ICS text.")
    }

    fn input_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Vault-relative .ics path (default calendar/personal.ics)" }
            }
        }))
    }

    async fn invoke(&self, input: Value) -> StasisResult<Value> {
        let path = optional_string(&input, "path");
        emit_invoked(
            &self.event_tx,
            self.name(),
            path.as_deref().unwrap_or("calendar/personal.ics"),
        );
        let response = CalendarService::export(path.as_deref())
            .map_err(|err| StasisError::PortFailure(err.to_string()))?;
        serde_json::to_value(response).map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}
