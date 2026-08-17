# HTTP API reference (Medousa Engine)

**Audience:** integrator

Base URL default: `http://127.0.0.1:7419`  
Override: `MEDOUSA_DAEMON_URL`

Types: [`medousa-types`](../../crates/medousa-types/) (`daemon_api`, `session`, `local`, …).  
SDK: [`docs/sdk/api-reference.md`](../sdk/api-reference.md).  
Generated publication: [`sdk-contract/openapi.json`](../../sdk-contract/openapi.json) (operation IDs from the declared router; regenerate with `UPDATE_API_CONTRACT=1 cargo test -p medousa --lib daemon::contract::tests::checked_in_contract_artifacts_match_generation`). Rust/Python helpers and Home/Tauri stream path ownership expand those generated operation tables; endpoint-shaped Tauri commands and `forge.ts` remain shims until Slice 4. The YAML accessor list in [`sdk-contract/manifest.yaml`](../../sdk-contract/manifest.yaml) is a known-incomplete shadow until that remaining cutover.  
Component notes: [component-daemon.md](../../architecture/component-daemon.md).

Subsystem guides: [interactive-streaming](interactive-streaming.md) · [artifacts](artifacts.md) · [vault](vault.md) · [calendar](calendar.md) · [workspace](workspace.md) · [forge](forge.md) · [agent-tools](agent-tools.md) · [runtime-config](runtime-config.md) · [extensions](extensions.md)

---

## Authentication

Protected requests must send exactly one `Authorization: Bearer <credential>`
header. This is required on loopback as well as LAN/Iroh: a loopback socket is
not caller identity. Missing credentials return `401 authentication_required`;
malformed, expired, revoked, or unknown credentials return `401
invalid_credential`; insufficient capability returns `403 forbidden`.
`401` responses include `WWW-Authenticate: Bearer realm="medousa"`.

Declared `/v1` failures use JSON `ApiErrorEnvelope` (`schema_version`, `code`,
`message`, `request_id`, `details`). Send `x-request-id` to correlate; otherwise
`request_id` is `unassigned`. Handler plaintext 4xx/5xx on declared routes are
wrapped into the same envelope. SSE and binary bodies are left unchanged.

Medousa, `medousa`, and `medousa tui` provision and load independent
`home-local`, `medousa-cli`, and `medousa-tui` credentials without exposing the
secrets to webview state or command-line arguments. External integrations must
pair, retain the issued bearer in an OS secret store, and configure their SDK
transport with it.

Anonymous access is limited to constant `/health` liveness, the bounded
`/pair/init` + `/pair/verify` ceremony while an operator pairing window is
active, and scoped preview URLs carrying their own short-lived grant. A supplied
but invalid bearer never falls back to anonymous access.

---

## Health & ops

| Method | Path | Types / response | SDK | CLI |
|--------|------|------------------|-----|-----|
| GET | `/health` | Constant liveness JSON (`status`, `apiVersion`) | — | readiness probes |
| GET | `/v1/health` | `HealthResponse` (protected) | `health().get()` | `medousa doctor` |
| GET | `/v1/stats` | `DaemonStatsResponse` | `http().get` | — |
| GET | `/v1/heartbeat/status` | `HeartbeatStatusResponse` | `http().get` | — |
| GET | `/v1/delivery/status` | `DeliveryHealthResponse` | `http().get` | — |
| GET | `/v1/continuations/status` | `ContinuationStatusResponse` | `http().get` | — |
| GET | `/v1/continuations/lineage/{turn_correlation_id}` | `TurnContinuationLineageResponse` | `http().get` | — |
| POST | `/v1/jobs/{job_id}/replay-and-resume` | `ReplayAndResumeResponse` | `http().post` | — |

Continuation replay is single-claim and reauthorizes the recorded profile
against the current session catalog. Records from older unversioned authority
formats, profiles that no longer have access, and delivery targets for another
session are abandoned rather than resumed. An accepted replay runs as a new
bounded execution with its own cancellation root and deadline.

`DaemonStatsResponse` also reports `active_turn_executions`, its process
high-water mark, and `missing_turn_context_invocations`. The final counter is a
fail-closed invariant signal: any non-zero increase means a provider or tool
leaf reached the runtime without admission context and was rejected before the
leaf future was polled.

Stasis dashboard mounted at `/dashboard` (HTML UI).

---

## Interactive chat (two-step)

`GET /v1/agent-modes` returns the registered modes and their runtime-derived
availability. Clients should disable unavailable modes and show the returned
reason instead of assuming that a protocol enum is ready to enter.

