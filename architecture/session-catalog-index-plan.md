# Session Catalog Index Plan

Enterprise-scale session listing for Medousa Engine. The app stays human (sidebar labels, previews); the engine stores a read-optimized index instead of recomputing summaries from full transcripts.

## Problem

`GET /v1/sessions` returns metadata (`SessionHistorySummary`), but building each row today:

- loads **full session history** (every turn, content, parts) to derive a 72-char preview
- scans the **entire verification index** twice per session
- may read verification JSON files from disk

For `limit=50`, that is 50+ transcript loads and 100+ index scans per request. SurrealDB and Rust are not the bottleneck — the algorithm is.

## What the endpoint represents

| Endpoint | Role |
|----------|------|
| `GET /v1/sessions` | **Session index** — browse/resume catalog (inbox list) |
| `GET /v1/sessions/{id}/history` | **Transcript** — full turns for one session |
| Channel `/history` (ingest) | **Scoped recent list** — per mapping key, not global index |

Consumers:

- **Medousa Home** — session sidebar, Context panel labels (`limit=50`)
- **TUI** — `/history` overlay with verification trust column (`limit=200`)
- **Telegram / Discord** — channel-scoped list (internal store); name resume uses resolver, not this HTTP route

The index row must be servable in **one indexed query** at any scale.

## Canonical index row (`session_catalog`)

| Field | Purpose | Updated when |
|-------|---------|--------------|
| `session_id` | Primary key | first turn / named session |
| `preview` | ≤72 char label fallback | `append_turn` (if turn has content) |
| `turn_count` | Sidebar / TUI count | `append_turn` |
| `last_activity_at` | Sort + relative time | `append_turn` |
| `display_name` | Human label (denormalized) | `set_session_display_name` |
| `verification_run_count` | TUI trust column | `persist_verification` |
| `last_verification_*` | TUI detail pane | `persist_verification` |

Rule: **list path never calls `load_history` or `read_all()` on verification index.**

## Phases

### Phase 1 — Catalog table + write hooks (this doc)

- [x] `session_catalog` module (file + Surreal backends)
- [x] Upsert on `append_turn`, `set_session_display_name`, `persist_verification`
- [x] `list_history_sessions` reads catalog only
- [x] Startup backfill when catalog empty but legacy data exists
- [x] Tests for Surreal list ordering and append hooks

### Phase 2 — Adjacent slow paths

- [x] `resolve_history_resume_target` — prefix/name lookup via catalog/meta, not `list_history_sessions(500)`
- [x] `format_channel_session_history` — turn counts from catalog, not `load_history().len()`
- [x] Optional `GET /v1/sessions?include_verification=false` (strip trust fields for lighter payloads)

### Phase 3 — Client polish

- [x] Cache session list in Home with stale-while-revalidate (`SESSIONS_STALE_MS`, in-flight coalescing)
- [x] Parallel `refreshSessions` + `ensureSessionHydrated` on connect
- [x] Debounce post-turn refresh calls (`scheduleSessionsRefresh`)
- [x] Home passes `include_verification=false` on session list

### Phase 4 — Naming + search at scale (next)

- [x] Home rename UI (`PUT /v1/sessions/{id}/name`) — pencil in session sidebar
- [x] Server-side `GET /v1/sessions?q=&cursor=` — catalog search + pagination past client `limit=50`
- [x] Auto-title from first user turn when preview is empty (write hook on `append_turn`)
- [ ] Case-insensitive display-name search in meta store (catalog already has lower lookup)

## Success criteria

| Check | Target |
|-------|--------|
| `curl /v1/sessions?limit=50` | Sub-100ms typical; single list query on Surreal |
| Query pattern | O(limit) index read, not O(limit × transcript size) |
| TUI `/history` | Trust columns unchanged (from catalog fields) |
| Backfill | One-time on upgrade; no user action |

## Implementation notes

- Surreal: `UPSERT session_catalog:{session_id}`; index on `last_activity_at DESC`
- File: `~/.local/share/medousa/catalog/{session_id}.json` — small rows, directory list for index
- Backfill: one GROUP BY on `session_turn`; verification index read **once** grouped by `session_id`
- `enrich_session_summaries` kept for display names not yet denormalized on old rows

## Related

- [component-daemon.md](component-daemon.md) — HTTP surface
- [medousa-home-tauri-design.md](medousa-home-tauri-design.md) — app session sidebar
- [tui-performance-target-plan.md](tui-performance-target-plan.md) — TUI responsiveness
