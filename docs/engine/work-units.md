# Work scopes and resource relationships

The full workshop daemon has a durable, owner-scoped registry for resource
relationships and session-independent work intent. It uses the existing
`cognition_runtime_query`, `cognition_runtime_mutate`, and `cognition_schema`
tools. It adds no app navigation or required Goals workflow.

This is the storage and intent foundation, with exact native user-vault note and
folder observations. Other native resource adapters, execution subscriptions,
autonomous coordination, provider federation, and contact delivery are subsequent
implementation steps. Recording a responsibility does not launch an executor or
register a schedule.

## Access and identity

The admitted turn supplies the domain: workshop authority plus authenticated
owner identity. Bound principal identity wins over any turn hint. For the local
app, the host uses the identity frozen at turn admission, rather than the
profile selected later. Requests cannot supply a domain, owner, or provenance
override. Reads require `content.read` and `workshop.interact`; mutations also
require `content.write`.

The registry lives on the workshop daemon under `{dataDir}/work_units`. It
contains scope metadata and intent; native notes, repositories, artifacts,
feeds, sessions, and executions retain their existing stores and access checks.
A recorded reference is neither a native content read nor an access grant.
Remote references do not establish cross-workshop identity or execution policy.

`ResourceRef` consists of `authority_id`, `kind`, and `id`. A locator may change
without changing that reference. Paths, titles, and content hashes alone do not
establish stable identity. The initial agent-facing registration records
unresolved claims. Models cannot assert native availability, native revisions,
or deletion; those facts require an authoritative adapter. `work.resolve` issues
and refreshes authoritative references for exact user-vault notes and folders.

## Native vault resolution

`work.resolve` is a mutation: it writes identity metadata and the admitted owner's
graph projection, so it requires the mutation permissions above. Supply an exact
configured `root_id`; it never falls back to the currently selected vault. The
target is a public user-vault note path, folder path, or previously issued exact
reference. For example:

```json
{"action":"work.resolve","root_id":"personal","target":{"kind":"note","path":"notes/design.md"}}
```

To refresh after a move, use `target: {"kind":"reference","reference":...}`
with the returned `resource.reference` and the same configured root ID. Responses
contain the resource, graph revision, and `coverage: "user_vault_exact_resource"`.
Unchanged observations do not append another graph event. Metadata carries a
native observation revision; no note body is copied into the graph or response.

Each physical vault has a daemon-issued namespace in the synced sidecar
`.medousa/vault/resource-identities.json`. References use
`vault:<namespace>:user:<resource-id>`, qualified by workshop authority and resource
kind. Paths are locators. Managed atomic writes and managed note moves preserve
the resource ID across owner and daemon restart. Managed deletion permanently
tombstones the ID and retains its links; restore or path reuse creates a new ID.
Existing note content and app navigation need no migration.

The adapter hashes at most 1 MiB of a note. Larger notes retain their ID but
resolve as `unavailable`, invalidating checkpoints on refresh. Folder observations
prove folder metadata only; they neither enumerate membership nor prove every
descendant ready. The sidecar holds at most 4,096 identities and 1 MiB of metadata,
with projection headroom checked before native mutations. Capacity exhaustion
rejects further mutations rather than evicting identities or tombstones.

Filesystem object identifiers and creation time provide reconciliation evidence,
not semantic identity. External replacement cannot inherit an old ID because its
bytes match. Missing or replaced resources resolve as `unavailable`; a physical
object observed at another path stays ambiguous until explicit reconciliation.
External moves and folder moves are not automatically adopted. Copied sidecars
under a different physical root, corrupt metadata, and unsupported versions fail
without resetting identities. Losing the sidecar produces a new namespace, so
old references cannot silently retarget. Unobserved filesystem identifier reuse
remains subject to the platform's object identity guarantees.

Native write/move/delete/restore journals retain identity bindings until sidecar
projection is synced. Write publication witnesses come from the staged file
handle, so an external replacement immediately after publication cannot inherit
the publisher's identity. If publication succeeds but projection fails, the native
commit reports repair required and retains its intent/receipt. Owner startup and
`work.resolve` replay pending journals before resolution. Unchanged metadata and
graph replays finish pending parent sync fences without adding observations.
A nonblocking root lock
rejects concurrent identity custody as overloaded; retry after the owner releases
it. When a write published without a durable physical publication witness,
recovery retains an ambiguous intent instead of guessing from equal content.
Identity resolution and further native mutations remain blocked until explicit
reconciliation through `work.reconcile`. An ambiguous startup retains the native
owner for ordinary content reads; identity operations still fail until repair.

