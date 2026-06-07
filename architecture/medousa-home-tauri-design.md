# Medousa Home — Tauri UI Design

> **Status:** M2.5 polish — Tauri v2 + SvelteKit + Skeleton UI v2  
> **Stack (locked):** Tauri 2 · SvelteKit 2 · Svelte 5 · Skeleton 2.x · daemon HTTP/SSE only

---

## Principles

1. **`medousa_daemon` is the only source of truth.** The Tauri shell is a thin adapter: HTTP POST + SSE subscribe. No workspace/vault business logic in the UI crate.
2. **Locus stays out of v1 Home.** Vault/workspace reads flow through frozen daemon APIs. Locus bridge writes remain debounced server-side (V2); no Locus editor in the main surface until a dedicated drawer milestone.
3. **Activity ≠ Locus.** The right panel shows `WorkspaceEvent` from `feed_appended` — not a Locus graph dump.
4. **Workspace-first, not config-first.** Medousa Home foregrounds live work, vault, and activity — unlike Hermes (agent catalog) or Cursor (in-repo IDE). Chat is the default bench; Library and Work board are first-class peers.
5. **Borrow layout DNA from three references** — see [Design references](#design-references) below.

---

## Design references

| Reference | Steal | Skip |
|-----------|-------|------|
| **Codex** | Center chat thread, bottom work rail, rounded composer with permission/model chips, optional review pane | Full three-pane diff editor (we are not a repo IDE) |
| **Hermes** | Labeled left nav, session list + search + new chat, branded empty state, settings gear | Skills-first homepage (we surface Work + Library instead) |
| **Cursor** | Thin status strip, inline change awareness on vault/card actions | Code editor center, git panel as primary nav |

**Medousa differentiation:** lives inside the running daemon workspace — vault wikilinks, kanban, SSE activity, card cancel/retry — none of the three references combine these in one shell.

---

## Stack

| Layer | Choice | Notes |
|-------|--------|-------|
| Shell | Tauri 2 | Native window, tray (M3), Rust-side HTTP/SSE |
| UI framework | SvelteKit 2 (SPA) | `adapter-static`, `ssr = false` — [Tauri SvelteKit guide](https://v2.tauri.app/start/frontend/sveltekit/) |
| Components | Skeleton UI v2 | `@skeletonlabs/skeleton` + `@skeletonlabs/tw-plugin`, Tailwind 3 |
| Daemon URL | `MEDOUSA_DAEMON_URL` | Default `http://127.0.0.1:7419` |

**Location:** `apps/medousa-home/`

---

## The Workshop — layout

```text
┌────┬────────┬────────────────────────────────────┬──────────┐
│Nav │Sessions│  Primary surface (one at a time)    │ Activity │
│    │(chat)  │  · Chat (default)                  │ feed SSE │
│    │        │  · Vault prose editor              │ + context│
│    │        │  · Kanban / card inspector         │          │
│    │        │  · Settings                        │          │
├────┴────────┴────────────────────────────────────┴──────────┤
│ Work rail — in-motion cards only (backlog/in_flight/wrap)   │
└─────────────────────────────────────────────────────────────┘
Nav = labeled sidebar (~176px): Home · Chat · Library · Work · Settings
Sessions = Hermes-style list when Chat active (~224px)
```

### Navigation (labeled sidebar)

| Item | Surface | Shipped |
|------|---------|---------|
| Home | Column overview + jump to work | M2 |
| Chat | Interactive turn SSE + session sidebar | M0 + M2.5 sessions |
| Library | Vault tree + editor | M1 |
| Work | Kanban + card inspector | M2 |
| Skills | Manuscript + capability catalog | M2.5 |
| Settings | Daemon URL, theme, notifications | M2.5 |

### Default landing

**Chat** on launch (Codex-style command center). Last vault note restore deferred to M1.

### Work rail placement

**Bottom horizontal rail** (Codex-style). Shows **in-motion cards only** (`backlog`, `in_flight`, `wrapping_up`) — terminal `blocked`/`done` cards stay on the kanban, not the rail.

### Chat sessions (Hermes-style)

When Chat is active, a secondary column lists daemon session history (`GET /v1/sessions`), supports search, new chat, and resume via `GET /v1/sessions/{id}/history`.

---

## Daemon route map (client)

All calls originate in `src-tauri/`; Svelte invokes commands and listens for events.

### M0 — wired

| UI need | Method | Route | Transport |
|---------|--------|-------|-----------|
| Health | GET | `/health` | HTTP |
| Workspace snapshot + live | GET | `/v1/workspace/stream?since_revision=` | SSE → `workspace://event` |
| Send message | POST | `/v1/interactive/turn` | HTTP |
| Stream reply | GET | `/v1/interactive/turn/{id}/stream` | SSE → `interactive://event` |
| List sessions | GET | `/v1/sessions?limit=` | HTTP |
| Session history | GET | `/v1/sessions/{id}/history` | HTTP |
| Manuscript catalog | GET | `/v1/manuscripts?limit=&skills_only=` | HTTP |
| Capability catalog | GET | `/v1/capabilities` | HTTP |

### M1 — vault

| UI need | Route |
|---------|-------|
| List notes | `GET /v1/vault/notes` |
| Read / write | `GET/PUT /v1/vault/notes/{path}` |
| Search | `GET /v1/vault/search?q=` |
| Backlinks | `GET /v1/vault/backlinks?path=` |
| Card links | `POST /v1/workspace/cards/{id}/link-vault` |

### M2 — work actions

| Action | Route |
|--------|-------|
| Card detail | `GET /v1/workspace/cards/{id}` |
| Cancel | `POST /v1/workspace/cards/{id}/cancel` |
| Retry | `POST /v1/workspace/cards/{id}/retry` |
| Filtered feed | `GET /v1/workspace/feed?card_id=` |

### Explicitly not in Home v1

- Direct Locus MCP / store writes from the webview
- Job queue mutation beyond card cancel/retry
- Stasis `/dashboard` replacement

---

## Tauri ↔ Svelte contract

### Commands (invoke)

| Command | Args | Returns |
|---------|------|---------|
| `daemon_url` | — | Current base URL |
| `set_daemon_url` | `{ url }` | — |
| `daemon_health` | — | `{ ok, message }` |
| `workspace_stream_start` | `{ since_revision? }` | — |
| `workspace_stream_stop` | — | — |
| `interactive_turn_send` | `{ session_id, prompt }` | `{ turn_id, stream_url }` |
| `interactive_stream_stop` | — | — |
| `vault_list_notes` | `{ prefix?, limit? }` | `VaultNotesListResponse` |
| `vault_get_note` | `{ path }` | `VaultNoteContentResponse` |
| `vault_save_note` | `{ path, content, contentHash? }` | `VaultWriteResponse` |
| `vault_search` | `{ query, limit? }` | `VaultSearchResponse` |
| `vault_backlinks` | `{ path }` | `VaultBacklinksResponse` |
| `workspace_get_card` | `{ cardId }` | `WorkCardDetail` |
| `workspace_cancel_card` | `{ cardId }` | `WorkspaceCardActionResponse` |
| `workspace_retry_card` | `{ cardId }` | `WorkspaceCardActionResponse` |
| `session_list` | `{ limit? }` | `{ sessions }` |
| `session_get_history` | `{ sessionId }` | `{ session_id, turns }` |
| `catalog_list_manuscripts` | `{ prefix?, limit?, skillsOnly? }` | `ManuscriptCatalogResponse` |
| `catalog_list_capabilities` | — | `CapabilityListResponse` |

### Events (listen)

| Event | Payload | When |
|-------|---------|------|
| `workspace://event` | `WorkspaceStreamEvent` JSON | SSE frame |
| `workspace://error` | `{ message }` | Stream failure |
| `interactive://event` | `InteractiveTurnStreamEvent` JSON | Turn SSE |
| `interactive://error` | `{ message }` | Turn failure |

### Session identity

- Default session: `medousa-home` (persisted in `localStorage` on first launch).
- Surface tag on turns: `channel_surface: "home"`.

---

## Milestones

### M0 — shell

- [x] Tauri + SvelteKit + Skeleton scaffold
- [x] Workshop chrome (icon rail, chat, work rail, activity)
- [x] Workspace SSE → work rail + activity panel
- [x] Interactive turn POST + SSE → chat panel
- [ ] 2-week dogfood on frozen APIs (gate from plan)

### M1 — library

- [x] Vault tree + search (`GET /v1/vault/notes`, `/search`)
- [x] Prose editor with raw/preview toggle + `PUT` save (`If-Match: content_hash`)
- [x] Backlinks + wikilinks in right context panel
- [x] Work card click → `GET /v1/workspace/cards/{id}` → linked vault paths
- [x] Last-opened note restored from `localStorage`
- [ ] Vault create/delete UI (CLI OK for now)

### M2 — full home (current)

- [x] Work tab — kanban columns (backlog → done) with live stream counts
- [x] Swimlanes: intent, manuscript, job family, session (detail cache prefetch)
- [x] WrappingUp pulse emphasis on board + inspector
- [x] Card inspector — cancel, retry, ask Medousa, vault links, result excerpt
- [x] Native notification when card transitions to `done`
- [x] Home overview — column counts + jump to work board
- [ ] Telegram card summary via outbox (deferred)

### M2.5 — UX polish (current)

- [x] Labeled nav sidebar (Hermes-style)
- [x] Chat session list + resume history
- [x] Codex-style composer (rounded input, surface chips, Enter-to-send)
- [x] Settings surface — daemon URL, dark mode, notification toggle
- [x] Work rail filters to in-motion cards only
- [x] Skills & Tools catalog (read-only, Hermes parity)
- [x] Status strip — daemon health + workspace revision (Cursor-style)
- [x] Session pinning (star toggle in session sidebar)

### M3 — polish (current)

- [x] Resizable split panes — Activity panel + vault tree (persisted widths)
- [x] System tray — show/hide/quit + open chat; close hides to tray
- [x] Pop-out chat window (`chat-popout` label, `/popout/chat` route)
- [x] Drag-to-cancel — drop zone on work board (no column reorder)
- [x] Vault diff chips — `+N -M` line stats in editor + context panel
- [x] Split primary + inspector side-by-side on Work tab
- [x] Tray badge for blocked card count (tooltip + Linux title + taskbar badge)

---

## Theme

Skeleton preset **sahara** (`data-theme="sahara"` on `<body>`). Dark mode via `class` on `<html>`; operator toggle in Settings.

---

## Build & run

```bash
# Daemon must be running
medousa start daemon

cd apps/medousa-home
npm install
npm run tauri dev
```

Env:

- `MEDOUSA_DAEMON_URL` — override daemon base (optional)
- `MEDOUSA_HOME_PROVIDER` / `MEDOUSA_HOME_MODEL` — interactive turn defaults (optional; fallback `ollama` / `qwen2.5:7b`)

---

## Document history

| Date | Change |
|------|--------|
| 2026-05-30 | Initial design — Workshop layout, stack lock, daemon map, Locus boundaries |
| 2026-05-30 | **M1 shipped:** Library tree, editor, context panel, card→vault links |
| 2026-05-30 | **M2 shipped:** Kanban + swimlanes, card inspector, done notifications |
| 2026-05-30 | **M2.5:** Labeled nav, session sidebar, settings, composer, work-rail filter; design refs (Cursor/Codex/Hermes) |
| 2026-05-30 | **M2.5 cont.:** Skills & Tools catalog, status strip, session pinning, `GET /v1/manuscripts` |
| 2026-05-30 | **M3 shipped:** Split panes, tray, pop-out chat, drag-to-cancel, vault diff chips |
| 2026-05-30 | **M3+ shipped:** Work kanban+inspector split pane, tray blocked-count badge |
