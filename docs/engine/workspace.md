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

## Persistence and recovery

Workspace cards, associations, ask jobs, turn workers, revision, and feed are
projected from one ordered mutation journal. The daemon checkpoints that journal
to a versioned snapshot after 128 generations, when the journal reaches 8 MiB,
or during an explicit flush. Snapshot publication and journal retirement are
synced as one ordered owner operation; a flush reports an earlier write failure
instead of acknowledging an unsaved generation.

Recovery loads the last complete snapshot and replays complete journal records.
An incomplete final JSONL record is ignored as an interrupted append; a
generation gap or unsupported record version is corruption, not an empty
workspace. On first use, the owner imports the legacy revision, feed, card,
association, ask-job, and turn-worker files into the version-2 layout without
deleting those legacy sources.

The initial fixed safety limits are a 256-command / 16 MiB admission queue, a
64 MiB snapshot, 4,096 or 8 MiB of workspace feed entries (whichever comes
first), and 2,000 retained records per record projection. Replaceable updates
are coalesced before serialization. These are implementation safety limits, not
configuration knobs or archival guarantees.

---

## Deep links

Mobile notifications and share targets use `medousa://work/{card_id}`.

App doc: [medousa-home.md](../apps/medousa-home.md)