Observations are explicit, not background subscriptions. `work.get` does not
refresh native content; refresh relevant resources before using a checkpoint for
new work. Project overlays, projects, artifacts, feeds, execution identities, and
cross-workshop resolution are not covered by this adapter.

## Native reconciliation

`work.reconcile` uses the same admitted owner, mutation permissions, explicit
configured `root_id`, and daemon filesystem authority as `work.resolve`. It adds
no user-managed Goals workflow or new app UI. The runtime validates evidence;
the reasoning agent cannot override native availability or use equal bytes to
assert identity. Fetch its typed schema through `cognition_schema`.

Inspect and attempt ordinary journal recovery first:

```json
{"action":"work.reconcile","root_id":"personal","command":{"operation":"inspect"}}
```

Inspection returns a bounded `pending_journals` list with exact operation IDs,
intent digests, locators, and witness/receipt flags, plus a bounded recovery
diagnostic. It contains no note bodies or original journal payloads. Inspection
can open a root whose ordinary startup recovery is ambiguous. Directory inspection
is capped at 128 entries and each journal at 64 KiB; larger or malformed metadata
fails without resetting identity. Ordinary recovery runs before repair and never
repeats a file mutation.

To adopt an external move, provide `command.operation: "adopt_move"`, a fresh
`command_id`, the original local `reference`, its exact `expected_native_revision`
from `work.resolve`, and the new public `path`. The runtime requires the same
physical object and kind, an original locator that no longer owns it, and no
competing identity for that object. A changed observation, tombstone, symlink,
foreign authority/root, or equal-content copy cannot pass that check. The accepted
move retains the reference and its graph links. Any replaced object already
registered at the new locator becomes unavailable with its own ID retained.
Folder adoption changes that folder only; reconcile known descendants separately.

When publication cannot be proven, provide `command.operation:
"quarantine_journal"`, a fresh `command_id`, and the inspected `operation_id` and
`expected_intent_digest`. The runtime preserves the exact intent and any native
receipt in `.medousa/vault/reconciliations/<operation-id>.json`, then atomically
publishes its decision and unavailable identity facts in the sidecar before
removing the active intent and syncing its parent. It changes no note bytes and
never retries write/move/delete/restore effects. Permanent tombstones remain
tombstones. An unproven publication does not bind the reserved or prior ID to the
current file; resolve that file separately to issue an appropriate new reference.
Quarantine reports `native_outcome: "unresolved"`, not completed work, and affected
unavailable facts invalidate old readiness checkpoints when projected.

Repair responses contain an immutable `reconciliation` receipt, affected current
`resources`, a graph revision when resources were projected, and
`file_effects_replayed: false`. Retry the exact request after uncertainty. Command
keys bind to the admitted domain and full request; changed intent under the same
key conflicts. Replays retain the receipt but publish current observations,
without rolling a locator back after a later managed move. A decision published
before intent cleanup is finished at owner restart or the next identity operation.
Archive-before-decision failures leave the active intent in place. Graph projection
failures remain retryable without repeating file effects.

The sidecar retains at most 256 repair receipts within its existing 1 MiB bound.
Exhaustion denies new reconciliation without evicting receipts; exact replay still
works. Compaction remains future work. Corrupt or changed archives/journals and
copied namespace metadata fail closed rather than granting arbitrary reattachment.
Reconciliation is native metadata repair, not execution completion, lifecycle
cancellation, automatic background scanning, or cross-workshop federation.

## Query actions

`work.graph` accepts:

| Field | Meaning |
|---|---|
| `collection` | `resources` (default), `relationships`, `work_units`, `events`, or `budget_reservations` |
| `anchor` | Optional exact resource reference; event history is domain-scoped and rejects an anchor |
| `direction` | For relationship queries: `both` (default), `incoming`, or `outgoing` |
| `limit` | 1–100, default 20 |
| `cursor` | Returned `next_cursor`, passed unchanged with the same query |

