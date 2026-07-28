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

Architecture notes: [v0.8.0-forge-plan.md](../../architecture/v0.8.0-forge-plan.md).

Storage root: `{MEDOUSA_DATA_DIR}/forge` (events + evidence outside the worktree).

---

## HTTP API

Base path: `/v1/forge`. Types are `medousa-forge` serde models (`WorkItem`,
`ExecutionLease`, `ReviewDecision`, …).

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/v1/forge/items` | Register undertaking (`repo_path`, `base_ref`, optional `policy`) |
| GET | `/v1/forge/items` | List items |
| GET | `/v1/forge/items/{id}` | Load item |
| POST | `/v1/forge/items/{id}/provision` | Create governed worktree env |
| POST | `/v1/forge/items/{id}/attempts` | Begin attempt (default executor `human`) → lease |
| POST | `/v1/forge/leases/{lease_id}/heartbeat` | Liveness (`generation` required) |
| POST | `/v1/forge/leases/{lease_id}/complete` | Seal checkpoint + evidence |
| POST | `/v1/forge/leases/{lease_id}/interrupt` | Interrupt; work preserved |
| POST | `/v1/forge/leases/{lease_id}/fail` | Fail attempt; return to Ready |
| POST | `/v1/forge/items/{id}/decisions` | Record evidence-bound review decision |
| POST | `/v1/forge/items/{id}/apply` | Apply decision (`PreserveBranch` / `FastForwardOnly` / `ExportPatch`) |
| POST | `/v1/forge/items/{id}/discard` | Discard env (worktree then branch) |
| POST | `/v1/forge/items/{id}/run-script` | Reference script executor (`argv`) |
| POST | `/v1/forge/items/{id}/export` | Portable bundle to `destination` |

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

### Lease fencing

`begin` returns `lease.lease_id` and `lease.generation`. Every lease mutation
must present the same `generation`. Stale adapters get `409`.

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