`GET`/`PUT /v1/agent-modes/policy` reads or updates the user's proposal policy.
`proposal_ttl_seconds` accepts 5–86,400 seconds. `auto_accept` is `never`,
`task`, or `all`; the default is `never` with a 30-second timeout.

| Method | Path | Types | SDK |
|--------|------|-------|-----|
| POST | `/v1/interactive/turn` | `InteractiveTurnRequest` → `InteractiveTurnResponse` (includes `stream_url`) | `interactive().start_turn` |
| GET | `/v1/interactive/turn/{turn_id}/stream` | SSE: `InteractiveTurnStreamEvent` | `interactive().stream` / `stream_reconnecting` |

**Stream query:** `GET …/stream?since=<seq>` (optional `u64`, default `0`). Replays events with `seq > since` from the **durable turn journal** on disk, then tails live events. Each SSE payload includes monotonic **`seq`** per turn — clients track the last seen `seq` and reconnect with `?since=` after drops.

See [interactive-streaming.md](interactive-streaming.md). **Do not** expect SSE on the POST itself.

`InteractiveTurnRequest.host_context` carries a typed, bounded editor, note, or
page snapshot separately from `prompt`. The daemon persists the human prompt as
written, stores host context as structured turn metadata, and projects that
metadata into model context. Clients must not append prompt wrappers. Host
context is advisory and never grants filesystem or vault authority.

`InteractiveTurnRequest.agent_mode` is a per-turn behavioral override,
independent of interactive/background ticket delivery. When omitted, the
daemon checks the active task lease, then the session selection, then defaults
to `general`. `coder` additionally requires an active Forge undertaking and
its turn-scoped authority for file, shell, and engineering tools; without a
binding it enters the restricted project-setup phase. Resolution is
deterministic and does not require an additional model call.

`InteractiveTurnRequest.code_project_setup_authorized` (also accepted by
`POST /v1/turns`) records that the principal explicitly selected a surface
action allowing unbound Coder to choose, bind, or create a project. The daemon
honors it only during Coder setup, projects it separately into runtime context,
and keeps the persisted human prompt unchanged. Omit it or send `false` for
ordinary turns.

### Registered client tools

