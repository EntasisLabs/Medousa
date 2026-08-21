# Forge

**Audience:** integrator

Forge is Medousa’s **undertaking custody** layer: intentional work episodes over
versionable material (a vault with Versions enabled, or any git repository),
with governed environments, lease-fenced attempts, sealed evidence, human
review, and durable dispositions.

It is **not** vault Versions (material memory), **not** the Work board
(`/v1/workspace/cards` — runtime activity projection), and **not** Stasis
(durable job runtime). Agents are optional executors; humans and scripts are
valid too.

Verbs:

| System | Verb |
|--------|------|
| Versions | remember material |
| Forge | own undertakings |
| Stasis | keep labor alive |
| Work board | project in-flight activity |

Architecture notes: [v0.7.0-forge-plan.md](../../architecture/v0.7.0-forge-plan.md).

Storage root: `{MEDOUSA_DATA_DIR}/forge` (events + evidence outside the worktree).

H06 scaling notes (Implementing — not Validated; see architecture H06 acceptance matrix):

- Scaffolding aims for in-memory per-item tails and a catalog projection for listings.
- `GET /v1/forge/items` without query params still returns an array (compatibility window, catalog-backed, capped). `?limit=&cursor=` returns `{ items, next_cursor, truncated }`.
- Forge/Git work is intended to admit through a bounded execution service. Queue-full should return `503` / `overloaded`. Do not call blocking Forge/Git from async code without that service.
- Slug uniqueness scaffolding uses a reservation journal rather than a full-item scan; durability/repair evidence is still open.
- Coder logical checkpoints are being separated from worktree audits; resume must require an exact generation-fenced observation once observation fencing is complete.
- v1 JSONL readers remain for rollback. Framed log v2 and migration are scaffolding until later cars close acceptance.

Contributor rule: do not call blocking Forge, Git, filesystem, or process waits from async handlers. Admit work through `ForgeExecutionService` (`run` / `run_on_repo` / `run_async` + `supervise_git`). Queue-full must return typed overload, never inline fallback.

Coder safe boundaries: `persist_boundary` and `mark_status` should write logical state only. `persist_current` and `latest_safe_resume` must obtain a current exact observation via capture → observe → recheck. Incomplete or unknown denies automatic resume.

---

## HTTP API

