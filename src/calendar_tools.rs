//! Host/worker calendar tools: list, create, update, delete, import, export.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use tokio::sync::mpsc;

use crate::calendar::CalendarService;
use crate::daemon_api::{
    CalendarAlarm, CalendarDeleteResponse, CalendarExportResponse, CalendarImportRequest,
    CalendarImportResponse, CalendarListResponse, CalendarWriteRequest, CalendarWriteResponse,
};
use crate::events::TuiEvent;
use crate::semantic_values::{RequiredContent, TrimmedText};
use crate::typed_tools::{ToolId, medousa_tool};

const COGNITION_CALENDAR_LIST_ID: ToolId = ToolId::new("cognition_calendar_list");
const COGNITION_CALENDAR_EXPORT_ID: ToolId = ToolId::new("cognition_calendar_export");
const COGNITION_CALENDAR_CREATE_ID: ToolId = ToolId::new("cognition_calendar_create");
const COGNITION_CALENDAR_UPDATE_ID: ToolId = ToolId::new("cognition_calendar_update");
const COGNITION_CALENDAR_DELETE_ID: ToolId = ToolId::new("cognition_calendar_delete");
const COGNITION_CALENDAR_IMPORT_ID: ToolId = ToolId::new("cognition_calendar_import");

pub fn register_calendar_tools(
    registry: &mut impl crate::typed_tools::ToolRegistration,
    event_tx: mpsc::Sender<TuiEvent>,
) -> StasisResult<()> {
    registry.register_typed_tool(CognitionCalendarListTool::new(event_tx.clone()))?;
    registry.register_typed_tool(CognitionCalendarCreateTool::new(event_tx.clone()))?;
    registry.register_typed_tool(CognitionCalendarUpdateTool::new(event_tx.clone()))?;
    registry.register_typed_tool(CognitionCalendarDeleteTool::new(event_tx.clone()))?;
    registry.register_typed_tool(CognitionCalendarImportTool::new(event_tx.clone()))?;
    registry.register_typed_tool(CognitionCalendarExportTool::new(event_tx))?;
    Ok(())
}

fn emit_invoked(event_tx: &mpsc::Sender<TuiEvent>, tool_name: &str, summary: &str) {
    let _ = event_tx.try_send(TuiEvent::ToolInvoked {
        tool_name: tool_name.to_string(),
        input_summary: summary.to_string(),
    });
}

fn parse_rfc3339_str(raw: Option<&str>, field: &str) -> StasisResult<Option<DateTime<Utc>>> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| Some(dt.with_timezone(&Utc)))
        .map_err(|err| StasisError::PortFailure(format!("invalid {field}: {err}")))
}

fn optional_trimmed(value: Option<String>) -> Option<TrimmedText> {
    value.and_then(|value| TrimmedText::new(value).ok())
}

fn required_trimmed(value: Option<String>, field: &str) -> StasisResult<TrimmedText> {
    let value = value.ok_or_else(|| StasisError::PortFailure(format!("{field} is required")))?;
    TrimmedText::new(value).map_err(|_| StasisError::PortFailure(format!("{field} is required")))
}

#[derive(Debug, JsonSchema)]
pub struct CalendarAlarmInput {
    /// Minutes before dtstart
    trigger_minutes_before: i64,
    /// VALARM action (display)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    action: Option<String>,
}

fn deserialize_lenient_calendar_alarms<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<CalendarAlarmInput>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    let alarms = value
        .as_array()
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
                                .map(|value| value as i64)
                        })?;
                    (minutes > 0).then(|| CalendarAlarmInput {
                        trigger_minutes_before: minutes,
                        action: item
                            .get("action")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Some(alarms))
}

