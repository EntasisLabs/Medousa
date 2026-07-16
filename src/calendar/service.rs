//! CalendarService — vault `.ics` as source of truth.

use std::fs;
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};
use icalendar::{Calendar, CalendarComponent, CalendarDateTime, Component, DatePerhapsTime, Event, EventLike};
use medousa_types::{
    CalendarDeleteResponse, CalendarEvent, CalendarExportResponse, CalendarImportRequest,
    CalendarImportResponse, CalendarListResponse, CalendarWriteRequest, CalendarWriteResponse,
};
use uuid::Uuid;

use crate::vault::path::{normalize_vault_path, resolve_user_note_path};

pub const DEFAULT_CALENDAR_PATH: &str = "calendar/personal.ics";

const EMPTY_CALENDAR: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Medousa//Calendar//EN\r\nCALSCALE:GREGORIAN\r\nEND:VCALENDAR\r\n";

pub struct CalendarService;

impl CalendarService {
    pub fn resolve_path(path: Option<&str>) -> Result<String> {
        let raw = path
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_CALENDAR_PATH);
        let normalized = normalize_vault_path(raw)?;
        if !normalized.ends_with(".ics") {
            bail!("calendar path must end with .ics");
        }
        if !normalized.starts_with("calendar/") {
            bail!("calendar path must live under calendar/");
        }
        Ok(normalized)
    }

    pub fn list_events(
        path: Option<&str>,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
    ) -> Result<CalendarListResponse> {
        let calendar_path = Self::resolve_path(path)?;
        let cal = Self::load_or_create(&calendar_path)?;
        let from = from.unwrap_or_else(|| Utc::now() - Duration::days(31));
        let to = to.unwrap_or_else(|| Utc::now() + Duration::days(62));
        if to <= from {
            bail!("`to` must be after `from`");
        }

        let mut events = Vec::new();
        for component in cal.components.iter() {
            let CalendarComponent::Event(event) = component else {
                continue;
            };
            let Some(base) = event_to_dto(event, &calendar_path) else {
                continue;
            };
            events.extend(expand_occurrences(&base, from, to));
        }
        events.sort_by(|a, b| a.dtstart.cmp(&b.dtstart).then(a.uid.cmp(&b.uid)));
        Ok(CalendarListResponse {
            calendar_path,
            events,
        })
    }

    pub fn create_event(request: &CalendarWriteRequest) -> Result<CalendarWriteResponse> {
        let calendar_path = Self::resolve_path(request.calendar_path.as_deref())?;
        let mut cal = Self::load_or_create(&calendar_path)?;
        let uid = request
            .uid
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}@medousa", Uuid::new_v4()));

        if cal.components.iter().any(|component| {
            matches!(component, CalendarComponent::Event(event) if event.get_uid() == Some(uid.as_str()))
        }) {
            bail!("event already exists: {uid}");
        }

        let event = build_event(request, &uid)?;
        let dto = event_to_dto(&event, &calendar_path).context("built event missing start")?;
        cal.push(event);
        Self::save(&calendar_path, &cal)?;
        Ok(CalendarWriteResponse {
            event: dto,
            created: true,
        })
    }

    pub fn update_event(uid: &str, request: &CalendarWriteRequest) -> Result<CalendarWriteResponse> {
        let calendar_path = Self::resolve_path(request.calendar_path.as_deref())?;
        let uid = uid.trim();
        if uid.is_empty() {
            bail!("uid is required");
        }
        let mut cal = Self::load_or_create(&calendar_path)?;
        let idx = cal.components.iter().position(|component| {
            matches!(component, CalendarComponent::Event(event) if event.get_uid() == Some(uid))
        });
        let Some(idx) = idx else {
            bail!("event not found: {uid}");
        };
        let event = build_event(request, uid)?;
        let dto = event_to_dto(&event, &calendar_path).context("built event missing start")?;
        cal.components[idx] = CalendarComponent::Event(event);
        Self::save(&calendar_path, &cal)?;
        Ok(CalendarWriteResponse {
            event: dto,
            created: false,
        })
    }

    pub fn delete_event(uid: &str, path: Option<&str>) -> Result<CalendarDeleteResponse> {
        let calendar_path = Self::resolve_path(path)?;
        let uid = uid.trim();
        if uid.is_empty() {
            bail!("uid is required");
        }
        let mut cal = Self::load_or_create(&calendar_path)?;
        let before = cal.components.len();
        cal.components.retain(|component| {
            !matches!(component, CalendarComponent::Event(event) if event.get_uid() == Some(uid))
        });
        let deleted = cal.components.len() < before;
        if !deleted {
            bail!("event not found: {uid}");
        }
        Self::save(&calendar_path, &cal)?;
        Ok(CalendarDeleteResponse {
            uid: uid.to_string(),
            deleted: true,
            calendar_path,
        })
    }

    pub fn import(request: &CalendarImportRequest) -> Result<CalendarImportResponse> {
        let calendar_path = Self::resolve_path(request.calendar_path.as_deref())?;
        let incoming: Calendar = request
            .ics
            .parse()
            .map_err(|err| anyhow::anyhow!("invalid ics: {err}"))?;
        let mut cal = Self::load_or_create(&calendar_path)?;
        let mut imported = 0usize;
        let mut updated = 0usize;
        let mut skipped = 0usize;
        let mut warnings = Vec::new();

        for component in incoming.components {
            let CalendarComponent::Event(mut event) = component else {
                skipped += 1;
                let kind = match &component {
                    CalendarComponent::Todo(_) => "VTODO",
                    CalendarComponent::Venue(_) => "VVENUE",
                    CalendarComponent::Other(_) => "other",
                    CalendarComponent::Event(_) => unreachable!(),
                    _ => "unknown",
                };
                warnings.push(format!("skipped non-event component ({kind})"));
                continue;
            };
            let uid = match event
                .get_uid()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                Some(uid) => uid.to_string(),
                None => {
                    let uid = format!("{}@medousa", Uuid::new_v4());
                    event.uid(&uid);
                    uid
                }
            };
            if let Some(idx) = cal.components.iter().position(|existing| {
                matches!(existing, CalendarComponent::Event(e) if e.get_uid() == Some(uid.as_str()))
            }) {
                cal.components[idx] = CalendarComponent::Event(event);
                updated += 1;
            } else {
                cal.push(event);
                imported += 1;
            }
        }
        Self::save(&calendar_path, &cal)?;
        Ok(CalendarImportResponse {
            calendar_path,
            imported,
            updated,
            skipped,
            warnings,
        })
    }

    pub fn export(path: Option<&str>) -> Result<CalendarExportResponse> {
        let calendar_path = Self::resolve_path(path)?;
        let _ = Self::load_or_create(&calendar_path)?;
        let absolute = resolve_user_note_path(&calendar_path)?;
        let ics = fs::read_to_string(&absolute)
            .with_context(|| format!("read calendar {}", absolute.display()))?;
        Ok(CalendarExportResponse {
            calendar_path,
            content_type: "text/calendar".to_string(),
            ics,
        })
    }

    fn load_or_create(calendar_path: &str) -> Result<Calendar> {
        let absolute = resolve_user_note_path(calendar_path)?;
        if !absolute.exists() {
            if let Some(parent) = absolute.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create calendar dir {}", parent.display()))?;
            }
            fs::write(&absolute, EMPTY_CALENDAR)
                .with_context(|| format!("create calendar {}", absolute.display()))?;
        }
        let text = fs::read_to_string(&absolute)
            .with_context(|| format!("read calendar {}", absolute.display()))?;
        text.parse()
            .map_err(|err| anyhow::anyhow!("parse calendar {}: {err}", absolute.display()))
    }

    fn save(calendar_path: &str, cal: &Calendar) -> Result<()> {
        let absolute = resolve_user_note_path(calendar_path)?;
        if let Some(parent) = absolute.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create calendar dir {}", parent.display()))?;
        }
        let serialized = cal.to_string();
        fs::write(&absolute, serialized)
            .with_context(|| format!("write calendar {}", absolute.display()))?;
        Ok(())
    }
}

