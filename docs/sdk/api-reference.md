# Medousa SDK — API reference

**Audience:** integrator

Full overview: [README.md](README.md). HTTP routes: [../engine/http-api.md](../engine/http-api.md).

Contract source of truth: [`../../sdk-contract/manifest.yaml`](../../sdk-contract/manifest.yaml) (validated by `scripts/check-sdk-contract.sh`).

Rust async methods require `medousa-sdk` feature `async` (default). SSE requires `sse` (default). Python is async-first with accessor-based sync client.

---

## `health()`

| Method | HTTP | Response type |
|--------|------|---------------|
| `get()` | `GET /health` | `HealthResponse` |

---

## `http()`

Generic JSON escape hatch when no typed wrapper exists.

| Method | HTTP |
|--------|------|
| `get<T>(path)` | GET |
| `get_query<T>(path, query)` | GET with query |
| `post<T, B>(path, body)` | POST |
| `post_empty<T>(path)` | POST empty body |
| `put<T, B>(path, body)` | PUT |
| `patch<T, B>(path, body)` | PATCH |
| `delete<T>(path)` | DELETE |

---

## `ingest()`

| Method | HTTP | Types |
|--------|------|-------|
| `post(request)` | `POST /v1/ingest` | `IngestRequest` → `IngestResponse` |

---

## `local_models()`

| Method | HTTP | Types |
|--------|------|-------|
| `hardware()` | `GET /v1/local/hardware` | `LocalHardwareResponse` |
| `catalog()` | `GET /v1/local/catalog` | `LocalCatalogResponse` |
| `list()` | `GET /v1/local/models` | `LocalModelsResponse` |
| `engine_status()` | `GET /v1/local/engine/status` | `LocalEngineStatus` |
| `start_download(model_id)` | `POST /v1/local/models/download` | `LocalModelDownloadResponse` |
| `remove_model(model_id)` | `DELETE /v1/local/models/{id}` | JSON |
| `download_status(job_id)` | `GET /v1/local/models/download/{job_id}` | `ModelDownloadProgress` |
| `download_events(job_id)` | SSE `GET .../events` | `ModelDownloadProgress` stream |

---

## `jobs()`

| Method | HTTP | Types |
|--------|------|-------|
| `enqueue_ask(request)` | `POST /v1/jobs/ask` | `EnqueueAskRequest` → `EnqueueResponse` |
| `result(job_id)` | `GET /v1/jobs/{id}/result` | `JobResultResponse` |
| `report(job_id)` | `GET /v1/jobs/{id}/report` | `JobReportResponse` |
| `enqueue_report(request)` | `POST /v1/jobs/report` | `EnqueueReportRequest` |
| `enqueue_prompt(request)` | `POST /v1/jobs/prompt` | `EnqueuePromptRequest` |
| `complete_actions(job_id, request)` | `POST .../complete-actions` | `AskJobCompleteActionsRequest` |
| `archive(job_id, request)` | `POST .../archive` | `ArchiveAskJobRequest` |

---

## `recurring()`

| Method | HTTP | Types |
|--------|------|-------|
| `register_prompt(request)` | `POST /v1/recurring/prompt` | `RegisterRecurringPromptRequest` |
| `list()` | `GET /v1/recurring` | `RecurringListResponse` |
| `update(recurring_id, request)` | `PATCH /v1/recurring/{id}` | `UpdateRecurringRequest` |
| `delete(recurring_id)` | `DELETE /v1/recurring/{id}` | `DeleteRecurringResponse` |
| `runs(recurring_id)` | `GET .../runs` | `RecurringRunsResponse` |
| `delivery_status(recurring_id)` | `GET .../delivery` | `RecurringDeliveryResponse` |

---

## `sessions()`

| Method | HTTP | Types |
|--------|------|-------|
| `list(limit)` | `GET /v1/sessions?limit=` | `SessionHistoryListResponse` |
| `history(session_id)` | `GET /v1/sessions/{id}/history` | `SessionHistoryResponse` |
| `set_display_name(session_id, name)` | `PUT /v1/sessions/{id}/name` | `SessionSetDisplayNameRequest` |
| `append_turn(session_id, request)` | `POST /v1/sessions/{id}/turns` | `SessionAppendTurnRequest` |
| `delete(session_id)` | `DELETE /v1/sessions/{id}` | `SessionDeleteResponse` |
| `list_turns(session_id)` | `GET /v1/sessions/{id}/turns` | `SessionHistoryResponse` |
| `active_turn(session_id)` | `GET .../active-turn` | active turn ticket |
| `cancel_active_turn(session_id)` | `POST .../active-turn` | cancel |

---

## `interactive()`

| Method | HTTP | Types |
|--------|------|-------|
| `start_turn(request)` | `POST /v1/interactive/turn` | `InteractiveTurnRequest` → `InteractiveTurnResponse` |
| `stream(stream_url)` | SSE from `stream_url` | `InteractiveTurnStreamEvent` stream |
| `stream_turn(request)` | start + SSE | combined helper |
| `cancel(session_id)` | `POST /v1/sessions/{id}/active-turn` | cancel active turn |