#[derive(Debug, JsonSchema)]
pub struct CalendarWriteFieldsInput {
    /// Event title
    #[schemars(required, with = "String")]
    summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    location: Option<String>,
    /// RFC3339 start. All-day: YYYY-MM-DDT00:00:00Z for that calendar date.
    #[schemars(required, with = "String")]
    dtstart: Option<String>,
    /// RFC3339 end. All-day: exclusive next-day YYYY-MM-DDT00:00:00Z.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    dtend: Option<String>,
    /// True for DATE (calendar-day) events; use UTC midnights for dtstart/dtend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "bool", skip_serializing_if = "Option::is_none")]
    all_day: Option<bool>,
    /// Optional RRULE body (without RRULE: prefix)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    rrule: Option<String>,
    /// Optional vault-relative markdown note linked to this event
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    note_path: Option<String>,
    /// Display alerts before start
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        with = "Vec<CalendarAlarmInput>",
        skip_serializing_if = "Option::is_none"
    )]
    alarms: Option<Vec<CalendarAlarmInput>>,
    /// Vault-relative .ics path (default calendar/personal.ics)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    /// Alias for path
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    calendar_path: Option<String>,
}

impl CalendarWriteFieldsInput {
    fn into_request(self, uid: Option<String>) -> StasisResult<CalendarWriteRequest> {
        Ok(CalendarWriteCommand::from_fields(self, uid)?.into_request())
    }
}

#[derive(Debug)]
struct CalendarWriteCommand {
    uid: Option<TrimmedText>,
    summary: TrimmedText,
    description: Option<TrimmedText>,
    location: Option<TrimmedText>,
    dtstart: DateTime<Utc>,
    dtend: Option<DateTime<Utc>>,
    all_day: bool,
    rrule: Option<TrimmedText>,
    note_path: Option<TrimmedText>,
    alarms: Vec<CalendarAlarm>,
    calendar_path: Option<TrimmedText>,
}

impl CalendarWriteCommand {
    fn from_fields(fields: CalendarWriteFieldsInput, uid: Option<String>) -> StasisResult<Self> {
        let summary = required_trimmed(fields.summary, "summary")?;
        let dtstart = parse_rfc3339_str(fields.dtstart.as_deref(), "dtstart")?
            .ok_or_else(|| StasisError::PortFailure("dtstart is required (RFC3339)".to_string()))?;
        let alarms = fields
            .alarms
            .unwrap_or_default()
            .into_iter()
            .map(|alarm| CalendarAlarm {
                trigger_minutes_before: alarm.trigger_minutes_before.min(i32::MAX as i64) as i32,
                action: alarm.action.unwrap_or_else(|| "display".to_string()),
            })
            .collect();

        Ok(Self {
            uid: optional_trimmed(uid),
            summary,
            description: optional_trimmed(fields.description),
            location: optional_trimmed(fields.location),
            dtstart,
            dtend: parse_rfc3339_str(fields.dtend.as_deref(), "dtend")?,
            all_day: fields.all_day.unwrap_or(false),
            rrule: optional_trimmed(fields.rrule),
            note_path: optional_trimmed(fields.note_path),
            alarms,
            calendar_path: optional_trimmed(fields.path)
                .or_else(|| optional_trimmed(fields.calendar_path)),
        })
    }

    fn into_request(self) -> CalendarWriteRequest {
        CalendarWriteRequest {
            uid: self.uid.map(TrimmedText::into_string),
            summary: self.summary.into_string(),
            description: self.description.map(TrimmedText::into_string),
            location: self.location.map(TrimmedText::into_string),
            dtstart: self.dtstart,
            dtend: self.dtend,
            all_day: self.all_day,
            rrule: self.rrule.map(TrimmedText::into_string),
            calendar_path: self.calendar_path.map(TrimmedText::into_string),
            note_path: self.note_path.map(TrimmedText::into_string),
            alarms: self.alarms,
        }
    }
}

#[derive(Deserialize)]
struct CalendarWriteWire {
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    uid: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    summary: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    description: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    location: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    dtstart: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    dtend: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_bool"
    )]
    all_day: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    rrule: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    note_path: Option<String>,
    #[serde(default, deserialize_with = "deserialize_lenient_calendar_alarms")]
    alarms: Option<Vec<CalendarAlarmInput>>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    path: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
    )]
    calendar_path: Option<String>,
}

