# Medousa (app source)

Native desktop and mobile shell for **Medousa**. User-facing product docs: [../../README.md](../../README.md).

Developer build instructions: [../../docs/cookbook/build-from-source.md](../../docs/cookbook/build-from-source.md).

Tauri v2 + SvelteKit + Skeleton UI v2 workshop shell for Medousa.

Design: [`../../architecture/medousa-home-tauri-design.md`](../../architecture/medousa-home-tauri-design.md)  
Mobile: [`../../architecture/medousa-home-mobile-plan.md`](../../architecture/medousa-home-mobile-plan.md)  
**iPhone dev (Mac):** [`MOBILE-DEV.md`](MOBILE-DEV.md)

## Prerequisites

- Node.js 20+ with npm
- Rust toolchain (for Tauri)
- **Released app builds** bundle and start the engine automatically — no terminal.
- **Dev only:** if the sidecar is missing, run `medousa_daemon` on `http://127.0.0.1:7419` or set `MEDOUSA_DAEMON_URL`.

## Develop

```bash
cd apps/medousa-home
npm install   # required — includes @tauri-apps/plugin-notification (M2/M8e)
npm run tauri dev
```

If Vite reports `Failed to resolve import "@tauri-apps/plugin-notification"`, run `npm install` again in this directory and restart the dev server.

On Linux, you may see `libayatana-appindicator is deprecated` once at startup when the tray icon loads. That comes from Tauri’s tray stack (`tray-icon` → libappindicator), not Medousa Home code; it is harmless until upstream migrates to `libayatana-appindicator-glib` ([tauri-apps/tray-icon#260](https://github.com/tauri-apps/tray-icon/issues/260)).

## Surfaces

### M0
- **Chat** — `POST /v1/interactive/turn` + turn SSE
- **Work rail** — thin cards from `GET /v1/workspace/stream`
- **Activity** — `feed_appended` events from the same stream

### M1 (Library)
- **Vault tree** — space roots (Journal, Inbox, …), filter chips, Lucide icons, system noise filter
- **Editor** — space badge in header, empty state, create note, quick capture, space templates
- **Search** — `GET /v1/vault/search`
- **Context panel** — backlinks, wikilinks, card-linked notes (`GET /v1/workspace/cards/{id}`)

### M2 (Work board)
- **Kanban** — columns + swimlane grouping (intent, manuscript, job family, session)
- **Card inspector** — cancel, retry, ask Medousa, linked vault notes
- **Done notifications** — native OS toast when a card hits `done`
- **Home** — column count overview

Settings and tray polish ship in M3.

### M8 (Mobile — Pulse shell)

At viewport **≤768px** (or resize your devtools), Home switches to the **mobile shell**:

- **Pulse** — glance + one hero action
- **Work** — vertical timeline (not kanban)
- **Chat** — same turn SSE, thumb-friendly frame
- **You** — Notes, Skills, Schedule, Channels, Settings, Workshop health

**M8e native touches:** notification tap opens the work card (`medousa://work/{id}`), blocked-count app badge, haptics on key gestures, OS share sheet on job results.

**M9 product skin:** Pulse answers *waiting / working / quiet* in human language (no filename hero, no three zero-tiles). Global top chrome removed; You hub split into Stay in touch · Workshop. See [mobile-m9-plan](../../architecture/medousa-home-mobile-m9-plan.md).

Deep link dev (browser): `http://localhost:1420/?work=<card-id>`. Tauri desktop: `xdg-open 'medousa://work/<card-id>'`.

Desktop layout is unchanged above the breakpoint.
