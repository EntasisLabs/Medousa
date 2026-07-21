# HTTP API reference (Medousa Engine)

**Audience:** integrator

Base URL default: `http://127.0.0.1:7419`  
Override: `MEDOUSA_DAEMON_URL`

Types: [`medousa-types`](../../crates/medousa-types/) (`daemon_api`, `session`, `local`, …).  
SDK: [`docs/sdk/api-reference.md`](../sdk/api-reference.md).  
Component notes: [component-daemon.md](../../architecture/component-daemon.md).

Subsystem guides: [interactive-streaming](interactive-streaming.md) · [artifacts](artifacts.md) · [vault](vault.md) · [calendar](calendar.md) · [workspace](workspace.md) · [agent-tools](agent-tools.md) · [runtime-config](runtime-config.md) · [extensions](extensions.md)

---

## Health & ops

| Method | Path | Types / response | SDK | CLI |
|--------|------|------------------|-----|-----|
| GET | `/health` | `HealthResponse` | `health().get()` | `medousa doctor` |
| GET | `/v1/stats` | `DaemonStatsResponse` | `http().get` | — |
| GET | `/v1/heartbeat/status` | `HeartbeatStatusResponse` | `http().get` | — |
| GET | `/v1/delivery/status` | `DeliveryHealthResponse` | `http().get` | — |
| GET | `/v1/continuations/status` | `ContinuationStatusResponse` | `http().get` | — |
| GET | `/v1/continuations/lineage/{turn_correlation_id}` | `TurnContinuationLineageResponse` | `http().get` | — |
| POST | `/v1/jobs/{job_id}/replay-and-resume` | `ReplayAndResumeResponse` | `http().post` | — |

Stasis dashboard mounted at `/dashboard` (HTML UI).

---

## Interactive chat (two-step)

| Method | Path | Types | SDK |
|--------|------|-------|-----|
| POST | `/v1/interactive/turn` | `InteractiveTurnRequest` → `InteractiveTurnResponse` (includes `stream_url`) | `interactive().start_turn` |
| GET | `/v1/interactive/turn/{turn_id}/stream` | SSE: `InteractiveTurnStreamEvent` | `interactive().stream` / `stream_reconnecting` |

**Stream query:** `GET …/stream?since=<seq>` (optional `u64`, default `0`). Replays events with `seq > since` from the **durable turn journal** on disk, then tails live events. Each SSE payload includes monotonic **`seq`** per turn — clients track the last seen `seq` and reconnect with `?since=` after drops.

See [interactive-streaming.md](interactive-streaming.md). **Do not** expect SSE on the POST itself.

### Sessions & turns

| Method | Path | Types | SDK |
|--------|------|-------|-----|
| GET | `/v1/sessions` | `SessionHistoryListResponse` | `sessions().list` |
| GET | `/v1/sessions/{session_id}/history` | `SessionHistoryResponse` | `sessions().history` |
| PUT | `/v1/sessions/{session_id}/name` | `SessionSetDisplayNameRequest` | `sessions().set_display_name` |
| DELETE | `/v1/sessions/{session_id}` | — | `http().delete` |
| POST | `/v1/sessions/{session_id}/turns` | `SessionAppendTurnRequest` | `sessions().append_turn` |
| GET | `/v1/sessions/{session_id}/turns` | turn list | `http().get` |
| GET | `/v1/sessions/{session_id}/active-turn` | active turn ticket | `http().get` |
| POST | `/v1/sessions/{session_id}/active-turn` | cancel active turn | `http().post` |
| POST | `/v1/turns` | create turn ticket | `http().post` |
| GET | `/v1/turns/{turn_id}` | turn ticket | `http().get` |

---

## Ingest & channels

| Method | Path | Types | SDK |
|--------|------|-------|-----|
| POST | `/v1/ingest` | `IngestRequest` → `IngestResponse` | `ingest().post` |
| GET | `/v1/ingest/{stream_id}/stream` | ingest SSE (`?since=<seq>` same as interactive) | `http().get` |
| POST | `/v1/deliver/outbox` | webhook delivery | `http().post` |
| GET | `/v1/deliver/poll/{job_id}` | `DeliverPollResponse` | `http().get` |

---

## Jobs & recurring

