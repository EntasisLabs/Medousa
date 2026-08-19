# Calendar

**Audience:** integrator

Personal calendar events are stored as RFC 5545 `.ics` files in the vault (default `calendar/personal.ics`). There is no Surreal table for events.

---

## Store

| Path | Purpose |
|------|---------|
| `calendar/personal.ics` | Default personal calendar |
| Other vault-relative `*.ics` | Optional alternate calendars via `path` / `calendar_path` |

MIME: `.ics` → `text/calendar` in the vault service.

---

## HTTP API

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/v1/calendar/events` | List events (`from`, `to`, optional `path`) |
| POST | `/v1/calendar/events` | Create event |
| PUT | `/v1/calendar/events/{uid}` | Update event |
| DELETE | `/v1/calendar/events/{uid}` | Delete event |
| POST | `/v1/calendar/import` | Merge VEVENTs from raw ICS |
| GET | `/v1/calendar/export` | Export ICS text |

SDK: `client().calendar()` — list, create, update, delete, `import_ics`, export. Home UI must use the typed SDK, not raw paths.

All-day contract: calendar-date UTC midnights (`YYYY-MM-DDT00:00:00Z`) with `all_day: true`; exclusive `dtend` for multi-day spans.

Optional fields on create/update/list:

| Field | ICS | Purpose |
|-------|-----|---------|
| `note_path` | `X-MEDOUSA-NOTE` | Vault-relative markdown note for rich body/attachments |
| `alarms[]` | `VALARM` (`TRIGGER:-PTnM`) | Display alerts: `trigger_minutes_before` before `dtstart` |
| `rrule` | `RRULE` | Recurrence (expanded on list) |

Home overlays vault reminders from `calendar/reminders.md` (`- [ ] … @due(YYYY-MM-DD)`); those are not VEVENT/VTODO.

---

## Agent tools

| Tool | Purpose |
|------|---------|
| `cognition_calendar_query` `action=calendar.list` | List events in a time range (RRULE expanded) |
| `cognition_calendar_query` `action=calendar.export` | Export ICS text |
| `cognition_calendar_mutate` `action=calendar.create` | Create event |
| `cognition_calendar_mutate` `action=calendar.update` | Update by `uid` |
| `cognition_calendar_mutate` `action=calendar.delete` | Delete by `uid` |
| `cognition_calendar_mutate` `action=calendar.import` | Import raw ICS body |

Public primitives; fetch field schemas with `cognition_schema` `domain=calendar`. Default store: `calendar/personal.ics`.

Source: `src/calendar_tools.rs`, service: `src/calendar/service.rs`. See also [agent-tools.md](agent-tools.md).
