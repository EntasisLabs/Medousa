# Conversation context addressing, derivation, and replication

> **Status:** Fork proof implemented; explicit stashes next (2026-08-21)
>
> **First proof:** Fork a personal chat from any committed transcript entry into a
> new session and open it in a new Medousa tab.
>
> **Goal:** Make conversation history durably addressable at authority, session,
> transcript-entry, and execution boundaries so fork, context extraction,
> subagent handoff, cloud execution, and replication share one foundation.

## Decision

Fork is a product operation, not the persistence primitive.

Medousa will model conversation history as immutable transcript entries bound to
an ordered session log. Callers select history with a durable context manifest,
then materialize that selection as a derived session when they need an
independent conversation. Replication mirrors an existing logical session and
preserves its identities; derivation creates a new logical session and records
where its context came from.

The vocabulary is deliberately generic:

- **authority** identifies the durable workshop data authority;
- **session** identifies one logical conversation log;
- **transcript entry** identifies one immutable persisted message or receipt;
- **entry sequence** identifies an entry's ordered position in a session;
- **execution** identifies the interactive/worker run that caused entries;
- **stream event sequence** remains the replay cursor within one execution;
- **context manifest** identifies an ordered selection of conversation history;
- **derivation** records a new session materialized from a context manifest;
- **replica** is another physical copy of the same logical session.

Do not add fork-specific fields such as `forked_from_session_id` to transcript
rows. Fork is the first consumer of the generic derivation contract.

## Why this is needed

The current model has two meanings for "turn":

1. `ConversationTurn` is a persisted transcript entry with a role, content, and
   timestamp, but no durable entry id or position.
2. A daemon turn ticket is an execution with a `turn_id`; its stream contains a
   monotonic per-execution `seq` used for SSE replay.

Those concepts must stay distinct. Reusing `seq` or `turn_id` for transcript
ordering would make replay, replication, and provenance ambiguous.

Today Surreal orders transcript history by timestamp. That is adequate for
display but not a durable cutoff: timestamps can collide, imported history can
retain old timestamps, and a selected message needs a coordinate that survives
restarts and transport.

## Identity hierarchy

### Stable logical authority

Introduce a daemon-owned `AuthorityId` persisted with the workshop data. It
identifies the logical authority for sessions and survives daemon restarts,
process replacement, and data migration to another device.

It is not:

- Home's client-local workshop registry key (`personal`, a saved connection id);
- the mesh `device_id`, which identifies a cryptographic physical peer;
- a daemon `instance_id`, which identifies one process/boot for leases and
  reconciliation.

The separation is:

| Identifier | Lifetime | Purpose |
|---|---|---|
| `authority_id` | lifetime of the workshop data authority | namespace durable sessions and entries |
| `device_id` | lifetime of a paired cryptographic device | transport trust and signed mesh envelopes |
| `instance_id` | one daemon process/boot | leases, fencing, and recovery |

### Typed coordinates

Shared contract types should express coordinates structurally rather than as
delimiter-packed strings:

```rust
pub struct SessionRef {
    pub authority_id: AuthorityId,
    pub session_id: SessionId,
}

pub struct TranscriptEntryRef {
    pub session: SessionRef,
    pub entry_id: TranscriptEntryId,
    pub entry_seq: u64,
}

pub struct ExecutionRef {
    pub authority_id: AuthorityId,
    pub session_id: SessionId,
    pub execution_id: TurnId,
}
```

`TranscriptEntryRef` intentionally carries both `entry_id` and `entry_seq`.
The id provides stable identity and idempotency; the sequence provides position
and efficient range selection. Stores must reject a reference when those two
coordinates do not resolve to the same entry.

### Sequence domains

Sequence names must expose their domain:

| Field | Scope | Meaning |
|---|---|---|
| `entry_seq` | session | committed transcript order |
| `event_seq` / existing stream `seq` | execution | turn-stream replay order |
| `replica_cursor` | authority + session + peer | last replicated committed entry |
| `generation` | lease/resource specific | fencing and mutable runtime ownership |

No API may expose a bare `seq` when its domain is not already unambiguous from
the enclosing type.

## Storage model

### Logical records

The target model separates immutable content from its occurrence in a session:

```rust
pub struct TranscriptEntry {
    pub entry_id: TranscriptEntryId,
    pub created_at: DateTime<Utc>,
    pub caused_by: Option<ExecutionRef>,
    pub role: String,
    pub content: String,
    pub tool_names: Vec<String>,
    pub answer_state: Option<String>,
    pub parts: Option<Vec<TurnPart>>,
    pub slice_summary: Option<TurnSliceSummary>,
    pub speaker_profile_id: Option<String>,
    pub content_digest: String,
}

pub struct SessionEntry {
    pub session: SessionRef,
    pub entry_seq: u64,
    pub entry_id: TranscriptEntryId,
    pub source: Option<TranscriptEntryRef>,
    pub committed_at: DateTime<Utc>,
}
```

