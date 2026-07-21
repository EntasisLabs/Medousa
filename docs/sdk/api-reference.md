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
| `stream_reconnecting(stream_url)` | SSE with `?since=` replay | `InteractiveTurnStreamEvent` stream (client helper) |
| `stream_reconnecting_with_policy(stream_url, policy)` | SSE with custom `ReconnectPolicy` | `InteractiveTurnStreamEvent` stream |
| `stream_turn_reconnecting(request)` | start + reconnecting SSE | combined helper (recommended) |
| `cancel(session_id)` | `POST /v1/sessions/{id}/active-turn` | cancel active turn |

**Client helpers** (`stream_reconnecting*`, `stream_turn_reconnecting`) are not separate HTTP routes — they track `event.seq`, reconnect with `?since=<last_seq>`, and apply bounded backoff + overlap guard. See `medousa_sdk::ReconnectPolicy` and `medousa_sdk::stream_path_with_since`.

Both Rust (`sse` feature) and Python ship built-in SSE clients — [interactive-streaming.md](interactive-streaming.md).

---

## `agents()`

Hot-swappable external agent runtimes (Cursor / Codex via ACP). Native Medousa turns stay on `interactive()` / turn tickets. See [ADR-008](../architecture/decisions/adr-008-hot-swappable-agent-runtime.md).

| Method | HTTP | Types |
|--------|------|-------|
| `list_runtimes()` | `GET /v1/agents/runtimes` | `AgentRuntimeListResponse` |
| `create_session(request)` | `POST /v1/agents/sessions` | `CreateAgentSessionRequest` → `CreateAgentSessionResponse` |
| `prompt(id, request)` | `POST /v1/agents/sessions/{id}/prompt` | `AgentSessionPromptRequest` → `AgentSessionPromptResponse` |
| `stream(stream_url)` | `GET …/stream` (SSE) | `InteractiveTurnStreamEvent` |
| `stream_session(request)` | create + SSE | combined helper |
| `cancel(id)` | `POST /v1/agents/sessions/{id}/cancel` | `CancelAgentSessionResponse` |
| `list_permission_requests(status?, limit?)` | `GET /v1/agents/permission-requests` | `AgentPermissionRequestListResponse` |
| `approve_permission(id, request)` | `POST …/approve` | `AgentPermissionResolveResponse` |
| `deny_permission(id, request)` | `POST …/deny` | `AgentPermissionResolveResponse` |

---

## `runtime()`

| Method | HTTP | Types |
|--------|------|-------|
| `artifact_command(request)` | `POST /v1/runtime/artifact/command` | `ArtifactCommandRequest` |
| `artifact_fetch(request)` | `POST /v1/runtime/artifact/fetch` | `ArtifactFetchRequest` |
| `artifact_write(request)` | `POST /v1/runtime/artifact/write` | `ArtifactWriteRequest` |
| `artifact_delete(request)` | `POST /v1/runtime/artifact/delete` | `ArtifactDeleteRequest` |
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

## `calendar()`

| Method | HTTP | Types |
|--------|------|-------|
| `list_events(query)` | `GET /v1/calendar/events` | `CalendarListResponse` |
| `create_event(request)` | `POST /v1/calendar/events` | `CalendarWriteRequest` → `CalendarWriteResponse` |
| `update_event(uid, request)` | `PUT /v1/calendar/events/{uid}` | `CalendarWriteRequest` → `CalendarWriteResponse` |
| `delete_event(uid, query?)` | `DELETE /v1/calendar/events/{uid}` | `CalendarDeleteResponse` |
| `import_ics(request)` | `POST /v1/calendar/import` | `CalendarImportRequest` → `CalendarImportResponse` |
| `export(query?)` | `GET /v1/calendar/export` | `CalendarExportResponse` |

---

## `environment()`

| Method | HTTP | Types |
|--------|------|-------|
| `get_spec(profile_id?)` | `GET /v1/environment/spec` | `EnvironmentSpecResponse` |
| `put_spec(request)` | `PUT /v1/environment/spec` | `EnvironmentSpecPutRequest` |
| `get_status(...)` | `GET /v1/environment/status` | `EnvironmentStatusResponse` |
| `validate_spec(request)` | `POST /v1/environment/spec/validate` | `EnvironmentValidateRequest` |
| `propose_spec(request)` | `POST /v1/environment/spec/propose` | `EnvironmentSpecPutRequest` |
| `get_pending(profile_id?)` | `GET /v1/environment/spec/pending` | `EnvironmentPendingResponse` |
| `dismiss_pending(profile_id?)` | `DELETE /v1/environment/spec/pending` | — |
| `apply_pending(profile_id?)` | `POST /v1/environment/spec/pending/apply` | `EnvironmentSpecResponse` |
| `stream_spec(...)` | SSE `GET /v1/environment/spec/stream` | `EnvironmentStreamEvent` |

Incremental patch ops (`remove_custom_surface`, `remove_component`, etc.) are agent-internal via `cognition_environment_patch`. SDK integrators use `put_spec` for full spec replace.

---

## `components()`

| Method | HTTP | Types |
|--------|------|-------|
| `store_get(component_id, ...)` | `GET /v1/components/{id}/store` | `ComponentStoreGetResponse` |
| `store_set(component_id, key, request)` | `PUT /v1/components/{id}/store?key=` | `ComponentStoreSetRequest` |
| `store_list_keys(component_id, ...)` | `GET /v1/components/{id}/store/keys` | `ComponentStoreListResponse` |
| `store_get_key(component_id, key, ...)` | `GET /v1/components/{id}/store/{key}` | `ComponentStoreGetResponse` |
| `store_put_key(component_id, key, request)` | `PUT /v1/components/{id}/store/{key}` | `ComponentStoreSetRequest` |
| `store_delete_key(component_id, key, ...)` | `DELETE /v1/components/{id}/store/{key}` | `ComponentStoreDeleteResponse` |
| `runtime_tail_events(component_id, ...)` | `GET /v1/components/{id}/runtime/events` | `ComponentRuntimeEventsTailResponse` |
| `runtime_append_events(component_id, request)` | `POST /v1/components/{id}/runtime/events` | `ComponentRuntimeEventsRequest` |
| `runtime_complete_probe(component_id, probe_id, request)` | `POST .../probe/{probe_id}/result` | `ComponentRuntimeProbeResult` |

---

## `feeds()`

| Method | HTTP | Types |
|--------|------|-------|
| `list(profile_id?)` | `GET /v1/feeds` | `FeedListResponse` |
| `tail(feed_id, query)` | `GET /v1/feeds/{feed_id}/tail` | `FeedTailQuery` |
| `mark_read(feed_id, request)` | `POST /v1/feeds/{feed_id}/read` | `FeedReadRequest` |
| `stream(profile_id?)` | SSE `GET /v1/feeds/stream` | `FeedStreamEvent` |

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
- Environment patch semantics (`cognition_environment_patch` ops — no HTTP patch route; use `environment().put_spec`)
- Tauri app uses bridge commands for SSE when `WorkshopTransport` cannot stream directly

Track new wrappers in PRs that update `sdk-contract/manifest.yaml`.
