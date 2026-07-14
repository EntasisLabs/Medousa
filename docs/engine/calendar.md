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

---

## Agent tools

| Tool | Purpose |
|------|---------|
| `cognition_calendar_list` | List events in a time range (RRULE expanded) |
| `cognition_calendar_create` | Create event |
| `cognition_calendar_update` | Update by `uid` |
| `cognition_calendar_delete` | Delete by `uid` |
| `cognition_calendar_import` | Import raw ICS body |
| `cognition_calendar_export` | Export ICS text |

Discover domain **calendar**. Host auto-unlocks the domain (bootstrap peek: `cognition_calendar_list`). Research/General workers get the full set.

Source: `src/calendar_tools.rs`, service: `src/calendar/service.rs`. See also [agent-tools.md](agent-tools.md).