Host integrations can register tools that execute in the host process while
the daemon owns the model turn. See [agent-tools.md](agent-tools.md#registered-client-tools)
for the protocol and surface-scoping rules.

| Method | Path | Types |
|--------|------|-------|
| POST | `/v1/clients/register` | `RegisterClientRequest` → `RegisterClientResponse` |
| GET | `/v1/clients/{client_id}/tools/next?wait_ms=…` | `ClientToolRequest` or `null` |
| POST | `/v1/clients/{client_id}/tools/{request_id}/result` | `ClientToolResultRequest` → `ClientToolResultResponse` |

### Sessions & turns

| Method | Path | Types | SDK |
|--------|------|-------|-----|
| POST | `/v1/sessions` | `CreateSessionRequest` → daemon-generated `CreateSessionResponse.session_id` | `MedousaClient.createSession` (TypeScript) |
| GET | `/v1/sessions` | `SessionHistoryListResponse` (`origin_surface`, `has_code_work` on each summary) | `sessions().list` |
| GET | `/v1/sessions/{session_id}/history` | `SessionHistoryResponse` | `sessions().history` |
| PUT | `/v1/sessions/{session_id}/name` | `SessionSetDisplayNameRequest` | `sessions().set_display_name` |
| GET | `/v1/sessions/{session_id}/agent-mode` | Effective selection and source | `sessions().agent_mode` |
| PUT | `/v1/sessions/{session_id}/agent-mode` | `SetSessionAgentModeRequest` | `sessions().set_agent_mode` |
| DELETE | `/v1/sessions/{session_id}/agent-mode?scope=session\|task` | Clear selection or lease | `sessions().clear_agent_mode` |
| GET | `/v1/sessions/{session_id}/agent-mode/proposals` | `AgentModeProposalListResponse` | `sessions().agent_mode_proposals` |
| PUT | `/v1/sessions/{session_id}/agent-mode/proposals/{proposal_id}` | `DecideAgentModeProposalRequest` | `sessions().decide_agent_mode_proposal` |
| GET | `/v1/sessions/{session_id}/code-binding` | Shared Forge undertaking binding | `sessions().code_binding` |
| PUT | `/v1/sessions/{session_id}/code-binding` | `SetSessionCodeBindingRequest` | `sessions().set_code_binding` |
| DELETE | `/v1/sessions/{session_id}/code-binding` | Clear shared undertaking binding | `sessions().clear_code_binding` |
| POST | `/v1/sessions/{session_id}/code-project` | `StartSessionCodeProjectRequest` → create, provision, and bind | `sessions().start_code_project` |
| DELETE | `/v1/sessions/{session_id}` | `SessionDeleteResponse` | `sessions().delete` |
| GET | `/v1/session-deletions/{deletion_id}` | `SessionDeleteResponse` | `http().get` |
| POST | `/v1/sessions/{session_id}/turns` | `SessionAppendTurnRequest` | `sessions().append_turn` |
| GET | `/v1/sessions/{session_id}/turns` | turn list | `http().get` |
| GET | `/v1/sessions/{session_id}/active-turn` | active turn ticket | `http().get` |
| POST | `/v1/sessions/{session_id}/active-turn` | cancel active turn | `http().post` |
| POST | `/v1/sessions/{session_id}/workshop/steer` | steer one exact bound-workshop generation (`work_id`, `message`) | `http().post` |
| POST | `/v1/turns` | create turn ticket | `http().post` |
| GET | `/v1/turns/{turn_id}` | turn ticket | `http().get` |

Workshop steering requires the exact `work_id` returned by the bound-workshop
handoff. A stale generation receives `409 Conflict` and cannot steer a newer
workshop for the same session.

`POST /v1/sessions` is the authority for new chat identifiers. Omit
`session_id`; current daemons return a `ses_` identifier with 128 bits of
randomness and reject caller-selected identifiers. Existing valid legacy IDs
remain usable on read, turn, and deletion routes during migration.

Deletion first persists a durable tombstone, then cancels active work and runs
the registered storage-surface inventory. `SessionDeleteResponse.status` is
`complete`, `retryable_partial`, `blocked`, or `deleting`; the compatibility
`deleted` field is true only for `complete`. Each surface reports only a bounded
reason class—never a raw path. Retry the same session DELETE after a partial
result; it reuses the returned `deletion_id`. The status can also be read from
`GET /v1/session-deletions/{deletion_id}`. Tombstoned sessions reject new
turns and other session-owned mutations.

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
| GET/PUT | `/v1/runtime/workers` | Worker capacity and preferred lane shares | `http().get/put` |
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
| GET | `/v1/feeds/{feed_id}/latest-good` | `feeds().latest_good` |
| POST | `/v1/feeds/{feed_id}/read` | `feeds().mark_read` |
| GET (SSE) | `/v1/feeds/stream` | `feeds().stream` |

Each `(profile, feed)` has an independent ordered persistence owner. Appends
write one framed record before publication; mark-read writes a synced cursor
whose event generation cannot exceed the committed log. Recovery ignores only
an incomplete final JSONL record and migrates legacy feed paths on the next
append.

The retained tail is capped at 200 events and 4 MiB, and one event may not
exceed 256 KiB. Logs compact after 400 records or 8 MiB and are read with a
16 MiB hard limit. These fixed limits make `tail` a recent-view API, not an
archive. Independent feeds may progress concurrently; disk work is globally
limited to 16 operations.

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
| GET/POST | `/v1/vault/notes` | List (`VaultNoteSummary[]`) / create notes |
| GET/PUT/DELETE | `/v1/vault/notes/{*note_path}` | Read / write / delete note |
| GET | `/v1/vault/tags` | List tags |
| GET | `/v1/vault/search` | Full-text search |
| GET | `/v1/vault/changes` | Note deltas since a vault generation |
| GET | `/v1/vault/backlinks` | Backlinks for path |

See [vault.md](vault.md).

---

## Native ChatGPT account authentication

These endpoints connect a ChatGPT account to the daemon-owned native Medousa
runtime. They do not use or modify the Codex CLI login. Tokens and the device
authorization secret never leave the daemon; clients receive only the user code,
an opaque login id, connection status, account id, and expiry.

| Method | Path | Types / purpose |
|--------|------|-----------------|
| GET | `/v1/auth/chatgpt` | `ChatGptOAuthStatusResponse` |
| POST | `/v1/auth/chatgpt/begin` | Start device authorization → `BeginChatGptOAuthResponse` |
| POST | `/v1/auth/chatgpt/complete` | Poll once with `CompleteChatGptOAuthRequest` → `CompleteChatGptOAuthResponse` |
| POST | `/v1/auth/chatgpt/refresh` | Refresh now → `ChatGptOAuthStatusResponse` |
| DELETE | `/v1/auth/chatgpt` | Revoke best-effort and delete local credentials → `DisconnectChatGptOAuthResponse` |

`complete` is intentionally non-blocking. While authorization is pending, the
response includes `retry_after_seconds`; clients should wait and call it again.
This makes the same flow usable when Home and the workshop daemon are on
different machines. The daemon refreshes within five minutes of expiry and
deduplicates concurrent refreshes. An upstream authentication failure permits
one refresh-and-retry; a permanent refresh failure changes status to
`reauth_required`.

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

Session creation and prompt requests may include `code_context`, a typed,
bounded snapshot of the user's active Code workspace (outcome, file,
cursor/selection, open files, diagnostics, and last verification). The daemon
formats this for the selected ACP provider; clients should not construct
provider-specific prompt wrappers.