Both Rust (`sse` feature) and Python ship built-in SSE clients — [interactive-streaming.md](interactive-streaming.md).

---

## `runtime()`

| Method | HTTP | Types |
|--------|------|-------|
| `artifact_command(request)` | `POST /v1/runtime/artifact/command` | `ArtifactCommandRequest` |
| `artifact_fetch(request)` | `POST /v1/runtime/artifact/fetch` | `ArtifactFetchRequest` |
| `artifact_list_ui(request)` | `POST /v1/runtime/artifact/list-ui` | `ArtifactListUiRequest` |
| `config_command(request)` | `POST /v1/runtime/config/command` | `RuntimeConfigCommandRequest` |
| `stage_route_command(request)` | `POST /v1/runtime/stage-route/command` | `StageRouteCommandRequest` |

---

## `capabilities()`

| Method | HTTP | Types |
|--------|------|-------|
| `list()` | `GET /v1/capabilities` | `CapabilityListResponse` |
| `get(capability_id)` | `GET /v1/capabilities/{id}` | `CapabilityResolveResponse` |
| `reindex()` | `POST /v1/capabilities/reindex` | JSON |

---

## `mcp_gateway()`

| Method | HTTP |
|--------|------|
| `status()` | `GET /v1/mcp/gateway/status` |

---

## `budget()`

| Method | HTTP | Types |
|--------|------|-------|
| `list(pending_only)` | `GET /v1/turns/budget-requests?...` | `TurnBudgetRequestListResponse` |
| `get(request_id)` | `GET /v1/turns/budget-requests/{id}` | `TurnBudgetRequestResponse` |
| `approve(request_id, body)` | `POST .../approve` | `TurnBudgetApproveRequest` |
| `deny(request_id, body)` | `POST .../deny` | `TurnBudgetDenyRequest` |

---

## `vault()`

| Method | HTTP | Types |
|--------|------|-------|
| `list_roots()` | `GET /v1/vault/roots` | `VaultRootsResponse` |
| `add_root(request)` | `POST /v1/vault/roots` | `VaultAddRootRequest` |
| `set_active_root(request)` | `PUT /v1/vault/active` | `VaultSetActiveRootRequest` |
| `list_notes(query)` | `GET /v1/vault/notes` | `VaultNotesListResponse` |
| `create_note(request)` | `POST /v1/vault/notes` | `VaultWriteRequest` |
| `get_note(path)` | `GET /v1/vault/notes/{path}` | `VaultNoteContentResponse` |
| `update_note(path, request)` | `PUT /v1/vault/notes/{path}` | `VaultWriteRequest` |
| `delete_note(path)` | `DELETE /v1/vault/notes/{path}` | `VaultDeleteResponse` |
| `list_tags(query)` | `GET /v1/vault/tags` | `VaultTagsListResponse` |
| `search(query)` | `GET /v1/vault/search` | `VaultSearchResponse` |
| `backlinks(query)` | `GET /v1/vault/backlinks` | `VaultBacklinksResponse` |

---

## `workspace()`

| Method | HTTP | Types |
|--------|------|-------|
| `list_cards()` | `GET /v1/workspace/cards` | JSON |
| `get_card(card_id)` | `GET /v1/workspace/cards/{id}` | JSON |
| `cancel_card(card_id)` | `POST .../cancel` | `WorkspaceCardActionResponse` |
| `archive_card(card_id)` | `POST .../archive` | `WorkspaceCardActionResponse` |
| `retry_card(card_id)` | `POST .../retry` | `WorkspaceCardActionResponse` |
| `link_vault(card_id, request)` | `POST .../link-vault` | `WorkspaceLinkVaultRequest` |
| `feed()` | `GET /v1/workspace/feed` | JSON |
| `snapshot()` | `GET /v1/workspace/snapshot` | JSON |
| `stream()` | SSE `GET /v1/workspace/stream` | planned |

---

## Sync clients

| Rust | Python |
|------|--------|
| `BlockingMedousaClient` — same accessors, blocking reqwest | `MedousaClientSync` — `client.health().get()` pattern |

SSE streaming is async-only on both SDKs.

---

## Types parity

| Rust | Python |
|------|--------|
| `medousa_types::*` | `medousa.types.*` (generated from JSON Schema) |

Regenerate Python types after Rust DTO changes:

```bash
cargo run -p medousa-types-schema
python scripts/gen-python-types.py
```

See [python.md](python.md).

---

## Remaining gaps (use `http()`)

- Identity, grapheme, workflows (full surface)
- Ingest SSE stream
- Tauri app uses bridge commands for SSE when `WorkshopTransport` cannot stream directly

Track new wrappers in PRs that update `sdk-contract/manifest.yaml`.