impl CalendarWriteWire {
    fn into_parts(self) -> (Option<String>, CalendarWriteFieldsInput) {
        (
            self.uid,
            CalendarWriteFieldsInput {
                summary: self.summary,
                description: self.description,
                location: self.location,
                dtstart: self.dtstart,
                dtend: self.dtend,
                all_day: self.all_day,
                rrule: self.rrule,
                note_path: self.note_path,
                alarms: self.alarms,
                path: self.path,
                calendar_path: self.calendar_path,
            },
        )
    }
}

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

#[derive(Debug)]
struct CalendarListCommand {
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    path: Option<TrimmedText>,
}

impl TryFrom<CalendarListInput> for CalendarListCommand {
    type Error = StasisError;

    fn try_from(input: CalendarListInput) -> Result<Self, Self::Error> {
        Ok(Self {
            from: parse_rfc3339_str(input.from.as_deref(), "from")?,
            to: parse_rfc3339_str(input.to.as_deref(), "to")?,
            path: optional_trimmed(input.path),
        })
    }
}

#[medousa_tool(id = COGNITION_CALENDAR_LIST_ID)]
impl CognitionCalendarListTool {
    /// List personal calendar events in a time range (RRULE expanded). Default store: calendar/personal.ics.
    async fn invoke_typed(
        &self,
        input: CalendarListInput,
    ) -> stasis::prelude::Result<CalendarListResponse> {
        let command = CalendarListCommand::try_from(input)?;
        let from = command.from;
        let to = command.to;
        let path = command.path.map(TrimmedText::into_string);
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

#[derive(Debug, JsonSchema)]
pub struct CalendarCreateInput {
    #[serde(flatten)]
    fields: CalendarWriteFieldsInput,
    #[schemars(skip)]
    hidden_uid: Option<String>,
}

impl<'de> Deserialize<'de> for CalendarCreateInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (hidden_uid, fields) = CalendarWriteWire::deserialize(deserializer)?.into_parts();
        Ok(Self { fields, hidden_uid })
    }
}

#[medousa_tool(id = COGNITION_CALENDAR_CREATE_ID)]
impl CognitionCalendarCreateTool {
    /// Create a calendar event in the vault .ics store. For all-day events set all_day=true and use UTC midnights for the calendar date.
    async fn invoke_typed(
        &self,
        input: CalendarCreateInput,
    ) -> stasis::prelude::Result<CalendarWriteResponse> {
        let request = input.fields.into_request(input.hidden_uid)?;
        emit_invoked(
            &self.event_tx,
            COGNITION_CALENDAR_CREATE_ID.as_str(),
            &request.summary,
        );
        CalendarService::create_event(&request)
            .map_err(|err| StasisError::PortFailure(err.to_string()))
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

#[derive(Debug, JsonSchema)]
pub struct CalendarUpdateInput {
    /// Event UID to update
    #[schemars(required, with = "String")]
    uid: Option<String>,
    #[serde(flatten)]
    fields: CalendarWriteFieldsInput,
}

impl<'de> Deserialize<'de> for CalendarUpdateInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (uid, fields) = CalendarWriteWire::deserialize(deserializer)?.into_parts();
        Ok(Self { uid, fields })
    }
}

#[medousa_tool(id = COGNITION_CALENDAR_UPDATE_ID)]
impl CognitionCalendarUpdateTool {
    /// Update an existing calendar event by uid (full replace of mutable fields).
    async fn invoke_typed(
        &self,
        input: CalendarUpdateInput,
    ) -> stasis::prelude::Result<CalendarWriteResponse> {
        let uid = required_trimmed(input.uid, "uid")?.into_string();
        let request = input.fields.into_request(Some(uid.clone()))?;
        emit_invoked(&self.event_tx, COGNITION_CALENDAR_UPDATE_ID.as_str(), &uid);
        CalendarService::update_event(&uid, &request)
            .map_err(|err| StasisError::PortFailure(err.to_string()))
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

#[derive(Debug, JsonSchema)]
pub struct CalendarDeleteInput {
    #[schemars(required, with = "String")]
    uid: Option<String>,
    /// Vault-relative .ics path (default calendar/personal.ics)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    path: Option<String>,
}

impl<'de> Deserialize<'de> for CalendarDeleteInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            uid: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            path: Option<String>,
        }
        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            uid: input.uid,
            path: input.path,
        })
    }
}