| Method | Path | Types | SDK |
|--------|------|-------|-----|
| POST | `/v1/jobs/ask` | `EnqueueAskRequest` → `EnqueueResponse` | `jobs().enqueue_ask` |
| GET | `/v1/jobs/{job_id}/result` | `JobResultResponse` | `http().get` |
| GET | `/v1/jobs/{job_id}/report` | `JobReportResponse` | `http().get` |
| POST | `/v1/jobs/{job_id}/complete-actions` | `AskJobCompleteActionsRequest` | `http().post` |
| POST | `/v1/jobs/{job_id}/archive` | `ArchiveAskJobRequest` | `http().post` |
| POST | `/v1/jobs/report` | `EnqueueReportRequest` | `http().post` |
| POST | `/v1/jobs/prompt` | `EnqueuePromptRequest` | `http().post` |
| GET | `/v1/recurring` | list definitions | `http().get` |
| POST | `/v1/recurring/prompt` | `RegisterRecurringPromptRequest` | `recurring().register_prompt` |
| PATCH | `/v1/recurring/{recurring_id}` | update | `http().patch` |
| DELETE | `/v1/recurring/{recurring_id}` | delete | `http().delete` |
| GET | `/v1/recurring/{recurring_id}/runs` | runs | `http().get` |
| GET | `/v1/recurring/{recurring_id}/delivery` | delivery status | `http().get` |

---

## Runtime commands & artifacts

| Method | Path | Types | SDK |
|--------|------|-------|-----|
| GET | `/v1/runtime/defaults` | runtime defaults | `http().get` |
| GET/PUT | `/v1/runtime/tui-defaults` | JSON defaults blob | `http().get/put` |
| PUT | `/v1/runtime/inference-profiles` | inference profiles | `http().put` |
| POST | `/v1/runtime/config/command` | `RuntimeConfigCommandRequest` | `runtime().config_command` |
| POST | `/v1/runtime/stage-route/command` | `StageRouteCommandRequest` | `runtime().stage_route_command` |
| POST | `/v1/runtime/artifact/command` | `ArtifactCommandRequest` | `runtime().artifact_command` |
| POST | `/v1/runtime/artifact/fetch` | `ArtifactFetchRequest` | `runtime().artifact_fetch` |
| POST | `/v1/runtime/artifact/write` | `ArtifactWriteRequest` | `runtime().artifact_write` |
| POST | `/v1/runtime/artifact/delete` | `ArtifactDeleteRequest` | `runtime().artifact_delete` |
| POST | `/v1/runtime/artifact/list-ui` | `ArtifactListUiRequest` | `runtime().artifact_list_ui` |

See [artifacts.md](artifacts.md), [runtime-config.md](runtime-config.md).

---

## Environment (canvas)

| Method | Path | SDK |
|--------|------|-----|
| GET/PUT | `/v1/environment/spec` | `environment().get_spec` / `put_spec` |
| GET | `/v1/environment/status` | `environment().get_status` |
| POST | `/v1/environment/spec/validate` | `environment().validate_spec` |
| POST | `/v1/environment/spec/propose` | `environment().propose_spec` |
| GET/DELETE | `/v1/environment/spec/pending` | `environment().get_pending` / `dismiss_pending` |
| POST | `/v1/environment/spec/pending/apply` | `environment().apply_pending` |
| GET (SSE) | `/v1/environment/spec/stream` | `environment().stream_spec` |

Patch ops (`remove_custom_surface`, `remove_component`, etc.) are **agent-tool only** via `cognition_environment_patch`. Integrators replace the full spec with `PUT /v1/environment/spec` (same as Home).

---

## Components (canvas)

| Method | Path | SDK |
|--------|------|-----|
| GET/PUT | `/v1/components/{id}/store` | `components().store_get` / `store_set` |
| GET | `/v1/components/{id}/store/keys` | `components().store_list_keys` |
| GET/PUT/DELETE | `/v1/components/{id}/store/{key}` | `components().store_get_key` / `store_put_key` / `store_delete_key` |
| GET/POST | `/v1/components/{id}/runtime/events` | `components().runtime_tail_events` / `runtime_append_events` |
| POST | `/v1/components/{id}/runtime/probe/{probe_id}/result` | `components().runtime_complete_probe` |

---

## Feeds (canvas)

