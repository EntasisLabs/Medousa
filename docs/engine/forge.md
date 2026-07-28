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