#[derive(Debug)]
struct CalendarDeleteCommand {
    uid: TrimmedText,
    path: Option<TrimmedText>,
}

impl TryFrom<CalendarDeleteInput> for CalendarDeleteCommand {
    type Error = StasisError;

    fn try_from(input: CalendarDeleteInput) -> Result<Self, Self::Error> {
        Ok(Self {
            uid: required_trimmed(input.uid, "uid")?,
            path: optional_trimmed(input.path),
        })
    }
}

#[medousa_tool(id = COGNITION_CALENDAR_DELETE_ID)]
impl CognitionCalendarDeleteTool {
    /// Delete a calendar event by uid from the vault .ics store.
    async fn invoke_typed(
        &self,
        input: CalendarDeleteInput,
    ) -> stasis::prelude::Result<CalendarDeleteResponse> {
        let command = CalendarDeleteCommand::try_from(input)?;
        let uid = command.uid.into_string();
        let path = command.path.map(TrimmedText::into_string);
        emit_invoked(&self.event_tx, COGNITION_CALENDAR_DELETE_ID.as_str(), &uid);
        CalendarService::delete_event(&uid, path.as_deref())
            .map_err(|err| StasisError::PortFailure(err.to_string()))
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

#[derive(Debug, JsonSchema)]
pub struct CalendarImportInput {
    /// Raw RFC 5545 text
    #[schemars(required, with = "String")]
    ics: Option<String>,
    /// Vault-relative .ics path (default calendar/personal.ics)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    /// Alias for path
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    calendar_path: Option<String>,
}

impl<'de> Deserialize<'de> for CalendarImportInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            ics: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            path: Option<String>,
            #[serde(
                default,
                deserialize_with = "crate::typed_tools::deserialize_lenient_optional_string"
            )]
            calendar_path: Option<String>,
        }
        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            ics: input.ics,
            path: input.path,
            calendar_path: input.calendar_path,
        })
    }
}

#[derive(Debug)]
struct CalendarImportCommand {
    ics: RequiredContent,
    calendar_path: Option<TrimmedText>,
}

impl TryFrom<CalendarImportInput> for CalendarImportCommand {
    type Error = StasisError;

    fn try_from(input: CalendarImportInput) -> Result<Self, Self::Error> {
        let ics = input
            .ics
            .ok_or_else(|| StasisError::PortFailure("ics is required".to_string()))
            .and_then(|value| {
                RequiredContent::new(value)
                    .map_err(|_| StasisError::PortFailure("ics is required".to_string()))
            })?;
        Ok(Self {
            ics,
            calendar_path: optional_trimmed(input.path)
                .or_else(|| optional_trimmed(input.calendar_path)),
        })
    }
}