fn build_event(request: &CalendarWriteRequest, uid: &str) -> Result<Event> {
    let summary = request.summary.trim();
    if summary.is_empty() {
        bail!("summary is required");
    }
    let mut event = Event::new();
    event.uid(uid);
    event.summary(summary);
    if let Some(description) = request
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        event.description(description);
    }
    if let Some(location) = request
        .location
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        event.location(location);
    }

    if request.all_day {
        let start_date = request.dtstart.date_naive();
        event.all_day(start_date);
        if let Some(end) = request.dtend {
            let end_date = end.date_naive();
            if end_date > start_date {
                event.ends(DatePerhapsTime::Date(end_date));
            }
        }
    } else {
        event.starts(request.dtstart);
        if let Some(end) = request.dtend {
            if end <= request.dtstart {
                bail!("dtend must be after dtstart");
            }
            event.ends(end);
        } else {
            event.ends(request.dtstart + Duration::hours(1));
        }
    }

    if let Some(rrule) = request
        .rrule
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let value = rrule
            .strip_prefix("RRULE:")
            .unwrap_or(rrule)
            .trim()
            .to_string();
        event.add_property("RRULE", &value);
    }

    Ok(event.done())
}

fn event_to_dto(event: &Event, calendar_path: &str) -> Option<CalendarEvent> {
    let uid = event.get_uid()?.to_string();
    let summary = event
        .get_summary()
        .unwrap_or("(untitled)")
        .to_string();
    let description = event.get_description().map(str::to_string);
    let location = event.get_location().map(str::to_string);
    let (dtstart, all_day) = start_to_utc(event.get_start()?)?;
    let dtend = event.get_end().and_then(end_to_utc);
    let rrule = event.property_value("RRULE").map(str::to_string);

    Some(CalendarEvent {
        uid,
        summary,
        description,
        location,
        dtstart,
        dtend,
        all_day,
        rrule,
        calendar_path: calendar_path.to_string(),
        recurrence_id: None,
    })
}

