# Medousa Home — Tauri UI Design

> **Status:** M0 scaffold — Tauri v2 + SvelteKit + Skeleton UI v2  
> **Stack (locked):** Tauri 2 · SvelteKit 2 · Svelte 5 · Skeleton 2.x · daemon HTTP/SSE only

---

## Principles

1. **`medousa_daemon` is the only source of truth.** The Tauri shell is a thin adapter: HTTP POST + SSE subscribe. No workspace/vault business logic in the UI crate.
2. **Locus stays out of v1 Home.** Vault/workspace reads flow through frozen daemon APIs. Locus bridge writes remain debounced server-side (V2); no Locus editor in the main surface until a dedicated drawer milestone.
3. **Activity ≠ Locus.** The right panel shows `WorkspaceEvent` from `feed_appended` — not a Locus graph dump.
4. **Codex command center + Obsidian library + Word-like prose** — but M0 ships chat + work rail + activity only; vault editor is M1.

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
┌──┬──────────────────────────────────────────────┬──────────┐
│█ │  Primary surface (one at a time)             │ Activity │
│█ │  · Chat (default)                            │ feed SSE │
│█ │  · Vault prose editor (M1)                   │ or card  │
│█ │  · Card inspector (M2)                       │ context  │
│█ │                                              │          │
├──┴──────────────────────────────────────────────┴──────────┤
│ Work rail — thin cards from workspace stream (SSE)         │
└────────────────────────────────────────────────────────────┘
█ = icon rail (48px): Home · Chat · Library · Work · Settings
```

### Navigation (icon rail)

| Icon | Surface | M0 | M1+ |
|------|---------|----|-----|
| Home | Dashboard stub → chat | ✓ | Kanban overview |
| Chat | Interactive turn SSE | ✓ | Pop-out (M3) |
| Library | Vault tree + editor | — | M1 |
| Work | Card inspector / kanban | rail only | M2 |
| Settings | Daemon URL, theme | stub | full |

### Default landing

**Chat** on launch (Codex-style command center). Last vault note restore deferred to M1.

### Work rail placement

**Bottom horizontal rail** (Codex-style). Vertical left rail remains an M3 option.

---

## Daemon route map (client)

All calls originate in `src-tauri/`; Svelte invokes commands and listens for events.

### M0 — wired

| UI need | Method | Route | Transport |
|---------|--------|-------|-----------|
| Health | GET | `/v1/health` | HTTP |
| Workspace snapshot + live | GET | `/v1/workspace/stream?since_revision=` | SSE → `workspace://event` |
| Send message | POST | `/v1/interactive/turn` | HTTP |
| Stream reply | GET | `/v1/interactive/turn/{id}/stream` | SSE → `interactive://event` |

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

### M1 — library (current)

- [x] Vault tree + search (`GET /v1/vault/notes`, `/search`)
- [x] Prose editor with raw/preview toggle + `PUT` save (`If-Match: content_hash`)
- [x] Backlinks + wikilinks in right context panel
- [x] Work card click → `GET /v1/workspace/cards/{id}` → linked vault paths
- [x] Last-opened note restored from `localStorage`
- [ ] Vault create/delete UI (CLI OK for now)

### M2 — full home

- Kanban toggle + swimlanes (`intent`, `manuscript_id`)
- `card_upserted` → Done native notification
- Card inspector primary surface

### M3 — polish

- Split panes, system tray, pop-out chat
- Drag-to-cancel only (no fake reorder)

---

## Theme

Skeleton preset **sahara** (warm study). Dark mode via `class` strategy; operator toggle in Settings (M1).

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
