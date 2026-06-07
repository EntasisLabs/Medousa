# Medousa Home

Tauri v2 + SvelteKit + Skeleton UI v2 workshop shell for Medousa.

Design: [`../../architecture/medousa-home-tauri-design.md`](../../architecture/medousa-home-tauri-design.md)

## Prerequisites

- Node.js 20+ with npm
- Rust toolchain (for Tauri)
- `medousa_daemon` running on `http://127.0.0.1:7419` (or set `MEDOUSA_DAEMON_URL`)

## Develop

```bash
cd apps/medousa-home
npm install   # required — includes @tauri-apps/plugin-notification (M2 done toasts)
npm run tauri dev
```

If Vite reports `Failed to resolve import "@tauri-apps/plugin-notification"`, run `npm install` again in this directory and restart the dev server.

## Surfaces

### M0
- **Chat** — `POST /v1/interactive/turn` + turn SSE
- **Work rail** — thin cards from `GET /v1/workspace/stream`
- **Activity** — `feed_appended` events from the same stream

### M1 (Library)
- **Vault tree** — `GET /v1/vault/notes`
- **Editor** — raw/preview toggle, `PUT /v1/vault/notes/{path}` with `If-Match`
- **Search** — `GET /v1/vault/search`
- **Context panel** — backlinks, wikilinks, card-linked notes (`GET /v1/workspace/cards/{id}`)

### M2 (Work board)
- **Kanban** — columns + swimlane grouping (intent, manuscript, job family, session)
- **Card inspector** — cancel, retry, ask Medousa, linked vault notes
- **Done notifications** — native OS toast when a card hits `done`
- **Home** — column count overview

Settings and tray polish ship in M3.