fn start_to_utc(value: DatePerhapsTime) -> Option<(DateTime<Utc>, bool)> {
    match value {
        DatePerhapsTime::Date(date) => {
            let dt = date
                .and_time(NaiveTime::MIN)
                .and_utc();
            Some((dt, true))
        }
        DatePerhapsTime::DateTime(dt) => Some((calendar_dt_to_utc(dt)?, false)),
    }
}

fn end_to_utc(value: DatePerhapsTime) -> Option<DateTime<Utc>> {
    match value {
        DatePerhapsTime::Date(date) => Some(date.and_time(NaiveTime::MIN).and_utc()),
        DatePerhapsTime::DateTime(dt) => calendar_dt_to_utc(dt),
    }
}

/// Map common Windows/Outlook TZIDs to IANA names when chrono_tz cannot parse them.
fn windows_tzid_to_iana(tzid: &str) -> Option<&'static str> {
    match tzid.trim() {
        "Pacific Standard Time" => Some("America/Los_Angeles"),
        "Eastern Standard Time" => Some("America/New_York"),
        "Central Standard Time" => Some("America/Chicago"),
        "Mountain Standard Time" => Some("America/Denver"),
        "GMT Standard Time" => Some("Europe/London"),
        "UTC" | "Coordinated Universal Time" => Some("Etc/UTC"),
        _ => None,
    }
}

fn resolve_tz(tzid: &str) -> Option<chrono_tz::Tz> {
    chrono_tz::Tz::from_str(tzid)
        .ok()
        .or_else(|| windows_tzid_to_iana(tzid).and_then(|iana| chrono_tz::Tz::from_str(iana).ok()))
}

fn calendar_dt_to_utc(value: CalendarDateTime) -> Option<DateTime<Utc>> {
    match value {
        CalendarDateTime::Floating(naive) => Some(Utc.from_utc_datetime(&naive)),
        CalendarDateTime::Utc(dt) => Some(dt),
        CalendarDateTime::WithTimezone { date_time, tzid } => {
            if let Some(tz) = resolve_tz(&tzid) {
                return tz
                    .from_local_datetime(&date_time)
                    .single()
                    .map(|dt| dt.with_timezone(&Utc))
                    .or_else(|| Some(Utc.from_utc_datetime(&date_time)));
            }
            // Unknown TZID — treat as floating/UTC rather than dropping the event.
            Some(Utc.from_utc_datetime(&date_time))
        }
    }
}