Properties:

- `TranscriptEntry` is immutable after commit.
- `(session, entry_seq)` is unique.
- `(session, entry_id)` is unique.
- Committed `entry_seq` starts at 1 and is contiguous with no duplicates or
  gaps.
- A batch append allocates a contiguous sequence range atomically.
- `content_digest` makes import and replication conflicts detectable.
- `caused_by` connects persisted history to an interactive or worker execution.
- `source` records the source occurrence when an entry is bound into a derived
  session. It is absent for native appends and replicas.

A derived session may bind the same immutable entry content at a new session
position. It does not need to duplicate the payload. Authorization is always
checked through an accessible `SessionEntry`; knowledge of an `entry_id` alone
never grants access to content.

This normalization is the target Surreal model. During migration, the file
backend may retain one self-contained record per line as long as its public and
store contracts preserve the same identities and invariants.

One serialized commit owner per session assigns `entry_seq`, persists the entry
and binding records, updates the catalog projection, and acknowledges the
commit. This follows ADR-016: callers submit typed append/derive commands and
never allocate sequence numbers or publish catalog success themselves.

### Deletion and retention

Deleting a session removes its `SessionEntry` bindings and session-owned
metadata. It must not remove immutable entry content still referenced by another
session or replica. Unreferenced entry payloads may be garbage-collected only by
an explicit, auditable store operation after all bindings and retention holds
are checked.

This extends the enumerated deletion requirements in ADR-014; adding the entry
and derivation stores requires adding them to the session deletion inventory.

## Context manifests

A context manifest is the reusable selection primitive:

```rust
pub struct ConversationRangeSelection {
    pub session: SessionRef,
    pub after_entry_seq: Option<u64>,
    pub through_entry_seq: u64,
}

pub struct ResolvedConversationRange {
    pub selection: ConversationRangeSelection,
    pub selection_digest: String,
}

pub struct ContextManifest {
    pub manifest_id: ContextManifestId,
    pub sources: Vec<ResolvedConversationRange>,
    pub created_by: Principal,
    pub created_at: DateTime<Utc>,
}
```

Ranges are ordered. Multiple ranges can express a curated set of turns, a
combination of chats, or a bounded worker context. A single range from the
beginning of a session through one entry is a fork prefix.

Phase 1 accepts committed contiguous ranges only. Later versions may add exact
entry selections or transformations, but they must be explicit. Summarizing,
redacting, or converting a source selection creates new entries with provenance;
it must not silently mutate the meaning of a faithful manifest.

Each resolved range's `selection_digest` allows a consumer to verify that the
range matches what its creator selected. It is an integrity check, not an
authorization token. A client submits `ConversationRangeSelection`; the
authority resolves it and stamps the persisted manifest before materialization.

## Derivation

Materializing a context manifest creates a new logical session:

```rust
pub struct SessionDerivation {
    pub derivation_id: DerivationId,
    pub target_session: SessionRef,
    pub manifest: ContextManifest,
    pub intent: DerivationIntent,
    pub caused_by: Option<ExecutionRef>,
    pub created_by: Principal,
    pub created_at: DateTime<Utc>,
}
```

`DerivationIntent` is descriptive policy/audit metadata, not a different
persistence path. Initial values may include:

- `fork`;
- `work_context`;
- `worker_context`;
- `prompt_stash`;
- `import`;
- `other(String)` for forward-compatible SDKs.

The derivation transaction must:

1. authenticate the actor;
2. resolve and authorize every source range;
3. freeze the committed source cutoffs and verify their digests;
4. create the target session and access policy;
5. bind selected entries in manifest order with contiguous target sequences;
6. persist the derivation record;
7. publish catalog visibility only after the records commit;
8. return the target session and derivation coordinates.

Failure must not leave a visible half-derived session. Retry with the same
idempotency key must return the same result or a deterministic conflict.

### API shape

The contract should expose the generic operation:

```http
POST /v1/sessions/derive
Idempotency-Key: <opaque key>
```

```json
{
  "sources": [
    {
      "session": {
        "authority_id": "auth_...",
        "session_id": "session_..."
      },
      "through_entry_seq": 18
    }
  ],
  "intent": "fork",
  "target": {
    "catalog": "single",
    "display_name": "Alternative authentication approach"
  }
}
```

Rust and Python SDKs expose `derive_session(selection, options)`. Product-level
helpers such as `fork_session(...)` build a manifest and call that method; they
do not create another daemon primitive.

The production route must be registered through the route-owned generated
contract from ADR-019, including explicit authorization, body bounds,
idempotency, error schemas, and client generation metadata.

