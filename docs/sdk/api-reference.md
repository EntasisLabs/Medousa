# Medousa SDK — API reference

**Audience:** integrator

Full overview: [README.md](README.md). HTTP routes: [../engine/http-api.md](../engine/http-api.md).

Declared-router publication: [`../../sdk-contract/openapi.json`](../../sdk-contract/openapi.json) (regenerated from `DeclaredRouter`; validated by `scripts/check-api-contract.sh`). SDK helpers expand generated operation IDs rather than embedding `/v1` paths.

Rust async methods require `medousa-sdk` feature `async` (default). SSE requires `sse` (default). Python is async-first with accessor-based sync client.

---

## `health()`

| Method | HTTP | Response type |
|--------|------|---------------|
| `get()` | `GET /v1/health` | `HealthResponse` |

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

Forge undertaking custody and governed source/workspace operations currently use
this generic HTTP client rather than a dedicated typed SDK accessor. See the
[Forge engine guide](../engine/forge.md) and the
[HTTP route index](../engine/http-api.md#forge-undertakings).

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
| `search_transcripts(query, limit)` | `GET /v1/sessions/search?q=&limit=` | `SessionTranscriptSearchResponse` |
| `history(session_id)` | `GET /v1/sessions/{id}/history` | `SessionHistoryResponse` |
| `set_display_name(session_id, name)` | `PUT /v1/sessions/{id}/name` | `SessionSetDisplayNameRequest` |
| `agent_mode(session_id)` | `GET /v1/sessions/{id}/agent-mode` | `SessionAgentModeResponse` |
| `set_agent_mode(session_id, request)` | `PUT /v1/sessions/{id}/agent-mode` | Persist a session selection or task lease |
| `clear_agent_mode(session_id, scope)` | `DELETE /v1/sessions/{id}/agent-mode` | Clear a session selection or task lease |
| `agent_mode_proposals(session_id)` | `GET /v1/sessions/{id}/agent-mode/proposals` | List pending and resolved mode suggestions |
| `decide_agent_mode_proposal(session_id, proposal_id, accept)` | `PUT /v1/sessions/{id}/agent-mode/proposals/{proposal_id}` | Accept or deny a pending suggestion |
| `code_binding(session_id)` | `GET /v1/sessions/{id}/code-binding` | Read the conversation's shared Forge undertaking |
| `set_code_binding(session_id, work_id)` | `PUT /v1/sessions/{id}/code-binding` | Bind Home and editor surfaces to the same undertaking |
| `clear_code_binding(session_id)` | `DELETE /v1/sessions/{id}/code-binding` | Remove the shared undertaking binding |
| `start_code_project(session_id, request)` | `POST /v1/sessions/{id}/code-project` | Create, provision, and bind a repository-backed or blank project |
| `append_turn(session_id, request)` | `POST /v1/sessions/{id}/turns` | `SessionAppendTurnRequest` |
| `delete(session_id)` | `DELETE /v1/sessions/{id}` | `SessionDeleteResponse` |
| `list_turns(session_id)` | `GET /v1/sessions/{id}/turns` | `SessionHistoryResponse` |
| `active_turn(session_id)` | `GET .../active-turn` | active turn ticket |
| `cancel_active_turn(session_id)` | `POST .../active-turn` | cancel |

`delete` is complete only when `response.status == "complete"` and
`response.deleted` is true. Retry the same session ID for
`retryable_partial`; the daemon reuses `deletion_id` and replaces successful
per-surface results. Raw HTTP clients can query
`GET /v1/session-deletions/{deletion_id}`.

---

## `interactive()`

| Method | HTTP | Types |
|--------|------|-------|
| `start_turn(request)` | `POST /v1/interactive/turn` | `InteractiveTurnRequest` → `InteractiveTurnResponse` |
| `stream(stream_url)` | SSE from `stream_url` | `InteractiveTurnStreamEvent` stream |
| `stream_v2(stream_url)` | negotiated SSE from `stream_url` | `TurnStreamEnvelopeV2` stream |
| `stream_turn(request)` | start + SSE | combined helper |
| `stream_reconnecting_v2(stream_url)` | negotiated SSE with `?since=` replay | `TurnStreamEnvelopeV2` stream (recommended) |
| `stream_reconnecting_v2_with_policy(stream_url, policy)` | negotiated SSE with custom `ReconnectPolicy` | `TurnStreamEnvelopeV2` stream |
| `stream_turn_reconnecting_v2(request)` | start + typed reconnecting SSE | combined helper (recommended) |
| `stream_reconnecting*`, `stream_turn_reconnecting` | legacy SSE replay | frozen `InteractiveTurnStreamEvent` compatibility helpers |
| `cancel(session_id)` | `POST /v1/sessions/{id}/active-turn` | cancel active turn |

Set `InteractiveTurnRequest.code_project_setup_authorized` only after the
principal explicitly chooses a client action that allows unbound Coder to
choose, bind, or create a project. It does not expand authority on bound or
non-Coder turns, and it is stored separately from the human prompt.

**Client helpers** (`stream_reconnecting*`, `stream_turn_reconnecting*`) are not separate HTTP routes — they track `event.seq`, reconnect with `?since=<last_seq>`, and apply bounded backoff + overlap guard. See `medousa_sdk::ReconnectPolicy` and `medousa_sdk::stream_path_with_since`.

Both Rust (`sse` feature) and Python ship built-in SSE clients — [interactive-streaming.md](interactive-streaming.md).

The dependency-free TypeScript `@medousa/client` exposes
`streamTurnV2(response, options)` as its recommended start-response +
reconnecting stream helper. It negotiates the v2 media type and yields
`TurnStreamEnvelopeV2`. Conversation surfaces should use
`createTurnStreamProjectionState()` with `projectTurnStreamEvent()` for the
shared exhaustive projection, and may use `isTurnStreamTerminal()` or
`isBackgroundHandoffEvent()` for control flow. The older `streamTurn()` method
is a frozen v1 compatibility adapter for the support window, not a first-party
extension point.

---

## External-surface client tools

Browser, editor, and vault hosts can register tools that remain implemented in
their native runtime while the daemon owns the agent turn. The dependency-free
TypeScript adapter exposes these helpers directly; other SDKs can use the HTTP
routes below.

| Method | HTTP | Purpose |
|--------|------|---------|
| `registerClient(request)` | `POST /v1/clients/register` | Advertise surface-scoped tool definitions |
| `nextClientToolRequest(client_id, wait_ms)` | `GET /v1/clients/{client_id}/tools/next` | Long-poll an invocation |
| `completeClientToolRequest(client_id, request_id, result)` | `POST /v1/clients/{client_id}/tools/{request_id}/result` | Return output or an error |

The client-pull bridge is designed for hosts that cannot accept inbound
connections. Registrations and pending calls are bounded; only read-only
browser snapshot support is currently shipped.

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

`CreateAgentSessionRequest` and `AgentSessionPromptRequest` accept an optional
`CodeIntentContext`. Prefer it over embedding workspace paths, selections, or
diagnostics into provider-specific prompt text.

For a conversation in Coder mode, `create_session` requires the bound Forge
`work_id` and returns `409 Conflict` when it is absent. General-mode external
agent chats may remain unbound.

### Native ChatGPT account connection

Until a dedicated SDK accessor lands, integrations use the SDK's raw HTTP
client for the daemon-owned device flow:

| HTTP | Types |
|------|-------|
| `GET /v1/auth/chatgpt` | `ChatGptOAuthStatusResponse` |
| `POST /v1/auth/chatgpt/begin` | `BeginChatGptOAuthResponse` |
| `POST /v1/auth/chatgpt/complete` | `CompleteChatGptOAuthRequest` → `CompleteChatGptOAuthResponse` |
| `POST /v1/auth/chatgpt/refresh` | `ChatGptOAuthStatusResponse` |
| `DELETE /v1/auth/chatgpt` | `DisconnectChatGptOAuthResponse` |

The SDK client must poll `complete` according to `retry_after_seconds`; it must
not persist the returned user code or attempt to obtain daemon token material.

---

## `runtime()`

| Method | HTTP | Types |
|--------|------|-------|
| `agent_modes()` | `GET /v1/agent-modes` | `AgentModeListResponse` |
| `agent_mode_transition_policy()` | `GET /v1/agent-modes/policy` | Read proposal timeout and auto-accept policy |
| `set_agent_mode_transition_policy(policy)` | `PUT /v1/agent-modes/policy` | Update proposal timeout and auto-accept policy |
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
| `list_changes(query)` | `GET /v1/vault/changes` | `VaultChangesResponse` |
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
| `latest_good(feed_id, query)` | `GET /v1/feeds/{feed_id}/latest-good` | `FeedLatestGoodQuery` |
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

`InteractiveTurnRequest.surface` advertises independent rendering capabilities.
Use `supports_liquid_markdown` for clients that hydrate Liquid embeds; do not set
`supports_ui_artifacts` unless the client also implements HTML/scene artifact presentation.

Regenerate Python types after Rust DTO changes:

```bash
cargo run -p medousa-types-schema
python scripts/gen-python-types.py
```

See [python.md](python.md).

---

## Remaining gaps (use `http()`)

- Runtime worker capacity (`GET/PUT /v1/runtime/workers`)
- Identity, grapheme, workflows (full surface)
- Ingest SSE stream
- Environment patch semantics (`cognition_environment_patch` ops — no HTTP patch route; use `environment().put_spec`)
- Tauri app uses bridge commands for SSE when `WorkshopTransport` cannot stream directly

Track new wrappers in PRs that regenerate `sdk-contract/openapi.json` from the declared router.
