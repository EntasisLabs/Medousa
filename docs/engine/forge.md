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
| GET | `/v1/forge/items/{id}/source?path=…` | Read bounded UTF-8 source from the governed worktree |
| POST | `/v1/forge/items/{id}/source` | Lease-fenced source-file creation |
| PUT | `/v1/forge/items/{id}/source` | Lease-fenced source save with digest conflict detection |
| PATCH | `/v1/forge/items/{id}/source` | Lease-fenced source rename with digest conflict detection |
| DELETE | `/v1/forge/items/{id}/source` | Lease-fenced source deletion with digest conflict detection |
| GET | `/v1/forge/items/{id}/tree` | List tracked and unignored repository files (bounded to 20,000) |
| GET | `/v1/forge/items/{id}/search?query=…` | Fixed-string tracked-source search (bounded to 500 hits) |
| GET, PUT | `/v1/forge/items/{id}/workspace-state` | Restore/preserve open files, editor groups, positions, and bounded dirty drafts |
| GET | `/v1/forge/items/{id}/review` | Structured outcome, risk, verification, attribution, timeline, and changed-file summary |
| GET | `/v1/forge/items/{id}/tasks` | Manifest-derived checks, tests, builds, and run commands |
| POST | `/v1/forge/items/{id}/tasks/{task_id}/runs` | Start a named, cancellable project run |
| GET/DELETE | `/v1/forge/items/{id}/task-runs/{run_id}` | Poll or cancel a project run |
| GET | `/v1/forge/items/{id}/tests` | Discover addressable project tests |
| GET | `/v1/forge/items/{id}/review/file?path=…` | Exact baseline-to-reviewed file comparison with structured hunks |
| POST | `/v1/forge/items/{id}/review/file` | Reopen work and restore one text file to its baseline while retaining the reviewed checkpoint |
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

### Lease fencing

`begin` returns `lease.lease_id` and `lease.generation`. Every lease mutation
must present the same `generation`. Stale adapters get `409`.

Source saves are also lease-fenced. The body includes `path`, `content`,
`lease_id`, `generation`, and `expected_digest`. `path` is repository-relative;
absolute paths, traversal, symlink escapes, directories, binary content, and
files over 2 MiB are rejected. `expected_digest` is the `sha256:…` value from
the preceding GET. If the on-disk content no longer matches, PUT returns `409`
instead of overwriting concurrent work.

Create, rename, and delete use the same lease fence. Rename and delete also
require the digest last read by the client. New files use exclusive creation;
existing files are never replaced. Repository metadata, missing/outside parent
directories, and symlink escapes are rejected.

Code workspace state is stored under Forge's data root, outside the governed
worktree. Clean tab/group state does not require a lease. Persisting a dirty
draft requires the undertaking's live lease and is bounded to 2 MiB per draft,
8 MiB total, and 32 tabs. Drafts retain their source digest so clients can
surface recovery conflicts instead of silently applying stale text.

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
- The ACP session's `cwd` is forced to the item's governed worktree,
  overriding any client-supplied `cwd`.
- The lease begins on the session's first prompt (not at create), so empty
  sessions never leave a work item `Executing`. Executor kind is
  `acp-cursor` / `acp-codex` with `agent_session_id`, `acp_session_id` (ACP
  wire id), and `chat_session_id` recorded in the executor detail.
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

Home uses `/v1/forge/items/{id}/handoff` before moving from a human editing
lease to Codex or Cursor. The request includes `lease_id`, `generation`, and
`to_executor`. Forge records the transition, interrupts the current attempt,
and leaves the same worktree Ready. Starting the provider is a separate,
retryable operation: if provider startup fails, the user's files remain safe
and no executor owns a stale lease.