| Method | Path | SDK |
|--------|------|-----|
| GET | `/v1/feeds` | `feeds().list` |
| GET | `/v1/feeds/{feed_id}/tail` | `feeds().tail` |
| POST | `/v1/feeds/{feed_id}/read` | `feeds().mark_read` |
| GET (SSE) | `/v1/feeds/stream` | `feeds().stream` |

---

## Turn budget

| Method | Path | SDK |
|--------|------|-----|
| GET | `/v1/turns/budget-requests` | `budget().list` |
| GET | `/v1/turns/budget-requests/{request_id}` | `http().get` |
| POST | `/v1/turns/budget-requests/{request_id}/approve` | `budget().approve` |
| POST | `/v1/turns/budget-requests/{request_id}/deny` | `budget().deny` |

---

## Vault

| Method | Path | Purpose |
|--------|------|---------|
| GET/POST | `/v1/vault/roots` | List / add vault roots |
| PUT | `/v1/vault/active` | Set active root |
| GET/POST | `/v1/vault/notes` | List / create notes |
| GET/PUT/DELETE | `/v1/vault/notes/{*note_path}` | Read / write / delete note |
| GET | `/v1/vault/tags` | List tags |
| GET | `/v1/vault/search` | Full-text search |
| GET | `/v1/vault/backlinks` | Backlinks for path |

See [vault.md](vault.md).

---

## Agents (hot-swappable runtimes)

External ACP agents (Cursor / Codex). Clients use the Medousa SDK `agents()` accessor — not raw ACP. Native Medousa turns remain on `/v1/turns` + interactive.

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/v1/agents/runtimes` | List runtimes + availability |
| POST | `/v1/agents/sessions` | Create ACP session (bind Medousa `session_id`) |
| POST | `/v1/agents/sessions/{id}/prompt` | Send prompt |
| GET | `/v1/agents/sessions/{id}/stream` | SSE (same event shape as interactive) |
| POST | `/v1/agents/sessions/{id}/cancel` | Cancel session |
| GET | `/v1/agents/permission-requests` | List pending ACP permissions |
| POST | `/v1/agents/permission-requests/{id}/approve` | Approve |
| POST | `/v1/agents/permission-requests/{id}/deny` | Deny |

See [ADR-008](../architecture/decisions/adr-008-hot-swappable-agent-runtime.md) and [acp-external-agents](../cookbook/acp-external-agents.md).

---

## Calendar

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/v1/calendar/events` | List events in range (RRULE expanded) |
| POST | `/v1/calendar/events` | Create event |
| PUT/DELETE | `/v1/calendar/events/{uid}` | Update / delete event |
| POST | `/v1/calendar/import` | Merge ICS into vault calendar |
| GET | `/v1/calendar/export` | Export raw ICS |

---

## Workspace

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/v1/workspace/cards` | List cards |
| GET | `/v1/workspace/cards/{card_id}` | Card detail |
| POST | `/v1/workspace/cards/{card_id}/cancel` | Cancel |
| POST | `/v1/workspace/cards/{card_id}/archive` | Archive |
| POST | `/v1/workspace/cards/{card_id}/retry` | Retry |
| POST | `/v1/workspace/cards/{card_id}/link-vault` | Link vault note |
| GET | `/v1/workspace/feed` | Activity feed |
| GET | `/v1/workspace/snapshot` | Board snapshot |
| POST | `/v1/workspace/rebuild` | Rebuild projector |
| GET | `/v1/workspace/stream` | SSE feed |

See [workspace.md](workspace.md).

---

## Identity

| Method | Path |
|--------|------|
| POST | `/v1/identity/context` |
| POST | `/v1/identity/remember` |
| POST | `/v1/identity/digest-preview` |
| POST | `/v1/identity/export-markdown` |
| GET/POST | `/v1/identity/profiles` |
| PUT | `/v1/identity/profiles/active` |
| POST | `/v1/identity/profiles/export` |
| POST | `/v1/identity/profiles/import` |
| POST | `/v1/identity/update/propose` |
| POST | `/v1/identity/update/commit` |
| POST | `/v1/identity/history` |
| POST | `/v1/identity/rollback` |

CLI: `medousa-cli daemon-identity-*`

---

## Local inference (probe-only daemon)

The daemon **probes** `medousa_local` on `:7421`. Loading models uses `medousa models engine-load` or [`medousa-host`](../../crates/medousa-host/) — **not** a daemon `engine/load` route.

| Method | Path | SDK |
|--------|------|-----|
| GET | `/v1/local/hardware` | `local_models().hardware` |
| GET | `/v1/local/catalog` | `local_models().catalog` |
| GET | `/v1/local/models` | `local_models().list` |
| POST | `/v1/local/models/download` | `local_models().start_download` |
| GET | `/v1/local/models/download/{job_id}` | blocking `download_status` |
| GET | `/v1/local/models/download/{job_id}/events` | SSE progress |
| DELETE | `/v1/local/models/{model_id}` | `local_models().remove_model` |
| GET | `/v1/local/engine/status` | `local_models().engine_status` |

Provider id: `medousa-local` → `http://127.0.0.1:7421/v1`