## Replication

Replication and derivation reuse the same identities but have different
semantics:

| Operation | Session identity | Entry identity | Entry sequence | Provenance |
|---|---|---|---|---|
| native append | preserved | new | newly allocated | `caused_by` execution |
| derivation/fork | new | preserved immutable content | newly allocated in target | source occurrence + derivation |
| replica import | preserved | preserved | preserved | replica receipt/cursor |

Initial replication is single-writer:

- the session's authority owns sequence allocation and canonical appends;
- replicas import committed entries after a cursor;
- imports are idempotent by `entry_id` and validate sequence + digest;
- a conflicting payload, sequence, or digest fails closed;
- replicas do not become writers merely because they possess the data;
- transferring write authority requires a separate fenced authority-transfer
  protocol and is not part of this plan.

A remote daemon running an isolated subagent should initially derive a worker
session under its own authority from an authorized context manifest. It can
return result entries and provenance to the parent. Mirroring the parent's live
session and granting remote write authority is a later replication phase.

## Product operations built on the model

### Fork from here — first proof

The first shipped feature uses one source prefix:

1. User opens actions on a committed transcript entry.
2. Home sends the source `TranscriptEntryRef` to `derive_session` with
   `intent: fork`.
3. The daemon materializes a new personal session.
4. Home opens the returned session in a new workshop tab.
5. The new session presents subtle `Derived from …` navigation.

An active execution is never partially copied. Only committed entries at or
before the selected `entry_seq` are eligible.

`Fork with draft` is a Home composition: derive the session, copy the current
local composer draft into the target composer, and leave the source draft
untouched. Draft text is not smuggled into the derivation transaction.

### Work-context chat

A caller selects one or more ranges, derives a new session with
`intent: work_context`, then explicitly binds a Forge work item. Derivation does
not implicitly copy worktrees, leases, terminals, code bindings, approvals, or
mutable artifact ownership.

### Subagent/cloud context

A parent execution creates a bounded context manifest. The worker receives the
manifest or an authorized materialization and records the parent `ExecutionRef`
as causation. Worker results can be attached back to the parent without
pretending that the worker shared the parent's session runtime.

### Prompt stashes

Automatic per-session text recovery remains local and private. An explicit
daemon-owned prompt stash may reference a context manifest plus structured
composer content. Applying it can restore text into an existing session or
derive a new context session first.

## Authorization invariants

1. Resolving an entry requires read access to a session binding, not merely an
   `entry_id`.
2. Derivation requires read access to every complete source range and create
   access for the target catalog.
3. A derived target may not broaden the audience of source material unless the
   actor has an explicit export/share capability.
4. Shared-room derivation is deferred from the first proof. The first proof is
   personal-session to personal-session only.
5. Cross-authority materialization requires an authenticated transport grant,
   source-integrity verification, and an auditable import receipt.
6. Replica possession does not imply canonical write authority.
7. Deleting a source session does not silently delete an authorized derived
   session; retention and user-facing deletion copy must make this consequence
   clear.

## Compatibility and migration

### Legacy transcript assignment

Existing sessions need deterministic backfill:

1. Load entries using the current stable legacy order.
2. Assign `entry_seq` from 1 in that order.
3. Generate a durable `entry_id` for every legacy entry.
4. Persist the mapping before exposing addressable history.
5. Leave `caused_by` absent when no reliable historical execution can be proven.
6. Compute `content_digest` from a versioned canonical serialization.

Backfill must be restartable. It must never regenerate different ids after a
partial migration. Timestamp and content are not acceptable long-term identity
keys.

### API evolution

- Add entry coordinates to history responses without removing current content
  fields.
- Old clients continue rendering history while ignoring new fields.
- Fork UI is capability-gated on the daemon contract revision.
- Generated API bindings, Rust SDK, Python SDK, Home's Tauri bridge, engine docs,
  SDK docs, and strict contract verification change in the same implementation
  phase.

## Implementation phases

### Phase 0 — contract and authority identity

- [x] Add typed `AuthorityId`, `TranscriptEntryId`, `ContextManifestId`, and
  `DerivationId` contracts.
- [x] Define a stable daemon-owned authority identity and expose it through
  workshop/session API context.
- [x] Define canonical entry serialization and digest version.
- [x] Add schema and contract tests for coordinate validation.

### Phase 1 — durable transcript coordinates

- [x] Add immutable entry storage plus session-entry bindings in Surreal.
- [x] Extend the file backend with equivalent id/sequence semantics.
- [x] Allocate contiguous `entry_seq` ranges atomically on batch append.
- [ ] Persist `caused_by.execution_id` from interactive and worker paths.
- [x] Return entry references from history APIs.
- [x] Backfill existing history restartably.
- [x] Add concurrency, duplicate, digest-conflict, and migration tests.