Resource queries with an anchor return that exact record. Work-unit queries
with an anchor return units explicitly containing that resource; a work-unit
anchor also finds its record and units composing or depending on it. Semantic
adjacency does not expand the saved scope. A session anchor also finds exact
conversation attachments, independently of the unit's resource scope.

Pages contain the domain, current `revision`, typed `items`, `next_cursor`, and
`coverage: "current_workshop_domain_registry"`. Coverage does not claim a
complete mesh or a complete index of native resources. Cursors bind to domain,
query, and revision. If the registry changes between pages, restart pagination.
Events are ordered by commit revision and retain the exact command, provenance,
and receipt.

`work.get` accepts an exact `work_unit_id` and returns the saved unit. A unit has
no required session. An optional, visible local `origin` session records its
source; later sessions inspect the same unit by exact identity.
The response includes `readiness_current`, evaluated from the same registry
snapshot as the returned unit. This checks recorded adapter facts, scope revision,
state, and expiry; it does not silently refresh native content.
`budget_usage` reports the aggregate's reserved/settled cost, total admitted
execution count, and outstanding execution count from that same snapshot.

For example, inspect accepted work through the existing query tool:

```json
{"action":"work.graph","collection":"work_units","limit":20}
```

## Intent mutation action

`work.record` accepts a `command` containing `command_id`, `expected_revision`,
and a typed `mutation`. Fetch the full parameter schema with `cognition_schema`
for `work.record`. Supported mutation operations are:

| Operation | Purpose |
|---|---|
| `record_resource` | Register or revise an unresolved resource reference and locator |
| `put_relationship` | Record a named `supports`, `informs`, `produces`, `tracks`, or `related_to` link between registered resources |
| `accept_work` | Save exact identity, intent, finite/maintenance kind, scope, completion condition, optional origin, and contact preference |
| `set_scope` | Revise a nonterminal unit's explicit resources, children, and dependencies |
| `set_state` | Record a nonterminal unit's state, reason, and evidence |
| `set_contact` | Revise contact preference independently of lifecycle |
| `attach_conversation` | Associate an exact, visible local session with existing work without changing its scope or starting execution |
| `record_readiness` | Save a bounded maintenance checkpoint against exact scope and adapter-owned native revisions |
| `set_budget` | Set explicit aggregate ceilings and an absolute execution deadline |

The command contract also defines `reserve_budget` and `settle_budget` for native
execution adapters. The agent-facing host rejects both operations; a model
cannot claim execution custody or refund its own cost. Native dispatch paths are
not yet connected to the ledger.

Scope members must already exist in this domain. Work-unit membership uses
`children` or `depends_on`, rather than the resource list. Composition and
dependency cycles are rejected together. Ordinary resource relationship cycles
are allowed. Cancelling a parent does not cancel shared children or native
executions.

For example, with an empty registry at revision zero:

```json
{
  "action": "work.record",
  "command": {
    "command_id": "accept-release-documentation-1",
    "expected_revision": 0,
    "mutation": {
      "operation": "accept_work",
      "work_unit_id": "release-documentation-1",
      "intent": "Keep release documentation current",
      "kind": "maintenance",
      "scope": {},
      "completion_condition": "Documentation reflects accepted release changes",
      "contact": {"kind": "silent"}
    }
  }
}
```

The condition is retained intent, not an executable predicate or registered
maintenance trigger. The receipt acknowledges storage, not active execution.

States are `accepted`, `active`, `waiting`, `needs_attention`, `paused`,
`satisfied`, `failed`, and `cancelled`. Terminal units cannot be implicitly
reopened. A finite unit's satisfaction requires available adapter-resolved
evidence and satisfied finite component/dependency units. An unresolved resource or
the unit itself cannot prove satisfaction. Maintenance units remain ongoing
until explicitly terminated; the store rejects terminal satisfaction for them.
For an ongoing member, a parent must save a `scope.readiness` requirement with
that member's exact `work_unit_id` and a named `condition`. A matching current
checkpoint can satisfy that requirement while the maintenance unit stays active.
Lifecycle records do not replace native executor status or validate the prose
completion condition by themselves.

`record_readiness` requires an active maintenance unit, its exact
`expected_scope_revision`, a named `condition`, distinct local resources from its
saved scope with their current `native_revision`, and `valid_for_seconds` in
1–86,400. Every proof must match an available, adapter-owned resource record.
The host stamps observation and expiry times; callers cannot backdate them.
Native facts must already have been recorded by an adapter; model registration
alone cannot produce a checkpoint.