---

## Capabilities & MCP

| Method | Path | SDK |
|--------|------|-----|
| GET | `/v1/capabilities` | `capabilities().list` |
| GET | `/v1/capabilities/{capability_id}` | `capabilities().get` |
| POST | `/v1/capabilities/reindex` | `capabilities().reindex` |
| GET | `/v1/mcp/gateway/status` | `mcp_gateway().status` |
| POST | `/v1/mcp/policy/evaluate` | `http().post` |

Setup: [mcp-gateway-setup.md](../mcp-gateway-setup.md)

---

## Manuscripts, models catalog, media, STT

| Method | Path |
|--------|------|
| GET/POST | `/v1/manuscripts` |
| GET/PATCH | `/v1/manuscripts/{manuscript_id}` |
| GET | `/v1/models/catalog` |
| GET | `/v1/models/capabilities` |
| POST | `/v1/models/catalog/refresh` |
| POST | `/v1/media/upload` |
| GET | `/v1/media/{media_id}` |
| GET | `/v1/stt/status` |
| POST | `/v1/stt/transcribe` |

See [extensions.md](extensions.md).

---

## Workflows & tool history

| Method | Path |
|--------|------|
| GET/POST | `/v1/workflows` |
| POST | `/v1/workflows/plan` |
| POST | `/v1/workflows/schedule` |
| GET | `/v1/workflows/{workflow_id}` |
| GET | `/v1/workflows/{workflow_id}/runs` |
| GET | `/v1/tool-history/slices` |
| POST | `/v1/workflows/from-slice` |

---

## Grapheme & Locus

| Method | Path |
|--------|------|
| GET | `/v1/grapheme/modules` |
| GET | `/v1/grapheme/modules/{module_id}` |
| GET | `/v1/grapheme/modules/{module_id}/ops` |
| GET/PUT | `/v1/grapheme/allowlist` |
| GET/POST | `/v1/grapheme/scripts` |
| GET | `/v1/grapheme/scripts/{script_id}` |
| POST | `/v1/grapheme/compile` |
| POST | `/v1/grapheme/modules/load` |
| GET | `/v1/grapheme/lifecycle` |
| GET | `/v1/grapheme/lsp/workspace` |
| GET | `/v1/grapheme/lsp` (WebSocket) |
| POST | `/v1/grapheme/run` |
| GET | `/v1/locus/nodes` |
| GET | `/v1/locus/nodes/{sync_key}` |
| GET | `/v1/locus/tags` |

---

## Pairing (LAN / phone)

| Method | Path |
|--------|------|
| GET | `/qr` |
| GET | `/qr/image` |
| GET | `/qr.png` |
| POST | `/qr/rotate` |
| GET | `/pair/status` |
| GET | `/pair/iroh-ticket` |
| GET | `/pair/code` |
| POST | `/pair/init` |
| POST | `/pair/verify` |
| GET | `/pair/heartbeat` |
| POST | `/pair/heartbeat` |
| DELETE | `/pair/{pairing_id}` — loopback admin, or `Authorization: Bearer` session token for that pairing |

Cookbook: [mobile-and-lan.md](../cookbook/mobile-and-lan.md)

---

## Integration patterns

**Sync ask:**

```bash
medousa-cli daemon-ask "Summarize open risks" --daemon-url http://127.0.0.1:7419
```

**Async job:**

1. `POST /v1/jobs/ask`
2. Poll `GET /v1/jobs/{id}/result`

**Streaming chat:**

1. `POST /v1/interactive/turn`
2. `GET` the returned `stream_url` as SSE

More: [integrate-without-the-app.md](../cookbook/integrate-without-the-app.md)