Base path: `/v1/forge`. Types are `medousa-forge` serde models (`WorkItem`,
`ExecutionLease`, `ReviewDecision`, …).

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/v1/forge/items` | Register undertaking (`repo_path`, `base_ref`, optional `policy`) |
| POST | `/v1/forge/items/start` | Register and provision a project in one operation |
| POST | `/v1/forge/repositories/inspect` | Resolve a folder to its Git root and infer branch/status metadata |
| GET, PUT | `/v1/forge/repositories` | List recent/pinned workshop repositories or update a pin |
| GET | `/v1/forge/repositories/browse?path=…` | Browse directories inside daemon-owned workshop places |
| GET, POST | `/v1/forge/repositories/provider` | Discover optional GitHub/GitLab CLI adapters or clone into a daemon-scoped workshop folder |
| GET | `/v1/forge/items` | List items |
| GET | `/v1/forge/items/{id}` | Load item |
| GET | `/v1/forge/items/{id}/source?path=…` | Read governed source; UTF-8 edits return full content, while binary/large/lossy files return a read-only preview with `encoding`/`preview`/`truncated` |
| POST | `/v1/forge/items/{id}/source` | Lease-fenced source-file or directory creation (`kind=directory` seeds `.gitkeep`) |
| PUT | `/v1/forge/items/{id}/source` | Lease-fenced source save with digest conflict detection |
| PUT | `/v1/forge/items/{id}/source/batch` | Atomic digest-fenced writes to existing text files |
| PUT | `/v1/forge/items/{id}/source/workspace-edit` | Atomic ordered text/create/rename/delete workspace edit with digest-or-absence preconditions |
| PATCH | `/v1/forge/items/{id}/source` | Lease-fenced source rename with digest conflict detection |
| DELETE | `/v1/forge/items/{id}/source` | Lease-fenced source deletion with digest conflict detection |
| GET (SSE) | `/v1/forge/items/{id}/project-events?since=…` | Resumable path-aware source/Git project events for one work item |
| GET (SSE) | `/v1/forge/stream` | Live undertaking list freshness (state/kind only; no path cursor) |
| GET | `/v1/forge/items/{id}/tree` | List tracked and unignored repository files (bounded to 20,000) |
| GET | `/v1/forge/items/{id}/changes` | Working-copy Changes: branch, upstream ahead/behind, conflict flag, dirty/merge flags, and changed-file statuses |
| GET, POST | `/v1/forge/items/{id}/changes/file` | Per-file working-copy vs baseline diff (`GET`) or lease-fenced restore to baseline (`POST`) |
| POST | `/v1/forge/items/{id}/changes/file/hunk` | Lease-fenced revert of one diff hunk |
| POST | `/v1/forge/items/{id}/changes/fetch` | Fetch remotes for the governed worktree |
| POST | `/v1/forge/items/{id}/changes/pull` | Fast-forward-only pull |
| POST | `/v1/forge/items/{id}/changes/push` | Non-force push of the Forge branch |
| POST | `/v1/forge/items/{id}/changes/sync` | Fetch, then ff-only pull when behind, then push when ahead |
| POST | `/v1/forge/items/{id}/changes/checkpoint` | Seal the active lease for Review (same as lease complete) |
| GET | `/v1/forge/items/{id}/changes/history` | Commits since the project baseline |
| GET | `/v1/forge/items/{id}/changes/blame` | Line blame for one path |
| POST | `/v1/forge/items/{id}/changes/conflict` | Resolve unmerged path (`ours` / `theirs` / `baseline`) and clear conflict state |
| GET | `/v1/forge/items/{id}/search?query=…` | Repository search (`literal`/`regex`, case/whole-word, include/exclude globs, `scope=all\|changed`, `limit`, `cursor` pagination; bounded to 500 hits; includes untracked, honors ignore by default) |
| POST | `/v1/forge/items/{id}/search/replace` | Preview (`dry_run=true`) or apply digest-fenced repository replace; optional `paths` subset and `preconditions` |
| GET, PUT | `/v1/forge/items/{id}/workspace-state` | Restore/preserve open files, cursor positions, bounded dirty drafts, contextual Code layout, and task/output references |
| GET | `/v1/forge/items/{id}/review` | Structured outcome, risk, verification, attribution, timeline, comments, and changed-file summary |
| GET | `/v1/forge/evidence/{evidence_id}/receipts` | Sealed compact Coder evidence provenance (never raw payloads) |
| GET | `/v1/forge/items/{id}/tasks` | Manifest-derived checks plus safe `.vscode/tasks.json` entries |
| POST | `/v1/forge/items/{id}/tasks/{task_id}/runs` | Start a named, cancellable project run |
| GET | `/v1/forge/items/{id}/task-runs?limit=…` | List bounded active/recent run summaries for reconnect hydration |
| GET/DELETE | `/v1/forge/items/{id}/task-runs/{run_id}` | Poll or gracefully stop a project run (includes live bounded output, locations, readiness, and PTY attach state); `?force=true` force-stops |
| GET (SSE) | `/v1/forge/items/{id}/task-runs/{run_id}/events?since=…` | Stream task output chunks, incremental locations, readiness, and terminal state (`?since=` replay) |
| POST | `/v1/forge/items/{id}/task-runs/{run_id}/preview` | Mint a tokenized private preview path for a ready run |
| ANY | `/v1/forge/preview/{token}/…` | Reverse-proxy to workshop `127.0.0.1:{port}` (token-gated; no public app bind) |
| GET | `/v1/forge/items/{id}/tests?attempt_id=…` | Discover addressable project tests, optionally pinned to one exact attempt |
| GET | `/v1/forge/items/{id}/review/file?path=…` | Exact baseline-to-reviewed file comparison with structured hunks |
| POST | `/v1/forge/items/{id}/review/file` | Reopen work and restore one text file to its baseline while retaining the reviewed checkpoint |
| GET, POST | `/v1/forge/items/{id}/review/comments` | List or add line-anchored review comments bound to sealed evidence |
| PATCH, DELETE | `/v1/forge/items/{id}/review/comments/{comment_id}` | Resolve/edit or delete a review comment |
| POST | `/v1/forge/items/{id}/review/request-changes` | Record changes-requested feedback, reopen to Ready, and keep a revision brief for the next attempt |
| GET | `/v1/forge/items/{id}/tasks` | Detect safe project commands from repository manifests |
| POST | `/v1/forge/items/{id}/tasks/{task_id}/run` | Run a detected command and stage its result into active evidence |
| POST | `/v1/forge/items/{id}/provision` | Create governed worktree env |
| POST | `/v1/forge/items/{id}/attempts` | Begin attempt (default executor `human`) → lease |
| POST | `/v1/forge/items/{id}/handoff` | Preserve the worktree and release the current lease for another executor |
| GET, POST | `/v1/forge/items/{id}/provider` | Discover repository handoff state or push and create/update an external review |
| PUT | `/v1/forge/items/{id}/provider/context` | Save bounded HTTPS issue, PR, or ticket links |
| GET, POST | `/v1/forge/items/{id}/provider/comments` | Read supported review feedback or turn one comment into a new Forge item |
| POST | `/v1/forge/leases/{lease_id}/heartbeat` | Liveness (`generation` required) |
| POST | `/v1/forge/leases/{lease_id}/complete` | Seal checkpoint + evidence |
| POST | `/v1/forge/leases/{lease_id}/interrupt` | Interrupt; work preserved |
| POST | `/v1/forge/leases/{lease_id}/fail` | Fail attempt; return to Ready |
| POST | `/v1/forge/items/{id}/decisions` | Record evidence-bound review decision |
| POST | `/v1/forge/items/{id}/apply` | Apply decision (`PreserveBranch` / `FastForwardOnly` / `ExportPatch`) |
| POST | `/v1/forge/items/{id}/discard` | Discard env (worktree then branch) |
| POST | `/v1/forge/items/{id}/run-script` | Reference script executor (`argv`) |
| POST | `/v1/forge/items/{id}/export` | Portable bundle to `destination` |

### Project event stream

`GET /v1/forge/items/{id}/project-events?since=<seq>` is the authoritative
cursor for Code buffer reconciliation. Each SSE `project` payload is a
`ForgeProjectEvent` with monotonic `seq`, `work_id`, `kind`
(`created` / `changed` / `renamed` / `deleted` / `git_status` / `snapshot`),
optional `path` / `old_path` / `digest`, and `updated_at`.

Reconnect with `?since=<last_seq>` to replay the bounded in-memory journal
(`seq > since` for that work item), then tail live events. Lagged subscribers
re-snapshot from the journal rather than inventing gaps. Events come from:

- lease-fenced source create/save/batch/workspace-edit/rename/delete routes;
- a debounced worktree filesystem watcher (ignores `.git/**`).

Home's Code editor consumes this stream for all open buffers: clean tabs accept
the project version, dirty tabs keep the draft and offer compare/rebase,
renames/deletes recover tab identity, and the language client receives
`workspace/didChangeWatchedFiles` (plus create/rename/delete file notifications).

`GET /v1/forge/stream` remains the coarse undertaking list channel (work id,
state, event kind). It does not carry paths or a replay cursor.

### Project task-run output stream

`GET /v1/forge/items/{id}/task-runs/{run_id}/events?since=<seq>` streams live
stdout/stderr for a named project run. Each SSE `task` payload is a
`ProjectTaskOutputEvent` with monotonic `seq`, `run_id`, `kind`
(`output` / `state` / `gap`), optional `stream`/`text` for chunks, optional
incremental `locations`, and optional `state`/`result` when status changes.

If `since` predates retained replay, or a live receiver falls behind, the stream
emits `kind=gap` with structured `available_from`, the next retained sequence.
Clients must refetch the run snapshot and resume from that sequence; absence of
output events must not be interpreted as a contiguous replay.

Long-running / background tasks may emit `state=ready` (no `result`) when
output matches a built-in readiness pattern or a task's `ready_pattern` from
`.vscode/tasks.json`. Cancel may emit an early `state=cancelled` without
`result`; the stream stays open until the process exits and a terminal `state`
event includes the final result.

`GET …/tasks` merges manifest-detected commands with a thin
`.vscode/tasks.json` import (`npm` / `shell` / `process`, optional inline
problem-matcher `pattern`, background `endsPattern`). Full VS Code matcher
catalogs, `dependsOn`, and presentation panels are not supported.
Detected descriptors include a repository-relative `root`; discovery uses the
Git-visible file set, caps the number/depth of nested roots and tasks, and keeps
root-level IDs stable. Nested IDs include their root. Before spawning, Forge
canonicalizes the selected root, rejects absolute/traversing/symlink escapes,
and reports output locations relative to the repository even though the process
runs from the nested directory.

The additive descriptor contract is currently `version: 1`. It also reports
`source`, `interactive`, `background`, `default_rank`, aggregate `available`,
and structured `requirements` entries. Executable requirements name missing
workshop-host commands; JavaScript package requirements also detect absent
installed dependencies at the nearest lockfile root. Both provide exact repair
copy. Run routes revalidate health and return a conflict before starting an
unavailable task. Older clients can continue using the original fields, and
Home treats absent health fields from older daemons as available.

`GET …/task-runs/{run_id}` also returns bounded live `stdout`/`stderr`,
`output_truncated`, `locations`, `ready_url` (when a background task becomes
ready), and `next_seq` while the process is still running (and after exit for
replay). Each stdout/stderr tail caps at 256 KiB. Chunk replay keeps at most 400
events and 1 MiB. The registry admits at most 128 runs and 64 MiB of run
reservations, retains at most 64 terminal runs, and expires terminal entries
after 10 minutes. Active runs are never removed by terminal retention.
Snapshots also carry `test_id`, `started_at`, and `finished_at`. The collection
route returns at most 64 summaries (20 by default), newest first, without output
bodies. Its envelope reports active/terminal/retained counts, truncation, the
terminal limit/TTL, and a monotonic daemon-registry eviction count. Home fetches
the exact selected snapshot, resumes SSE from `next_seq`,
and falls back to a persisted active/recent run reference when connected to an
older daemon without the collection route.

Interactive, background, and long-running tasks are hosted directly in one
`medousa-session` PTY rather than launched once for Output and again for
Terminal. Their run snapshots and summaries include `session_id` and the
daemon-relative `attach_path`; every Home Terminal attachment is a peer on that
same workshop process. Cancellation first publishes `stopping` and sends an
interrupt. Repeating Stop with `?force=true` kills the hosted process. A ready
run also retains its tokenized `preview_path` alongside `ready_url` for Web
reattach; preview grants outlive the bounded run-registry TTL.

When readiness fires, the daemon may extract a loopback URL (`localhost` /
`127.0.0.1` / `0.0.0.0`) into `ready_url` and mint a short-lived preview token.
`POST …/task-runs/{run_id}/preview` returns `{ preview_path, token, ready_url,
port }`. Home opens co-located previews at `ready_url` directly; remote Homes
open `{daemon}/v1/forge/preview/{token}/…`, which reverse-proxies to
`127.0.0.1:{port}` on the workshop without binding the app publicly. WebSocket
HMR through the proxy is best-effort; prefer Stop/restart for broken live reload.

Forge records a canonical `active_attempts` set and resolves every lease
mutation against its addressed attempt. The legacy singular `active_attempt`
projection remains serialized for snapshot/client compatibility during the
Slice 5 migration. `Ready` and `Executing` work can admit another executor;
every active attempt has a distinct private worktree and branch.

Forge uses isolated attempts. The first isolated attempt forks a private branch
and worktree from the undertaking staging worktree, reproducing its
tracked, staged, deleted, binary, and regular untracked dirty state without
mutating the staging directory. Unsafe paths and untracked symlinks fail the
fork and remove its partial branch/worktree. The attempt owns that environment;
seal captures it, interruption preserves it, reconciliation recognizes it, and
discard reclaims it. A restarted turn reuses that preserved workspace after
verifying its Git root and branch, so unfinished edits survive without creating
one worktree per turn. When peers run concurrently, each new peer receives a
fresh isolated worktree rather than reusing an active environment.

`POST /v1/forge/items/{id}/attempts` returns the full fenced lease plus top-level
`attempt_id`, `worktree`, and `branch` fields. Forge item projections expose the
current lease-owned workspace through `environment`; the durable item still
retains its original staging anchor internally.

Forked attempt environments also expose an optional `derived_from` object with
the source `branch`, source `generation`, and immutable `forked_at` timestamp.
The field is absent for staging environments and snapshots created before
lineage metadata was introduced.

Sealing, interruption, and failure are peer-safe. Ending one attempt leaves the
item `Executing` while any healthy lease remains. After the last active attempt
ends, sealed evidence yields `AwaitingReview`; otherwise the item returns to
`Ready`. Seal journal entries carry `attempt_id`, allowing restart recovery to
complete or interrupt exactly the affected attempt.

`GET /v1/forge/items/{id}/review` returns `candidates` for every sealed attempt.
Pass `attempt_id` to that endpoint and to `/review/file` to select the exact
manifest, branch, worktree, and diff. Decisions already bind to the selected
attempt, evidence digest, baseline, and reviewed head.

### Concurrent Coder claims

Private attempt worktrees prevent direct filesystem races, but agents can still
touch the same logical code or external resource. Before every Coder tool call,
the runtime infers `read`, `write`, or `verify` claims from the governed tool and
its targets. A model's required `intent` explains the operation; it cannot
choose, weaken, or omit the inferred claims.

Worktree-absolute editor and LSP paths are canonicalized to undertaking-relative
file identities. Ordinary file overlaps remain admissible across isolated
attempts and are surfaced to every affected agent through the shared ambient
frame, causal activity events, and ranked pointers. Source mutations retain
their existing digest checks, while integration remains bound to exact evidence
and Git baselines.

Hazardous resources are serialized before the underlying tool runs. These
include dependency lockfiles, migration ordering, generated artifact sets,
shared Git references and indexes, databases, ports/services, deployments, and
publishing operations inferred from file paths or shell commands. A conflicting
call returns a structured `coder_claim_conflict` result with the holder's agent,
attempt, tool, intent, and claim expiry plus an actionable retry decision.

Write claims remain active while the Coder turn keeps heartbeating; read and
verify claims release when the call finishes. Long-running calls renew claims
every 30 seconds. Claims expire after two minutes without renewal and are
released immediately when an agent leaves. The activity index bounds active
claims and historical events; it stores coordination metadata, never source or
command-output payload bodies.

Export writes on the daemon/workshop filesystem. `destination` must be absent
or an empty directory; a non-empty destination returns `409` and is never
overwritten.

Repository discovery is daemon-owned. The catalog lives under the Forge data
root, records at most 50 recent/pinned paths, and never treats a Home-local
picker as authority for a remote workshop. Browsing canonicalizes every path,
rejects paths outside the workshop home/common repository places, hides dot
directories, and returns at most 500 folders per response. Inspection reports
Git branch/remotes, clean or dirty state, and active Forge projects targeting
the same canonical repository so clients can offer Continue existing / Start
another change before provisioning.

Provider adapters are optional daemon-side ports. Capability discovery checks
for the GitHub (`gh`) and GitLab (`glab`) CLIs on the connected workshop.
Clone accepts a validated provider namespace such as `owner/project`, derives
the destination name, refuses an existing destination, and only writes beneath
the same daemon-owned browse roots. Authentication remains with the provider
CLI; the HTTP contract does not accept provider tokens.

External handoff is available only after work reaches review or acceptance. It
pushes the governed Forge branch, then creates or updates the pull/merge request.
The body is generated from Forge’s outcome, risk, verification, changed-file
summary, sealed evidence digest, and linked work context. Provider state is
auxiliary item metadata: failure never changes Forge custody or completion
state. GitHub review comments can be listed and explicitly registered as a new
follow-up item; feedback never mutates completed work implicitly.

### Register body

```json
{
  "title": "Q3 ledger update",
  "brief": "Refresh liquid ledger and brief",
  "repo_path": "/path/to/vault-or-repo",
  "base_ref": "main",
  "owner": "optional-user-id",
  "policy": null
}
```

`repo_path` may be the active vault root when Versions is on, or any other git
root. Forge does not enable Versions for you.

Home normally calls `repositories/inspect` before `items/start`. Inspection is
read-only and returns the canonical worktree root, display name, current and
suggested base branch, `has_commits`, dirty-file count, and remotes. An unborn
repository returns `has_commits: false` and `suggested_base_ref: null`; clients
must ask the user to create an initial commit before starting governed work.
If a previously selected base ref disappears, registration/provisioning returns
`409 base_ref_missing` instead of silently selecting another branch. New work
can select an existing branch; a previously saved draft must be recreated
against one. Paths
always refer to the daemon/workshop filesystem. A co-located Home may obtain
the input path from a native folder picker; a remote Home must obtain it from
the workshop.

Detected tasks are intentionally bounded to commands declared by common
repository markers (`Cargo.toml`, `package.json`, `go.mod`, Python project
metadata, Makefile targets, and .NET projects). The run endpoint accepts a task
ID returned by the list endpoint, never arbitrary command text. Running is
lease-fenced; the command, output, exit status, and duration are appended to
the active attempt's evidence log for Review.

Review comparisons are always between the evidence manifest's exact baseline
and sealed OIDs, never a moving branch name. Text files return addressable
hunks for inline or side-by-side presentation; binary files return existence
and byte-size metadata instead of pretending to provide a meaningful text
diff. The structured review projection derives its timeline from Forge's
append-only event log and its attribution from governed attempts and recorded
verification.

Restoring from Review is an explicit recovery transition. Forge returns the
item to Ready, invalidates any approval bound to the superseded review, begins
a human attempt, and restores the selected baseline text. The sealed commit
and evidence are not rewritten, so the newer reviewed version remains a Git
recovery point until the user seals another revision. Binary baseline content
remains recoverable in Git but is not written through the Home text API.

Line-anchored review comments are append-only events on the work item
(`ReviewCommentAdded` / `Resolved` / `Deleted`). Each comment binds to an
`evidence_id`, path, side, and line range, with an `anchor_digest` of the
quoted line content. `POST …/review/request-changes` records a
`ChangesRequested` event (including a daemon-composed revision brief from
unresolved comments), then reopens the item to Ready via the same recovery
transition as restore — without requiring a per-file restore. The next agent
attempt should be seeded with that revision brief on the same work item.

### Lease fencing

`begin` returns `lease.lease_id` and `lease.generation`. Every lease mutation
must present the same `generation`. Stale adapters get `409`.

Source saves are also lease-fenced. The body includes `path`, `content`,
`lease_id`, `generation`, and `expected_digest`. `path` is repository-relative;
absolute paths, traversal, symlink escapes, and directories are rejected. Text
bodies over 2 MiB are rejected. GET opens binary or oversized files as a
read-only preview (`encoding`, `preview`, `truncated`) rather than refusing the
read. `expected_digest` is the `sha256:…` of the full on-disk bytes from the
preceding GET. If the on-disk content no longer matches, PUT returns `409`
instead of overwriting concurrent work.

Create, rename, and delete use the same lease fence. Rename and delete also
require the digest last read by the client. New files use exclusive creation;
existing files are never replaced. Repository metadata, missing/outside parent
directories, and symlink escapes are rejected.

Complete editor refactors use `PUT …/source/workspace-edit`. Its body contains
`lease_id`, `generation`, an ordered `operations` array, and `preconditions` for
every path named by an operation. An existing path precondition carries
`expected_digest`; a missing path precondition records expected absence.
Supported operation kinds are `write`, `create`, `rename`, and `delete`.
Before touching disk, Forge validates all paths, preconditions, sizes, and the
entire virtual existence sequence. It then applies at most 512 operations and
8 MiB of combined text as one transaction. Any failure restores all original
files, while a stale digest or unexpected path returns a conflict without
changing the worktree.

Code workspace state is stored under Forge's data root, outside the governed
worktree. Clean tab/group state does not require a lease. Persisting a dirty
draft requires the undertaking's live lease and is bounded to 2 MiB per draft,
8 MiB total, and 32 tabs. Drafts retain their source digest so clients can
surface recovery conflicts instead of silently applying stale text. The optional
`layout` object restores contextual Code regions (`context_panel`, `terminal`,
`tests`, `search`, `changes`, `output`) plus `primary_task`, `active_run`, and up
to 12 `recent_runs` independently of Home shell desktops. The additive
`bottom_panel` field records the mutually exclusive feedback channel
(`problems`, `output`, `tests`, or `terminal`); older boolean fields remain a
compatibility fallback. Home presents matcher locations from the selected task
snapshot as run-provenanced Problems without replacing language diagnostics.
Pane geometry and group tab strips remain shell-owned.

### Errors

| Status | When |
|--------|------|
| 404 | Unknown work / attempt / active lease |
| 409 | Invalid FSM, stale lease, base advanced, evidence/env drift |
| 422 | Policy / capture blocked |
| 400 | Git / bad request |

---

## Boot

On daemon start, Forge opens `{dataDir}/forge` and runs `reconcile_on_boot`
with a process-backed liveness probe before HTTP serves. Prior-boot leases are
interrupted (dirty work preserved); open operations roll forward; orphaned
worktrees are reported, never auto-deleted.

## ACP binding (Cursor/Codex executors)

An external agent chat session can opt in to Forge custody by setting
`work_id` on `POST /v1/agents/sessions`:

- The work item must exist, be `Ready`, have no active attempt, and have a
  provisioned environment with a live worktree. Violations return `409`.
- The ACP session's `cwd` is forced to the attempt's private governed worktree,
  overriding any client-supplied `cwd`.
- The lease begins before provider session creation so the provider process can
  never start in the staging anchor. Provider creation and stream-registration
  failures release custody. Executor kind is `acp-cursor` / `acp-codex` with
  `agent_session_id` and `chat_session_id` recorded in the executor detail.
- During the prompt pump the daemon heartbeats the lease every ~15s and
  stages prompt/tool lines into `attempts/{seq}/evidence/commands.jsonl`
  via lease-fenced `append_command_log` (so seal digests real executor
  activity, not an empty log).
- An ACP `Error` or pump failure calls `fail_attempt` — the work returns to
  `Ready` with `RestartAllowed`. Cancelling the session calls
  `interrupt_attempt` with `ResumeSupported { provider_token: <ACP wire
  sessionId> }` (not the Medousa process handle).
- **Resume:** on the next `POST /v1/agents/sessions` for that work, pass
  `resume_provider_token` or omit it — the daemon looks up
  `latest_resume_token` and tries ACP `session/resume` (then `session/load`),
  falling back to `session/new` when the vendor rejects the token. The
  response echoes `resumed: true|false`.
- `AcpEvent::Done` is **not** a seal. Sealing stays explicit:
  `POST /v1/forge/leases/{id}/complete`. Chat SSE streaming is untouched —
  the adapter reports beside the stream, never instead of it.

Plain chat sessions (no `work_id`) are unaffected and never touch Forge.

## Storage accounting and cache governance

Forge custody, governed worktrees, repository-group build caches, Detamu,
artifacts, and Coder evidence are reported separately by the workshop daemon:

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/v1/maintenance/storage` | Physical-byte and file-count report plus current policy |
| PUT | `/v1/maintenance/storage` | Replace cache caps, free-disk floor, inactivity age, and automatic-cleanup setting |
| POST | `/v1/maintenance/storage` | Preview (`{"dry_run":true}`) or execute (`false`) pressure-aware cache cleanup |

The settings payload uses bytes:

```json
{
  "enabled": true,
  "repository_cache_max_bytes": 10737418240,
  "global_cache_max_bytes": 32212254720,
  "free_disk_floor_bytes": 10737418240,
  "min_inactive_age_hours": 24
}
```

A zero repository/global cap or free-disk floor disables that individual
boundary. Automatic maintenance runs at most every six hours. Cap-based cleanup
waits for the configured inactivity age; free-disk pressure may reclaim an
inactive cache sooner. Turning automatic maintenance off does not disable
manual preview or cleanup.

Only explicit repository-group `.cache` roots beneath Forge worktrees are
eligible. A repository cache is protected while any undertaking for that group
is non-terminal, including Ready, Executing, sealing, and review states. The
governor rechecks protection immediately before deletion, selects eligible
caches oldest-first, and never deletes worktrees, Forge event/evidence custody,
Detamu, artifacts, or Coder evidence. Deleted build caches are regenerable.

### Ephemeral Coder evidence

When a Forge-bound Coder tool produces an oversized log, diagnostic, trace, or
event payload that has no cheaper authoritative query or existing artifact
reference, the model-facing bounded observation may include an ephemeral
evidence receipt. Source reads, Detamu results, and other requeryable payloads
are not copied.

Before persistence, Medousa redacts sensitive JSON fields and common textual
credential forms, serializes canonical JSON, identifies it globally by
SHA-256, and gzip-compresses the object. Identical redacted payloads share one
blob even across undertakings. Receipts expose a
`coder-evidence:sha256:<digest>` reference, never a daemon filesystem path.
`cognition_coder_evidence_read` can read at most 32 KiB at a time and rejects a
reference that is not attached to the active Forge undertaking.

The initial policy is deliberately hard-bounded:

| Boundary | Limit |
|----------|-------|
| Logical or physical bytes per object | 8 MiB |
| Referenced physical bytes per undertaking | 64 MiB |
| Global physical bytes, including index size | 512 MiB |
| Successful/reproducible TTL | 6 hours |
| Failed/non-reproducible TTL | 72 hours |

Reading an object refreshes its class TTL, but never overrides the global cap.
Under pressure, successful/reproducible objects leave before failed evidence,
then oldest access wins. The daemon's six-hour storage pass removes expired
objects and safe orphan blobs. This object store remains ephemeral.

When the undertaking seals, Forge validates the narrow receipt records staged
by the Coder perception governor and writes accepted metadata to the canonical
evidence bundle as `receipts.json`. `manifest.json` binds that file with
`compact_receipts_digest`, `compact_receipt_count`, and
`compact_receipt_rejections`. The review projection exposes the same counts,
and `GET /v1/forge/evidence/{evidence_id}/receipts` returns the typed sealed
receipts. Source tool and call identifiers remain distinct even when multiple
agents observed the same content-addressed object.

Sealing never copies the gzip object or raw tool output. Every accepted receipt
has `raw_evidence: "ephemeral_only"`; a command-log record that claims raw
promotion is rejected and counted. Durable raw retention requires a future
explicit user pin or a separately defined narrow review policy—it is not an
implicit consequence of a tool returning data.

Home uses `/v1/forge/items/{id}/handoff` before moving from a human editing
lease to Codex or Cursor. The request includes `lease_id`, `generation`, and
`to_executor`. Forge records the transition, interrupts the current attempt,
and leaves the same worktree Ready. Starting the provider is a separate,
retryable operation: if provider startup fails, the user's files remain safe
and no executor owns a stale lease.