When the Medousa conversation's effective mode is `coder`, session creation
must include its Forge `work_id`; otherwise the daemon returns `409 Conflict`.
The daemon resolves that id and forces the ACP process into the governed
worktree. General-mode ACP sessions may omit `work_id` for plain chat.

Native interactive turn and turn-ticket requests also accept `code_context`.
For Coder mode it is advisory editor state only: the daemon re-resolves the
undertaking, worktree, branch, baseline, dirty paths, and repository
instructions from Forge. UI-provided paths cannot select or escape the
governed worktree. Coder requires an active Forge undertaking. Each Coder turn
acquires a fenced Forge attempt, exposes only its mode-scoped coding tools,
records compact command receipts, interrupts turn-owned shell work on exit,
and releases the attempt without discarding worktree changes.

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

Guide: [workspace.md](workspace.md).

---

## Forge (undertakings)

Custody of intentional work episodes over a git target (vault or any repo). Distinct from workspace cards and vault Versions.

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/v1/forge/items` | Register |
| POST | `/v1/forge/items/start` | Register and provision in one operation |
| POST | `/v1/forge/repositories/inspect` | Inspect a repository path, commit readiness, and starting branch (`has_commits`, nullable `suggested_base_ref`) |
| GET, PUT | `/v1/forge/repositories` | Workshop repository recents and pins |
| GET | `/v1/forge/repositories/browse?path=…` | Scoped workshop directory/repository browser |
| GET, POST | `/v1/forge/repositories/provider` | Discover optional provider adapters or clone on the workshop |
| GET | `/v1/forge/items` | List. Unparameterized response is still a JSON array (compatibility window, catalog-backed, capped at 256). `?limit=&cursor=` returns `{ items, next_cursor, truncated }`. Queue-full Forge/Git work returns `503` / `overloaded`. |
| GET | `/v1/forge/items/{id}` | Get |
| POST | `/v1/forge/items/{id}/provision` | Provision env |
| POST | `/v1/forge/items/{id}/attempts` | Begin isolated attempt → lease plus exact `attempt_id`, `worktree`, and `branch` |
| GET | `/v1/forge/items/{id}/review?attempt_id=…` | Review all sealed candidates or select one exact attempt |
| GET | `/v1/forge/items/{id}/review/file?path=…&attempt_id=…` | Compare one file from the selected sealed attempt |
| POST | `/v1/forge/items/{id}/handoff` | Release the current executor while preserving its worktree |
| GET, POST | `/v1/forge/items/{id}/provider` | Discover or perform external repository review handoff |
| PUT | `/v1/forge/items/{id}/provider/context` | Attach HTTPS issue, PR, or ticket context |
| GET, POST | `/v1/forge/items/{id}/provider/comments` | Read review feedback or register a follow-up item |
| POST | `/v1/forge/leases/{lease_id}/heartbeat` | Heartbeat |
| POST | `/v1/forge/leases/{lease_id}/complete` | Seal |
| POST | `/v1/forge/leases/{lease_id}/interrupt` | Interrupt |
| POST | `/v1/forge/leases/{lease_id}/fail` | Fail |
| POST | `/v1/forge/items/{id}/decisions` | Review decision |
| POST | `/v1/forge/items/{id}/apply` | Apply disposition |
| POST | `/v1/forge/items/{id}/discard` | Discard |
| POST | `/v1/forge/items/{id}/run-script` | Script adapter |
| POST | `/v1/forge/items/{id}/export` | Export bundle |
| GET | `/v1/forge/items/{id}/tree` | Browse governed source tree |
| GET | `/v1/forge/items/{id}/changes` | Working-copy Changes (branch, upstream, conflict, file statuses) |
| GET, POST | `/v1/forge/items/{id}/changes/file` | Per-file working-copy vs baseline diff, or lease-fenced restore to baseline |
| POST | `/v1/forge/items/{id}/changes/file/hunk` | Lease-fenced single-hunk revert |
| POST | `/v1/forge/items/{id}/changes/fetch` | Fetch remotes |
| POST | `/v1/forge/items/{id}/changes/pull` | Fast-forward-only pull |
| POST | `/v1/forge/items/{id}/changes/push` | Non-force push of the Forge branch |
| POST | `/v1/forge/items/{id}/changes/sync` | Fetch → ff-only pull when behind → push when ahead |
| POST | `/v1/forge/items/{id}/changes/checkpoint` | Seal active lease for Review |
| GET | `/v1/forge/items/{id}/changes/history` | Commit history since baseline |
| GET | `/v1/forge/items/{id}/changes/blame` | Blame hunks for a path |
| POST | `/v1/forge/items/{id}/changes/conflict` | Resolve unmerged path (`ours`/`theirs`/`baseline`) |
| GET | `/v1/forge/items/{id}/search` | Repository search (`query`, optional `mode`, `case_sensitive`, `whole_word`, `include`, `exclude`, `include_ignored`, `scope`, `limit`, `cursor`; response may include `next_cursor`) |
| POST | `/v1/forge/items/{id}/search/replace` | Preview or apply digest-fenced repository replace (`dry_run`, `replacement`, search options, optional `paths`/`preconditions`/`lease_id`/`generation`) |
| GET | `/v1/forge/items/{id}/source?path=…` | Read governed source (full UTF-8, or read-only preview for binary/large/lossy with `encoding`/`preview`/`truncated`) |
| POST | `/v1/forge/items/{id}/source` | Create a governed source file or directory (`kind=directory` seeds `.gitkeep`) |
| PUT | `/v1/forge/items/{id}/source` | Save with digest conflict fencing |
| PATCH | `/v1/forge/items/{id}/source` | Rename with digest conflict fencing |
| DELETE | `/v1/forge/items/{id}/source` | Delete with digest conflict fencing |
| GET (SSE) | `/v1/forge/items/{id}/project-events?since=…` | Resumable path-aware source/Git events (`seq` cursor) |
| GET (SSE) | `/v1/forge/stream` | Undertaking list freshness |
| GET | `/v1/forge/items/{id}/workspace-state` | Restore editor tabs, drafts, and groups |
| PUT | `/v1/forge/items/{id}/workspace-state` | Persist lease-bound editor recovery state |
| GET | `/v1/forge/items/{id}/review` | Structured review synthesis, attribution, timeline, and comments |
| GET | `/v1/forge/items/{id}/review/file?path=…` | Compare one file between exact baseline and reviewed revisions |
| POST | `/v1/forge/items/{id}/review/file` | Reopen and restore one baseline text file while preserving reviewed evidence |
| GET, POST | `/v1/forge/items/{id}/review/comments` | List or add line-anchored review comments |
| PATCH, DELETE | `/v1/forge/items/{id}/review/comments/{comment_id}` | Resolve/edit or delete a review comment |
| POST | `/v1/forge/items/{id}/review/request-changes` | Request changes, reopen for another attempt, retain revision brief |
| GET | `/v1/forge/items/{id}/tasks` | Detect project commands (manifest + thin tasks.json) |
| POST | `/v1/forge/items/{id}/tasks/{task_id}/run` | Run a detected command and record its result |
| POST | `/v1/forge/items/{id}/tasks/{task_id}/runs` | Start a named, cancellable project run |
| GET/DELETE | `/v1/forge/items/{id}/task-runs/{run_id}` | Poll or cancel a project run (live bounded output + locations) |
| GET (SSE) | `/v1/forge/items/{id}/task-runs/{run_id}/events?since=…` | Stream task output, locations, readiness, and terminal state |
| POST | `/v1/forge/items/{id}/task-runs/{run_id}/preview` | Mint tokenized private preview path |
| ANY | `/v1/forge/preview/{token}/…` | Proxy to workshop loopback port |
| GET | `/v1/forge/evidence/{evidence_id}/patch` | Read a bounded page of the sealed patch |
| GET | `/v1/forge/evidence/{evidence_id}/commands` | Read a bounded page of the sealed command log |
| GET | `/v1/forge/evidence/{evidence_id}/receipts` | Read typed compact evidence provenance; raw payloads are excluded |

Guide: [forge.md](forge.md).

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

`LocalEngineStatus.phase` is the lifecycle signal. It is one of
`unavailable`, `cold`, `startingWorker`, `loading`, `ready`, `busy`,
`draining`, `unloading`, or `failed`; `loaded` remains for compatibility.
Older status payloads without `phase` are inferred as unavailable, cold, or
ready by current clients.

Current workers expose a private loopback handshake at `/_medousa/status`.
`LocalEngineStatus.worker` records the protocol version, generation ID, PID,
start time, exact model, aggregate verified-artifact digest, recipe revision,
worker-binary digest, runtime identity, and compiled backends. A listening port
is not readiness: supervisors accept only a compatible `ready` or `busy`
handshake, verify the spawned/tracked PID and requested model, and report an
occupied or incompatible listener as `failed`. Normal workers refuse model
allocation when the installed artifact or worker binary cannot be identified by
SHA-256. Unload targets the confirmed generation, waits up to ten seconds, and
uses forced process termination as the final memory-reclamation fallback.

`LocalResourceAdmission` is the shared pre-load decision record: current and
total host memory, system reserve, hardware-tier cap, steady/conversion/peak
estimates, critical-pressure threshold, and the explicit context/batch recipe.
It also records the selected accelerator backend, device index/UUID/name,
telemetry source, total/available device memory, device reserve, admissible
device memory, dynamic device budget, estimated device peak, and whether the
device envelope was enforced. Dynamic WDDM/Vulkan/working-set budgets include
current process usage, so their remaining headroom is `budget - processUsage`;
they outrank physical free-memory counters. Medousa reads WDDM budget and usage
from DXGI on Windows and Vulkan heap budget and estimated usage from
`VK_EXT_memory_budget`. Vulkan evidence remains scoped to Vulkan allocations
and is not substituted for CUDA or HIP process memory. Native vendor APIs then
outrank CLI fallbacks for the same device. An authoritative budget with missing
process usage fails closed while keeping the missing value nullable. The final
`admissibleMb` is the smaller of the host and enforced
device envelopes. Missing device counters stay nullable and produce an explicit
host-only decision rather than a fabricated device capacity. NVIDIA admission
dynamically loads the driver-provided NVML library—without a
CUDA toolkit dependency—to identify the device and read physical and
current-process memory. `nvidia-smi` remains a lower-priority fallback. Under
Windows WDDM, NVML's unavailable process-memory sentinel remains `null`; it is
never converted into capacity. Linux AMD admission follows the same native-first
rule with AMD SMI. Its ABI major is validated before any versioned structure is
read, and incompatible libraries fall back to `amd-smi` JSON rather than risking
misinterpreted telemetry. Before allocating,
the worker converts an admitted decision into a short-lived cross-process
activation lease; concurrent loads cannot spend the same host or device
headroom, and dead-process leases are reclaimed. The current safe baseline is
4K context, batch/concurrency 1. When a matching content-free benchmark
calibration exists, the record also exposes its sample count, observed host and
device peaks, static estimate, and margin. Calibrated high-water marks may raise
the enforced peak but never lower the static estimate. The worker exits after
five idle minutes and terminates under critical host-memory pressure.

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

Invite generation and inspection routes require a local-app or paired bearer.
Only `POST /pair/init` and `POST /pair/verify` are anonymous, and only an
operator-issued, unexpired, single-use invite can enter the ceremony.

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
| DELETE | `/pair/{pairing_id}` — `admin.identity`, or the paired bearer revoking itself |

Cookbook: [mobile-and-lan.md](../cookbook/mobile-and-lan.md)

## Local credential operations

These native-only administration routes require `admin.identity`. They never
return bearer secrets.

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/v1/admin/local-credentials` | Credential ids/generations/status plus bounded lifecycle diagnostics |
| POST | `/v1/admin/local-credentials/{name}/rotate` | Install the next generation and revoke the old one |
| DELETE | `/v1/admin/local-credentials/{name}` | Revoke one first-party client credential |

Supported names are `home-local`, `medousa-cli`, and `medousa-tui`. Successful
rotation/revocation affects new requests immediately and closes matching
long-lived daemon streams.

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
2. `GET` the returned `stream_url` as SSE. Use `Accept: text/event-stream` for
   v1 or `Accept: text/event-stream; medousa-version=2` for the typed v2
   envelope; both retain `?since=<seq>` replay semantics.

More: [integrate-without-the-app.md](../cookbook/integrate-without-the-app.md)