#[medousa_tool(id = COGNITION_CALENDAR_IMPORT_ID)]
impl CognitionCalendarImportTool {
    /// Merge VEVENT components from raw ICS text into the vault calendar (UID upsert).
    async fn invoke_typed(
        &self,
        input: CalendarImportInput,
    ) -> stasis::prelude::Result<CalendarImportResponse> {
        let command = CalendarImportCommand::try_from(input)?;
        let ics = command.ics.into_string();
        let path = command.calendar_path.map(TrimmedText::into_string);
        emit_invoked(
            &self.event_tx,
            COGNITION_CALENDAR_IMPORT_ID.as_str(),
            path.as_deref().unwrap_or("calendar/personal.ics"),
        );
        let request = CalendarImportRequest {
            ics,
            calendar_path: path,
        };
        CalendarService::import(&request).map_err(|err| StasisError::PortFailure(err.to_string()))
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

#[derive(Debug)]
struct CalendarExportCommand {
    path: Option<TrimmedText>,
}

impl TryFrom<CalendarExportInput> for CalendarExportCommand {
    type Error = StasisError;

    fn try_from(input: CalendarExportInput) -> Result<Self, Self::Error> {
        Ok(Self {
            path: optional_trimmed(input.path),
        })
    }
}

#[medousa_tool(id = COGNITION_CALENDAR_EXPORT_ID)]
impl CognitionCalendarExportTool {
    /// Export the vault calendar as raw ICS text.
    async fn invoke_typed(
        &self,
        input: CalendarExportInput,
    ) -> stasis::prelude::Result<CalendarExportResponse> {
        let command = CalendarExportCommand::try_from(input)?;
        let path = command.path.map(TrimmedText::into_string);
        emit_invoked(
            &self.event_tx,
            COGNITION_CALENDAR_EXPORT_ID.as_str(),
            path.as_deref().unwrap_or("calendar/personal.ics"),
        );
        CalendarService::export(path.as_deref())
            .map_err(|err| StasisError::PortFailure(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn calendar_create_input_preserves_hidden_uid_aliases_and_alarm_defaults() {
        let input: CalendarCreateInput = serde_json::from_value(json!({
            "uid": "event-1",
            "summary": "  Typed migration  ",
            "dtstart": "2026-08-09T12:00:00Z",
            "all_day": "not-a-bool",
            "path": "   ",
            "calendar_path": "calendar/team.ics",
            "alarms": [
                { "trigger_minutes_before": 15, "action": false },
                { "trigger_minutes_before": 0 },
                { "trigger_minutes_before": "soon" }
            ]
        }))
        .expect("legacy-compatible calendar input");

        let request = input
            .fields
            .into_request(input.hidden_uid)
            .expect("valid write request");
        assert_eq!(request.uid.as_deref(), Some("event-1"));
        assert_eq!(request.summary, "Typed migration");
        assert!(!request.all_day);
        assert_eq!(request.calendar_path.as_deref(), Some("calendar/team.ics"));
        assert_eq!(request.alarms.len(), 1);
        assert_eq!(request.alarms[0].trigger_minutes_before, 15);
        assert_eq!(request.alarms[0].action, "display");
    }

    #[test]
    fn calendar_commands_normalize_identifiers_and_preserve_ics_content() {
        let raw_ics = " \r\nBEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n ";
        let import = CalendarImportCommand::try_from(CalendarImportInput {
            ics: Some(raw_ics.to_string()),
            path: Some(" calendar/team.ics ".to_string()),
            calendar_path: Some("calendar/fallback.ics".to_string()),
        })
        .expect("import command");
        assert_eq!(import.ics.as_str(), raw_ics);
        assert_eq!(
            import.calendar_path.as_ref().map(TrimmedText::as_str),
            Some("calendar/team.ics")
        );

        let list = CalendarListCommand::try_from(CalendarListInput {
            from: Some(" 2026-08-09T00:00:00Z ".to_string()),
            to: Some("2026-08-10T00:00:00Z".to_string()),
            path: Some(" calendar/team.ics ".to_string()),
        })
        .expect("list command");
        assert_eq!(
            list.path.as_ref().map(TrimmedText::as_str),
            Some("calendar/team.ics")
        );

        let delete = CalendarDeleteCommand::try_from(CalendarDeleteInput {
            uid: Some(" event-1 ".to_string()),
            path: None,
        })
        .expect("delete command");
        assert_eq!(delete.uid.as_str(), "event-1");
    }

    #[test]
    fn calendar_import_command_rejects_blank_ics() {
        let error = CalendarImportCommand::try_from(CalendarImportInput {
            ics: Some(" \n\t".to_string()),
            path: None,
            calendar_path: None,
        })
        .expect_err("blank ics should fail");
        assert!(error.to_string().contains("ics is required"));
    }
}
