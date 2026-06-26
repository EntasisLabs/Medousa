# Medousa SDK — API reference

**Audience:** integrator

Full overview: [README.md](README.md). HTTP routes: [../engine/http-api.md](../engine/http-api.md).

All async methods require `medousa-sdk` feature `async` (default).

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
| `remove_model(model_id)` | `DELETE /v1/local/models/{id}` | `serde_json::Value` |

**Blocking only:** `BlockingLocalModelsClient::download_status(job_id)` → `GET /v1/local/models/download/{job_id}`

**No SDK wrapper yet:** download SSE `/v1/local/models/download/{job_id}/events`

---

## `jobs()`

| Method | HTTP | Types |
|--------|------|-------|
| `enqueue_ask(request)` | `POST /v1/jobs/ask` | `EnqueueAskRequest` → `EnqueueResponse` |

**HTTP only (no SDK wrapper):** `GET /v1/jobs/{id}/result`, `POST /v1/jobs/report`, `POST /v1/jobs/prompt`, complete-actions, archive

---

## `recurring()`

| Method | HTTP | Types |
|--------|------|-------|
| `register_prompt(request)` | `POST /v1/recurring/prompt` | `RegisterRecurringPromptRequest` → `RegisterRecurringResponse` |

**HTTP only:** list/update/delete recurring, list runs, delivery status

---

## `sessions()`

| Method | HTTP | Types |
|--------|------|-------|
| `list(limit)` | `GET /v1/sessions?limit=` | `SessionHistoryListResponse` |
| `history(session_id)` | `GET /v1/sessions/{id}/history` | `SessionHistoryResponse` |
| `set_display_name(session_id, name)` | `PUT /v1/sessions/{id}/name` | `SessionSetDisplayNameRequest` |
| `append_turn(session_id, request)` | `POST /v1/sessions/{id}/turns` | `SessionAppendTurnRequest` |

**HTTP only:** `DELETE /v1/sessions/{id}`, active-turn GET/POST, `GET /v1/sessions/{id}/turns`

---

## `interactive()`

| Method | HTTP | Types |
|--------|------|-------|
| `start_turn(request)` | `POST /v1/interactive/turn` | `InteractiveTurnRequest` → `InteractiveTurnResponse` |

SSE: open `stream_url` from response — [interactive-streaming.md](interactive-streaming.md). No built-in SSE client in SDK today (`sse` feature flag reserved).

---

## `runtime()`

| Method | HTTP | Types |
|--------|------|-------|
| `artifact_command(request)` | `POST /v1/runtime/artifact/command` | `ArtifactCommandRequest` → `ArtifactCommandResponse` |
| `artifact_fetch(request)` | `POST /v1/runtime/artifact/fetch` | `ArtifactFetchRequest` → `ArtifactFetchResponse` |
| `artifact_list_ui(request)` | `POST /v1/runtime/artifact/list-ui` | `ArtifactListUiRequest` → `ArtifactListUiResponse` |
| `config_command(request)` | `POST /v1/runtime/config/command` | `RuntimeConfigCommandRequest` |
| `stage_route_command(request)` | `POST /v1/runtime/stage-route/command` | `StageRouteCommandRequest` |

---

## `capabilities()`

| Method | HTTP | Types |
|--------|------|-------|
| `list()` | `GET /v1/capabilities` | `CapabilityListResponse` |
| `get(capability_id)` | `GET /v1/capabilities/{id}` | `CapabilityResolveResponse` |
| `reindex()` | `POST /v1/capabilities/reindex` | `serde_json::Value` |

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
| `approve(request_id, body)` | `POST .../approve` | `TurnBudgetApproveRequest` |
| `deny(request_id, body)` | `POST .../deny` | `TurnBudgetDenyRequest` |

---

## SDK gaps (use `http()` until wrapped)

- Session delete, active-turn, list session turns
- Jobs result/report/complete/archive
- Recurring CRUD
- Vault, workspace, identity, grapheme, workflows (full surface)
- Interactive SSE stream client
- Local model download SSE

Track new wrappers in PRs that add `medousa-sdk/src/*.rs` methods.
