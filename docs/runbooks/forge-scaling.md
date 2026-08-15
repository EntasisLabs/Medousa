# Forge and Coder scaling

Operational recovery for H06 bounded Forge/Git execution, slug/catalog repair,
supervised Git children, and workspace observation limits.

## Queue saturation

Forge/Git work is admitted through `ForgeExecutionService` before it runs.

| Symptom | Meaning | Action |
|---------|---------|--------|
| HTTP `503` with `kind: overloaded` | Command, byte, class, or repository-lane budget is exhausted | Retry with backoff. Do not restart the daemon to “clear” a healthy queue. Reduce concurrent Home/API clients or wait for network Git / observation jobs to finish. |
| Health stays up while Forge is slow | Expected. Blocking work is off Tokio request workers | Confirm `/health` and H03 streams still respond. If they stall, file an ASYNC-001 regression. |

Caps (starting numbers): 64 queued commands, 8 blocking jobs, 2 network Git processes, 2 observation jobs, 8 MiB queued bytes, 1 mutating operation per repository.

## Slug and catalog repair

Slug uniqueness is a durable reservation journal (`{MEDOUSA_DATA_DIR}/forge/slug_reservations.jsonl`). The listing catalog is a rebuildable projection.

- **Reserve without an item** (crash during register): `recover_orphans` releases reservations that never received an item generation. A later register of the same slug should succeed after release.
- **Item without catalog row**: list/load still works from the item owner. Rebuild the catalog from item snapshots; do not treat a missing catalog row as “slug available.”
- **Two live items on one slug** must never happen. If it does, stop writes, inspect the slug journal and both item logs, and keep the committed item generation.

## Stuck Git children

Network fetch/pull/push run under `supervise_git` (`kill_on_drop`, 120s deadline, 8 MiB combined stdout/stderr, `GIT_TERMINAL_PROMPT=0`).

- Timeout → typed Git error, not success.
- Truncated output is marked incomplete; do not treat it as a completed fetch.
- If a child survives a daemon crash, inspect `ps` for `git fetch|pull|push` under the workshop user and terminate the process group. Uncertain cleanup is recovery-required, not success.

## Observation limits

Resume requires an **exact** generation-fenced observation (capture → observe → recheck).

| Completeness | Operator meaning |
|--------------|------------------|
| `exact` | Generations and overflow were unchanged across the scan |
| `incomplete` | Entry/byte/time budget hit; deny automatic resume |
| `unknown` | Watcher overflow, restart, or generation change during the scan; deny automatic resume |

Resume budgets: 100k untracked entries, 1 GiB per file, 4 GiB aggregate, 30s. Ordinary post-mutation: 5s / 512 MiB. The filesystem watcher is a hint, never sole post-restart proof.

## Compaction and log v2

Compaction triggers at 1,000 tail events or 8 MiB. Framed log v2 (`events.v2`) is written beside v1; v1 stays read-only for rollback until a later cleanup car. Do not delete v1 readers or dual-read markers yet.

## Related

- [Forge engine](../engine/forge.md)
- [HTTP API](../engine/http-api.md)
- [Configuration reference](../configuration-reference.md)
