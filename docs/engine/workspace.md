# Workspace

**Audience:** integrator

The workspace projector surfaces agent work as **cards** on a kanban-style board (desktop) or timeline (mobile).

---

## HTTP API

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/v1/workspace/cards` | List cards (filters via query) |
| GET | `/v1/workspace/cards/{card_id}` | Card detail + linked vault notes |
| POST | `/v1/workspace/cards/{card_id}/cancel` | Cancel running work |
| POST | `/v1/workspace/cards/{card_id}/archive` | Archive |
| POST | `/v1/workspace/cards/{card_id}/retry` | Retry failed job |
| POST | `/v1/workspace/cards/{card_id}/link-vault` | Attach vault note |
| GET | `/v1/workspace/feed` | Activity feed entries |
| GET | `/v1/workspace/snapshot` | Full board snapshot |
| POST | `/v1/workspace/rebuild` | Rebuild projector from ledger |
| GET | `/v1/workspace/stream` | **SSE** — card/feed updates |

---

## SSE stream

`GET /v1/workspace/stream` emits events when cards move, complete, or feed entries append. The Medousa app uses Tauri `workspace_stream_start` / `workspace_stream_stop`.

SDK: use `client.http().get` on stream URL with a custom SSE client, or mirror the app bridge.

---

## Deep links

Mobile notifications and share targets use `medousa://work/{card_id}`.

App doc: [medousa-home.md](../apps/medousa-home.md)