fn expand_occurrences(
    base: &CalendarEvent,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Vec<CalendarEvent> {
    let Some(rrule) = base.rrule.as_deref() else {
        if occurrence_overlaps(base.dtstart, base.dtend, from, to) {
            return vec![base.clone()];
        }
        return Vec::new();
    };

    let freq = parse_freq(rrule);
    let interval = parse_interval(rrule).unwrap_or(1).max(1);
    let until = parse_until(rrule);
    let count = parse_count(rrule);
    let byday = parse_byday(rrule);

    let mut out = Vec::new();
    let duration = base
        .dtend
        .map(|end| end - base.dtstart)
        .unwrap_or_else(|| {
            if base.all_day {
                Duration::days(1)
            } else {
                Duration::hours(1)
            }
        });

    let mut cursor = base.dtstart;
    let mut emitted = 0usize;
    let mut guard = 0usize;
    while guard < 10_000 {
        guard += 1;
        if cursor >= to {
            break;
        }
        if let Some(until) = until
            && cursor > until
        {
            break;
        }
        if let Some(count) = count
            && emitted >= count
        {
            break;
        }

        let weekday_ok = byday.is_empty() || byday.contains(&cursor.weekday());
        if weekday_ok && occurrence_overlaps(cursor, Some(cursor + duration), from, to) {
            let mut instance = base.clone();
            instance.dtstart = cursor;
            instance.dtend = Some(cursor + duration);
            instance.recurrence_id = Some(cursor);
            out.push(instance);
            emitted += 1;
        }

        cursor = match freq.as_deref() {
            Some("DAILY") => cursor + Duration::days(interval as i64),
            Some("WEEKLY") => cursor + Duration::weeks(interval as i64),
            Some("MONTHLY") => add_months(cursor, interval as i32),
            Some("YEARLY") => add_months(cursor, (interval as i32) * 12),
            _ => {
                // Unsupported FREQ: include master if it overlaps.
                if occurrence_overlaps(base.dtstart, base.dtend, from, to) {
                    return vec![base.clone()];
                }
                return out;
            }
        };
    }
    out
}

fn occurrence_overlaps(
    start: DateTime<Utc>,
    end: Option<DateTime<Utc>>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> bool {
    let end = end.unwrap_or(start + Duration::hours(1));
    start < to && end > from
}

fn parse_freq(rrule: &str) -> Option<String> {
    rrule
        .split(';')
        .find_map(|part| part.strip_prefix("FREQ=").map(str::to_ascii_uppercase))
}

fn parse_interval(rrule: &str) -> Option<u32> {
    rrule
        .split(';')
        .find_map(|part| part.strip_prefix("INTERVAL=")?.parse().ok())
}

fn parse_count(rrule: &str) -> Option<usize> {
    rrule
        .split(';')
        .find_map(|part| part.strip_prefix("COUNT=")?.parse().ok())
}

fn parse_until(rrule: &str) -> Option<DateTime<Utc>> {
    let raw = rrule
        .split(';')
        .find_map(|part| part.strip_prefix("UNTIL="))?;
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Some(dt.with_timezone(&Utc));
    }
    if raw.len() == 8 {
        let date = NaiveDate::parse_from_str(raw, "%Y%m%d").ok()?;
        return Some(date.and_time(NaiveTime::from_hms_opt(23, 59, 59)?).and_utc());
    }
    if raw.ends_with('Z') && raw.len() >= 16 {
        let naive = chrono::NaiveDateTime::parse_from_str(raw.trim_end_matches('Z'), "%Y%m%dT%H%M%S")
            .ok()?;
        return Some(Utc.from_utc_datetime(&naive));
    }
    None
}

fn parse_byday(rrule: &str) -> Vec<Weekday> {
    let Some(raw) = rrule.split(';').find_map(|part| part.strip_prefix("BYDAY=")) else {
        return Vec::new();
    };
    raw.split(',')
        .filter_map(|token| {
            let day = token
                .chars()
                .rev()
                .take(2)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>();
            match day.as_str() {
                "MO" => Some(Weekday::Mon),
                "TU" => Some(Weekday::Tue),
                "WE" => Some(Weekday::Wed),
                "TH" => Some(Weekday::Thu),
                "FR" => Some(Weekday::Fri),
                "SA" => Some(Weekday::Sat),
                "SU" => Some(Weekday::Sun),
                _ => None,
            }
        })
        .collect()
}

fn add_months(dt: DateTime<Utc>, months: i32) -> DateTime<Utc> {
    let date = dt.date_naive();
    let year = date.year();
    let month = date.month() as i32 + months;
    let year_adj = year + (month - 1).div_euclid(12);
    let month_adj = ((month - 1).rem_euclid(12) + 1) as u32;
    let day = date.day().min(days_in_month(year_adj, month_adj));
    let naive_date = NaiveDate::from_ymd_opt(year_adj, month_adj, day).unwrap_or(date);
    naive_date
        .and_time(dt.time())
        .and_utc()
}

fn days_in_month(year: i32, month: u32) -> u32 {
    NaiveDate::from_ymd_opt(year, month + 1, 1)
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(year + 1, 1, 1).expect("valid date"))
        .pred_opt()
        .map(|d| d.day())
        .unwrap_or(28)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::service::vault_integration_test_lock;
    use std::sync::MutexGuard;

    fn with_temp_vault<T>(f: impl FnOnce() -> T) -> T {
        let _lock: MutexGuard<'_, ()> = vault_integration_test_lock();
        let base = std::env::temp_dir().join(format!("medousa-cal-{}", Uuid::new_v4().simple()));
        let vault = base.join("vault");
        fs::create_dir_all(&vault).expect("vault");
        // Safety: test-only env override for vault root path resolution via MEDOUSA_DATA_DIR.
        let data = base.join("data");
        fs::create_dir_all(data.join("vault")).ok();
        // Use vault roots via env if available; otherwise write under resolved path.
        let previous = std::env::var("MEDOUSA_DATA_DIR").ok();
        unsafe {
            std::env::set_var("MEDOUSA_DATA_DIR", &data);
        }
        // Ensure active vault points at our temp vault by writing into user_vault_root.
        // paths::user_vault_root uses MEDOUSA_DATA_DIR/vault
        let result = f();
        match previous {
            Some(value) => unsafe { std::env::set_var("MEDOUSA_DATA_DIR", value) },
            None => unsafe { std::env::remove_var("MEDOUSA_DATA_DIR") },
        }
        let _ = fs::remove_dir_all(base);
        result
    }

    #[test]
    fn create_list_delete_roundtrip() {
        with_temp_vault(|| {
            let start = Utc::now() + Duration::hours(2);
            let created = CalendarService::create_event(&CalendarWriteRequest {
                uid: None,
                summary: "Standup".into(),
                description: Some("daily".into()),
                location: None,
                dtstart: start,
                dtend: Some(start + Duration::minutes(30)),
                all_day: false,
                rrule: None,
                calendar_path: None,
            })
            .expect("create");
            assert!(created.created);
            assert_eq!(created.event.summary, "Standup");

            let listed = CalendarService::list_events(
                None,
                Some(start - Duration::hours(1)),
                Some(start + Duration::hours(2)),
            )
            .expect("list");
            assert_eq!(listed.events.len(), 1);

            let exported = CalendarService::export(None).expect("export");
            assert!(exported.ics.contains("Standup"));

            CalendarService::delete_event(&created.event.uid, None).expect("delete");
            let listed = CalendarService::list_events(
                None,
                Some(start - Duration::hours(1)),
                Some(start + Duration::hours(2)),
            )
            .expect("list empty");
            assert!(listed.events.is_empty());
        });
    }

    #[test]
    fn import_missing_uid_and_windows_tzid() {
        with_temp_vault(|| {
            let ics = [
                "BEGIN:VCALENDAR",
                "VERSION:2.0",
                "PRODID:-//Test//EN",
                "BEGIN:VEVENT",
                "SUMMARY:Outlook lunch",
                "DTSTART;TZID=Pacific Standard Time:20260715T120000",
                "DTEND;TZID=Pacific Standard Time:20260715T130000",
                "END:VEVENT",
                "BEGIN:VTODO",
                "SUMMARY:Skipped todo",
                "END:VTODO",
                "END:VCALENDAR",
            ]
            .join("\r\n");

            let result = CalendarService::import(&CalendarImportRequest {
                ics,
                calendar_path: None,
            })
            .expect("import");
            assert_eq!(result.imported, 1);
            assert_eq!(result.updated, 0);
            assert_eq!(result.skipped, 1);
            assert!(
                result
                    .warnings
                    .iter()
                    .any(|warning| warning.contains("VTODO")),
                "expected VTODO skip warning, got {:?}",
                result.warnings
            );

            let from = Utc.with_ymd_and_hms(2026, 7, 15, 0, 0, 0).unwrap();
            let to = Utc.with_ymd_and_hms(2026, 7, 16, 0, 0, 0).unwrap();
            let listed = CalendarService::list_events(None, Some(from), Some(to)).expect("list");
            let event = listed
                .events
                .iter()
                .find(|event| event.summary == "Outlook lunch")
                .expect("imported Outlook lunch event");
            assert!(!event.uid.is_empty(), "missing UID should be generated");
            // Pacific Daylight Time in July is UTC-7 → 12:00 local = 19:00 UTC.
            assert_eq!(
                event.dtstart,
                Utc.with_ymd_and_hms(2026, 7, 15, 19, 0, 0).unwrap()
            );
        });
    }
}