The saved checkpoint pins both native versions and registry revisions. A resource
update, loss of availability, scope edit, pause, or expiry invalidates it for new
parent completion. Scope edits and lifecycle transitions clear the checkpoint;
contact changes and conversation attachments preserve it. Previously satisfied
finite units retain their historical result. `work.resolve` refreshes exact vault
resources; freshness remains limited to observations already present in this
registry until native event subscriptions are connected.

An accepted origin is also its first conversation attachment. A later session
joins through `attach_conversation`, specifying the exact work identity and
authority-qualified `session`; no title matching occurs. New attachments require
current native session visibility. Exact command replay still returns its
original receipt after that session is removed. Attaching a conversation changes
neither scope nor contact policy, and does not reopen terminal work.

Contact preferences are `silent`, `return_to_origin` (default), `participant`
with a registered participant reference, or `channel` with a registered channel
reference. They retain user intent without implying transport availability.
Changing to silent neither cancels work nor erases results. This implementation
does not send messages or voice invitations.

## Aggregate budget custody

`accept_work` may include `budget`, or `set_budget` can set it later. The limits
are `cost_microusd`, `execution_count`, `concurrent_executions`, and an absolute
UTC `deadline`. Zero is a real ceiling, not an unknown estimate. A new native
reservation requires active work, an available adapter-owned local assignment
or job identity, and explicit limits on every charged aggregate. Expired
deadlines, paused aggregates, and exhausted ceilings deny new reservations.
Limits describe accounting bounds and do not grant execution or spending authority.
Wake/retry/review limits remain in native continuation admission; they are not
yet aggregated by this work ledger.

Each execution has one durable `reservation_id` in its owner domain. A held
reservation counts its reserved cost and one outstanding execution; completion
replaces the cost hold with actual cost and releases concurrency. Aggregate
accounting follows composition `children`, counting a shared reservation once
even through several paths. `depends_on` consumes another unit's result without
automatically charging its execution to the consumer.

Attaching already allocated children checks the new aggregate's limits and
retains its charge. Removing them later does not erase admitted cost. Terminal
parents receive no new child allocations; their historical charges remain.
Satisfaction requires settled custody. Cancellation and failure do not refund
held reservations or imply native execution cancellation. Only an adapter's
`not_started` settlement releases a hold without cost or execution count.

Native actual cost is retained even when it exceeds a reservation or ceiling;
future admissions use that overrun. If aggregate cost exceeds `u64`, the reported
cost saturates with `cost_overflowed: true`; individual ledger records retain the
exact values and further reservations fail closed. Settlements are immutable;
retry their exact command after an uncertain outcome.

Query `budget_reservations` with an assignment/job anchor for exact custody, or
a work-unit anchor for that aggregate's retained charges. This is durable
accounting groundwork; it does not yet constrain native executors until their
admission and settlement adapters are connected.

## Persistence and recovery

One capability-confined JSON snapshot per domain contains current records and
immutable command history. Domain filenames are domain-separated SHA-256 keys,
not raw owner IDs. Cross-process writers use a nonblocking advisory lock and
compare the expected domain revision. Each atomic snapshot publishes intent,
effects, and the replay receipt together, with file and parent sync fences
subject to the platform guarantees of `medousa-store`.

Retry the exact command after an uncertain write outcome. Its original receipt
is returned with `replayed: true`, even after later commits. Changing command
content or provenance under the same ID is a conflict. A new mutation requires
a new command ID and the current revision. No title or session-name matching
merges responsibilities.

The foundation bounds commands to 32 KiB, snapshots to 1 MiB, each record
collection to 4,096 entries, and each scope to 128 explicit members. Full stores
reject new writes without evicting intent or replay history; journal growth and
compaction remain a subsequent storage milestone. Corrupt, mismatched-domain,
or unsupported-version snapshots fail without resetting the domain. Native
tombstones preserve relationships and prevent identity reuse after deletion.

All daemon file operations enter through `ForgeExecutionService`; no graph
write introduces a blocking filesystem wait on a Tokio worker. Embedded/mobile
engines do not advertise these workshop-local actions.