### Phase 2 — context manifests and derivation

- [x] Add context selection resolution and authorization.
- [x] Add atomic `derive_session` with idempotency.
- [x] Persist derivation records and source occurrence mappings.
- [x] Update deletion inventory and orphan-entry garbage-collection rules.
- [x] Expose Rust/Python SDK methods and generated transport bindings.
- [x] Document the integrator contract.

### Phase 3 — fork proof

- [x] Add `Fork from here` to committed transcript-entry actions.
- [x] Open the derived session in a new workshop tab.
- [x] Show restrained source navigation in the derived session.
- [x] Add `Fork with draft` as a client composition.
- [x] Verify dark/light UI, keyboard navigation, reconnect, and stale-client
  capability behavior.

### Phase 4 — explicit stashes and context extraction

- [ ] Add structured explicit prompt stashes referencing optional manifests.
- [ ] Add selected-range and multi-range context creation.
- [ ] Add `Create work chat from selection` without implicit Forge binding copy.
- [ ] Surface saved drafts/context in Spotlight without turning it into a data
  management screen.

### Phase 5 — remote materialization and replicas

- [ ] Define signed context export/import grants and receipts.
- [ ] Materialize bounded worker sessions on another authority.
- [ ] Add idempotent replica deltas and per-peer high-water cursors.
- [ ] Add integrity/conflict tests across process restart and delivery replay.
- [ ] Keep canonical sessions single-writer until a separate authority-transfer
  ADR is accepted.

## Verification gates

The implementation is not complete until tests prove:

- concurrent batch appends never duplicate or reorder `entry_seq`;
- history restart and backfill preserve entry ids and sequences;
- selecting the same manifest twice is deterministic;
- idempotent derivation cannot create duplicate targets;
- unauthorized ranges reveal neither content nor existence details;
- target visibility cannot exceed source visibility without export authority;
- source deletion preserves referenced derived content and removes unreferenced
  content only through explicit garbage collection;
- replica replay is idempotent and conflicting digests fail closed;
- forked sessions contain exactly the selected committed prefix;
- live turn event replay remains independent of transcript entry sequencing;
- file and Surreal stores pass the same contract suite.

## Non-goals

- Multi-writer replicated sessions or conflict-free concurrent transcript edits.
- Moving a live execution between daemons in the fork proof.
- Copying Forge environments, terminals, leases, approval state, or mutable
  artifact ownership during derivation.
- Treating context possession as authorization.
- Replacing turn-stream `seq`, correlation ids, continuation records, or Forge
  attempt generations with transcript `entry_seq`.
- Automatically persisting private local composer drafts to the daemon.
- Building a conversation-tree UI before the fork interaction proves useful.

## Existing code anchors

| Area | Current anchor |
|---|---|
| Persisted transcript DTO | `crates/medousa-types/src/session.rs` (`ConversationTurn`) |
| Turn execution envelope | `crates/medousa-engine/src/turn_event.rs` (`TurnEnvelope`) |
| Stream replay sequence | `crates/medousa-types/src/turn_stream.rs` |
| Transcript stores | `src/session_store.rs`, `src/session.rs` |
| Session creation/history | `src/daemon_handlers.rs`, `src/daemon/router.rs` |
| Session catalogs | `src/session_catalog.rs`, `src/shared_session_catalog.rs` |
| Causal continuation | `src/turn_continuation.rs` |
| Coder resume provenance | `src/agent_runtime/coder_turn_checkpoint.rs` |
| Mesh device envelope | `src/mesh/envelope.rs` |
| Typed identifier authority | `docs/architecture/decisions/adr-014-identifier-and-filesystem-authority.md` |
| Generated API contract | `docs/architecture/decisions/adr-019-generated-api-contract.md` |
| Home session switching/drafts | `apps/medousa-home/src/lib/chat/sessionController.ts`, `draftPersistence.ts` |

## Locked decisions and later gates

Locked by this plan:

- fork is implemented through generic context derivation;
- transcript identity and execution identity remain distinct;
- transcript positions use `entry_seq`, not stream `seq`;
- logical authority, physical device, and daemon instance identities remain
  distinct;
- derivation creates a new session; replication preserves the session;
- immutable entries plus ordered session bindings are the target storage model;
- the first proof is personal, committed-history fork;
- replication starts single-writer.

Require a later explicit decision before implementation:

- authority-transfer and multi-writer semantics;
- shared-room export/audience-broadening policy;
- cross-authority retention and remote deletion propagation;
- transformed manifests (summary/redaction) and their trust policy;
- whether immutable entry payloads use reference-counted GC, retention epochs,
  or an append-only tombstone model at production scale.
