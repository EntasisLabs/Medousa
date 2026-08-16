# H07 — Incremental vault index and atomic mutation

> **Status:** Implementing — semantics train exit tests landed.
> Repair cars R0–R8 plus S0–S8: opaque NoteVersion, durable generation,
> relocate no-replace, resident reconcile, incremental persist, truthful
> cursors/deltas, fenced search, and complete Home paging. Findings are
> Mitigated on those unit exit tests; Validated still needs multi-OS P06/P07
> evidence.
>
> **Accountable owner:** vault engine maintainers
>
> **Reviewers:** filesystem/platform, daemon API, Home vault UI, search, security, release engineering
>
> **Audit findings:** PERF-003 (High), CONSIST-001 (High), PERF-006 (High)
>
> **Release gates:** Gate B — trustworthy state; Gate C — bounded hot paths
>
> **Required decisions:** [ADR-014](../../docs/architecture/decisions/adr-014-identifier-and-filesystem-authority.md), [ADR-016](../../docs/architecture/decisions/adr-016-transactional-store-ownership.md)
>
> **Dependencies:** H02 typed paths/root authority; H04 transaction primitives; H05 request context; H12 performance/CI gates
>
> **Verification:** [Performance budgets P06/P07](verification/performance-budgets.md), [crash/concurrency matrix](verification/crash-concurrency-matrix.md)

## Outcome

The workshop daemon owns a resident, generation-stamped vault projection. Warm
reads do no recursive filesystem walk; one changed note updates only its
metadata, lookup buckets, link edges, search terms, and tree ancestors. Every
daemon mutation validates its precondition and commits under one per-note/path
transaction, with crash-safe replace/move semantics and a truthful receipt.
External edits enter as separately observed mutations through bounded watcher
reconciliation instead of causing global rescans on every accessor.

Home consumes one immutable lookup/tree projection per vault generation. It
does not rebuild a full path set in every recursive row or synthesize a note
corpus for every wikilink. Only expanded visible rows are mounted, selection
ancestor membership is O(1), and `L` wikilinks perform expected O(L) lookups.

H07 owns vault index/search/link/tree algorithms, compare-and-write, external
editor reconciliation, and the Home lookup projection. H02 owns root/path and
symlink authority. H04 supplies shared atomic file/receipt primitives. The
filesystem remains authoritative for note content; derived indexes are
rebuildable and never allowed to conceal a content mutation.

## Current failures

### The index rescans instead of indexing

`ensure_index_fresh` recursively walks user and project-overlay roots, stats
every candidate, clones the current index, builds path sets, and compares the
two. `list_entries`, `get_entry`, `note_exists`, `backlinks_for`, and
`all_entries` call it. `get_note` calls entry lookup and backlinks separately,
so one request can scan twice.

The scan still misses real changes: timestamps are truncated to whole seconds
and compared without file size, high-resolution time, file identity, or content
digest. The design pays O(files) per read without obtaining a reliable
generation fence.

When one file changes, the store reparses it but rebuilds the complete link
index and directly rewrites complete link/index files. Wikilink resolution
scans all known paths for matching filename stems and all entries for repeatedly
slugified titles. Search reads every note body, lowercases it, accumulates all
hits, sorts them, and only then truncates.

All of this synchronous filesystem work is reachable from async handlers.
`PUT` additionally clones the entire Axum body to validate UTF-8 even though
the body is already an owned bounded byte value.

### Compare-and-write is split in two

`write_content` reads current content and validates `If-Match`, later performs
ordinary `fs::write`, then updates derived memory/files. Two callers presenting
the same valid hash can both pass before either writes; both report success and
one update disappears. Create checks `note_exists` and writes later, which has
the same race.

Content and derived indexes have no common generation/receipt. Direct
truncate-write can leave a partial note or index. Move creates the destination,
emits its write side effects, and then deletes the source, so interruption can
leave duplicates and a successful-looking partial operation. Trash replacement
can delete an older trash entry before the move commits.

### Home reconstructs global indexes locally

Each recursive `VaultTreeNode` walks its complete subtree when selection
changes. Across an expanded deep tree, all rows together visit O(n²) nodes.
Every row derives `new Set(vault.notes.map(...))`, including leaves; preview
constructs another set.

Each wikilink converts that set into fake `VaultNote` objects, then resolution
maps back to paths, rebuilds a set, scans filenames, and scans titles. A note
with `L` links in a vault with `N` notes performs O(L×N) work and allocation per
render. The store fetches at most 500 notes and rebuilds the whole tree, so the
UI is simultaneously expensive and incomplete for larger vaults.

## Invariants

1. The daemon is filesystem/vault authority; Home never treats its local disk
   or cached note array as authoritative for a remote workshop.
2. Each active root has one index owner assigning monotonic vault generation;
   each effective note has a monotonic content generation/version.
3. Warm get/list/backlink/tag/tree/wikilink operations do not walk or stat the
   complete vault.
4. One changed path updates only that note and affected lookup/link/search/tree
   buckets, amortized by changed content/edges/depth rather than total notes.
5. A note read returns content, metadata, backlinks, and version from one
   validated vault generation or retries/fails visibly.
6. Daemon mutations serialize by canonical effective note/path set. Precondition
   validation and commit occur inside the same owner transaction.
7. Exactly one of two daemon writers with the same expected version commits.
8. Create never clobbers; replace never publishes partial content; move/delete/
   restore has one recoverable operation identity and truthful terminal receipt.
9. Content commit is authoritative before derived projection/event publication.
   Derived failure is repairable and visible, not reported as lost content.
10. Watcher events are hints. Overflow, ambiguity, restart, root change, and
    missed-event suspicion schedule bounded reconciliation and never certify
    freshness.
11. User-root shadowing of project-overlay notes is deterministic within the
    same projection generation, including create/delete/restore transitions.
12. Arbitrary external editors are not falsely promised portable filesystem
    CAS. Observed external changes become separate versions; ambiguity blocks
    automatic overwrite/resume and preserves recoverable content where observed.
13. Every scan, watcher queue, reconciliation set, index, search posting,
    response page, and UI row set has count/byte/time limits.
14. Home builds shared lookup maps once per received vault generation and only
    visible rows perform reactive rendering work.
15. Wikilink resolution is deterministic and reports ambiguity; it never picks
    a result because `HashMap` iteration order happened to differ.

## Non-goals

- replacing the user's Markdown files with an opaque database;
- claiming exact serializability against uncooperative arbitrary processes on
  filesystems with no conditional rename/content-CAS primitive;
- relying solely on modification time or filesystem watcher delivery;
- rebuilding a global immutable map by cloning all entries per mutation;
- loading 100,000 complete note bodies into Home;
- preserving current fuzzy title matching when it is ambiguous;
- synchronously making every derived index durable with every content write;
- solving multi-peer merge/CRDT semantics; or
- using a polling full scan as the steady-state freshness mechanism.

## Target ownership model

```text
Workshop daemon
  VaultRegistry[VaultRootId]
    VaultIndexOwner
      root/source generations + watcher/reconcile state
      NoteOwners[EffectiveNoteId / canonical path lane]
      metadata/path/title/stem/tag/tree projections
      forward/backlink adjacency
      full-text search projection
      immutable generation handles / delta stream

Home per workshop connection
  VaultProjection(generation)
    metadataByPath + exactPathSet
    pathsByStem + pathsByFoldedTitle
    parent/ancestor table + flattened visible tree
    incremental deltas / pagination cursors
```

`VaultRegistry` and mutation-lane locks cover lookup/admission only. Blocking
file reads, parsing, hashing, replacement, scan, and persistence run through a
bounded vault I/O service. One slow root/note cannot hold unrelated roots.

## Identity, versions, and snapshots

### Effective note identity

Consume ADR-014 `VaultRootId` and normalized relative path/root handle types.
Internally distinguish `(root_id, source, normalized_path)` from the effective
path exposed to users. User notes shadow project-overlay notes at the same
normalized path. Shadow/unshadow is an indexed source transition, not an
accidental overwrite.

Where possible, record platform file identity (device/inode or volume/file ID)
plus size, nanosecond timestamp, and content digest. File identity is an
observation aid, not authorization; every open remains handle-relative and
revalidates H02 confinement.

### Strong version tokens

Replace a bare content hash with an opaque version/ETag derived from root ID,
effective note generation, source generation, content digest, and schema. API
clients treat it as opaque. It does not contain raw paths and is not authority.

Mutation preconditions are explicit:

```text
CreateOnly
Match(NoteVersion)
AbsentOrMatch(NoteVersion)   // only where product semantics require upsert
Unconditional               // privileged/explicit, never autosave default
```

Responses include `vault_generation`, `note_version`, source, and operation or
commit receipt. `If-Match` maps to `Match`; `If-None-Match: *` maps to
`CreateOnly`. Missing preconditions on update follow a documented API policy
and are not silently interpreted as safe autosave.

### Generation snapshot API

Expose combined queries so one call observes one generation:

```text
get_note(path, minimum_generation?)
  -> { content, metadata, backlinks, note_version, vault_generation }

list_notes(filter, cursor, limit, generation?)
resolve_wikilinks(source, tokens, generation?)
tree_page/children(parent, cursor, generation?)
changes_since(generation, cursor)
```

The owner captures the projection generation, reads/validates content identity,
and confirms the generation did not change before returning. On concurrent
external change it reconciles/retries within a bound or returns `stale/retry`,
never combines old content with new backlinks.

Pagination cursors bind root, filter/sort, generation, and last key. Expired
generations return `reset_required`; they do not splice pages from different
trees. Response count and encoded-byte limits apply independently.

## Incremental daemon index

### Startup and reconciliation

Startup loads a versioned derived snapshot containing root configuration,
generation, per-file observation, metadata, lookup buckets, link/search
generations, and integrity. It registers watchers before or with a defined
barrier around scanning so changes cannot disappear between scan and watch.

A full startup/recovery scan streams directory entries through bounded workers;
it does not retain duplicate path/body corpora. It compares file identity,
size, nanosecond time, and prior digest. Changed candidates are read/parsed;
unchanged candidates reuse derived data. Scan publication is one new generation.

Targeted reconcile commands are coalesced by normalized path. Watcher rename
cookies/paired events are used when reliable; otherwise reconcile old and new
paths. Overflow marks a root `StaleReconciling`, increments an epoch, and runs a
bounded full reconcile. Reads may serve a labeled stable generation according
to product policy, but writes requiring freshness wait/fail until their touched
paths are reconciled.

Periodic low-priority audit samples or generation-triggered scans catch watcher
loss. Manual refresh requests reconciliation; it does not run an unbounded scan
inside the HTTP handler.

### Projection structures

The owner maintains:

- metadata/content-version by effective path and by source identity;
- ordered keys for prefix, modification-time, tag, kind, and tree queries;
- exact normalized path set;
- filename-stem and folded-title multimaps;
- parent and ancestor IDs plus child ordering/counts;
- forward and backlink adjacency sets; and
- search document/posting state keyed by stable note identity/version.

Update touched buckets in place under the serialized owner or use structurally
shared/sharded snapshots. Do not clone an O(all notes) `HashMap` merely to
publish a generation. Readers receive an `Arc` generation handle, query the
owner, or read immutable shards; retention caps old generations and connected
clients cannot pin them forever.

### Links

Parse raw links once when note content changes. Resolve tokens through exact
path, same-directory path, root path, stem multimap, then folded-title multimap
in a documented order. Multimaps return deterministic sorted candidates.
Ambiguous tokens remain unresolved/ambiguous with candidates rather than
silently binding to one title substring.

When note A changes, diff `old_forward[A]` against `new_forward[A]`; remove/add
only those backlink edges. A path/title creation, deletion, or rename can alter
resolution for unresolved tokens elsewhere. Maintain a reverse unresolved-token
bucket keyed by normalized stem/title so only candidate-affected sources are
reresolved. Global link rebuild is recovery/verification work only.

### Search

Introduce a purpose-built versioned inverted index behind a `VaultSearchIndex`
port first, avoiding a new database dependency in this workstream. It stores
normalized terms/field postings, document length/recency metadata, and bounded
snippet source offsets by note version. One note mutation removes its old
document postings and inserts the new document.

Search normalizes the query once, retrieves candidate postings, maintains a
bounded top-k heap, and reads only the selected versioned documents/snippet
ranges needed for results. It never reads/lowercases every file or sorts all
hits. The port permits a later SQLite FTS/Tantivy implementation without
changing API semantics.

Search index snapshots are derived and generation-stamped. Missing/corrupt
search state rebuilds in background with progress; API responses state the
indexed vault generation or `indexing/stale` policy. They never imply complete
results while silently omitting unindexed generations.

## Atomic mutation protocol

### Write/create

All daemon/API/tool/autosave writers submit a typed mutation to the root owner.
The owner reserves bytes/I/O permits and the canonical path lane, drains or
reconciles pending watcher observations for that path, reads the current
authoritative version, and checks the precondition inside the lane.

For replace:

1. create a collision-safe temporary file in the same authorized directory;
2. write bounded bytes, set required permissions, and sync to requested level;
3. revalidate root/path authority and current observed version immediately
   before publication;
4. atomically replace with defined Windows behavior and sync the parent where
   required;
5. assign note/vault generation and append the mutation/receipt record;
6. update touched projections from the committed bytes;
7. publish one generation delta and feed/workspace event after commit; and
8. clean the exact temp/recovery marker without following links.

For create, use `create_new`/no-clobber publication semantics; a prior existence
check alone is forbidden. Two daemon writers with one ETag are serialized and
the loser receives `412 stale_version` including current opaque version where
authorized.

If content commits but a derived projection fails, return/record a committed
content receipt plus `index_repair_required`; do not retry the content write or
claim it failed ambiguously. The owner blocks dependent mutations or repairs
from the committed mutation journal/version.

### External-editor honesty

Portable ordinary filesystems do not provide “replace only if current bytes
still hash to X” against an arbitrary process. Per-note mutexes serialize
Medousa writers only; advisory locks help only cooperative editors. H07 does not
label that limitation solved by moving the hash check closer to `rename`.

The supported contract is:

- daemon/API writers have strict atomic compare-and-write;
- watcher-observed external content is captured/reconciled as a new external
  mutation/version before a later daemon `Match` can commit;
- writes reconcile pending path epochs and revalidate immediately before replace;
- a watcher event during publication triggers post-commit reconciliation;
- observed divergent bytes are preserved in the version/recovery area before
  projection advances; and
- watcher overflow, unstable identity, or an unresolvable interleaving marks the
  path `ExternallyAmbiguous`, rejects automatic overwrite, and requires reload/
  conflict resolution.

On platforms where cooperative file coordination is available, implement it
behind the mutation port and verify it. Product/docs must still distinguish the
strict daemon contract from best-effort detection of uncooperative writers. An
arbitrary writer that changes and is overwritten entirely between observable
filesystem states cannot be given a portable CAS guarantee; claiming otherwise
would be false. Version history/backups limit loss, and supported external-edit
tests must yield conflict or a preserved separately versioned mutation.

### Move, delete, and restore

Move reserves ordered source/destination lanes and validates source version plus
destination precondition in one operation. Same-filesystem move uses atomic
rename with a durable intent/receipt and publishes one projection generation.
No create-event is emitted before source removal commits.

Cross-filesystem move is not atomic. Use a recoverable operation record:
reserve destination, write/copy and sync verified bytes, publish destination
generation, move source to transaction trash, sync both parents, then commit.
Recovery completes or rolls back based on the operation state and hashes; API
does not report success mid-protocol.

Delete atomically renames to a unique trash transaction path without deleting
an existing trash entry. Restore uses create-only destination semantics. Trash
metadata records original version/generation and supports idempotent recovery.
All operations update shadow/overlay and affected links/search/tree exactly once.

## Bounded async execution

Vault handlers parse/validate bounded bodies without an unnecessary full copy
and submit commands to an async service. Recursive scans, file reads/writes,
sync/rename, content parse/hash, search rebuild, and compaction run in bounded
blocking workers with H05 cancellation/deadlines.

Initial safety limits, tuned by P06:

| Resource | Initial policy |
| --- | --- |
| Request note body | existing API limit made explicit; reject before duplicate allocation |
| Mutation commands | 64 global and 8 per root, with byte permits |
| Blocking vault jobs | 8 global; 2 scan/reconcile jobs; 1 mutation per path lane |
| Watcher/coalesced paths | 10,000 paths or 16 MiB; overflow becomes one root reconcile epoch |
| Retained projection generations | latest plus bounded active cursors/grace; clients cannot pin indefinitely |
| List/tree/search page | 500 records and encoded-byte cap; cursor required beyond |
| Link candidates | bounded deterministic candidates with explicit truncated ambiguity |
| Full reconcile | cancellable, progress-reported, one per root; foreground mutation lanes prioritized |

Queue full/closed returns `overloaded` or awaits bounded admission. It never
runs synchronous fallback on the request/turn thread. Root removal/shutdown
stops admission, drains mutations to a receipt deadline, cancels derived work,
and reports incomplete operations/reconciliation.

## Home projection and rendering

### Shared generation lookup

Add a pure `VaultLookupSnapshot` built once when a daemon generation snapshot
or delta is applied:

```text
generation
metadataByPath
knownPaths
pathsByStem
pathsByFoldedTitle
parentByNode / ancestorIdsForSelection
childrenByParent / ordered visible-node descriptors
```

Pass the snapshot/handle into Markdown, transclusion, live-editor, environment,
and view resolvers. Change `resolveWikilinkTarget` to accept the maps and return
`resolved | ambiguous | missing`; remove all `VaultNote[]` and fake-note
adaptation from link resolution. Backend and frontend share generated fixtures
for normalization and candidate ordering.

Store updates apply daemon deltas immutably at touched buckets and advance one
generation. If a delta gap or expired cursor occurs, fetch a fresh paged
snapshot. A local autosave result carries its committed generation/version; UI
does not manufacture a generation from timestamps.

### Tree

Precompute the selected path's ancestor ID set once per selection. Each visible
row answers selected/ancestor with O(1) membership; no row recursively traverses
its subtree. Hoist known-path/recent lookup out of rows.

Flatten only expanded branches into a visible row list and virtualize it with
stable node IDs, overscan, and keyboard/ARIA tree semantics. Collapsed and
offscreen nodes do no component reactive work. Expansion changes splice the
affected range or recompute O(visible nodes), not O(all notes) per row.

For browse/search/tag/kind modes, use paginated daemon projections and windowed
lists. Do not rely on `listVaultNotes(limit: 500)` as a complete vault corpus.
Background editors retain only their needed note/version buffers under existing
byte/count policy.

## Observability

Record, without note bodies or raw paths by default:

- root hash, vault/note/source generation, projection state and old-generation
  retention;
- watcher events/coalesced paths/overflow/lag, reconciliation candidates,
  files statted/read, bytes hashed/parsed, scan progress and reason;
- mutation lane wait, expected/current version outcome, bytes written, temp/
  sync/rename/parent-sync latency, durability, external ambiguity and repair;
- metadata/link/search/tree buckets touched and derived publication latency;
- list/search/tree cursor resets, page records/bytes, search candidate/posting
  counts and indexed generation;
- Home lookup builds/deltas, visible/mounted rows, ancestor preparation, link
  resolution probes/ambiguities, selection/edit long tasks, and heap; and
- event-loop canary latency during scan, search rebuild, and large mutation.

Diagnostics expose hashed relative identifiers, operation IDs, generations,
file observation metadata, queue state, and recovery actions—not content,
titles, tags, link text, or absolute paths unless explicit local debug policy
allows them.

## Migration plan

### H07.0 — Fixtures, race tests, and baselines

- Implement P06/P07 generated shallow/deep/wide/link-heavy vaults.
- Add deterministic two-writer, create, move, delete/restore, external-edit,
  watcher overflow, and crash failpoints before ownership changes.
- Record filesystem calls, bytes, allocation, event-loop latency, UI subtree
  visits, link probes, mounted rows, and current 500-note truncation behavior.

### H07.1 — Mutation owner and atomic file primitive

- Introduce typed root/path/version/precondition/receipt/error contracts.
- Add root owner plus ordered per-path mutation lanes and bounded I/O service.
- Make PUT/create strict CAS/create-only with same-directory atomic replace.
- Implement journaled move/delete/restore and exact post-commit publication.
- Remove direct handler/store filesystem mutation and body duplication.

### H07.2 — Resident incremental projection

- Add generation-stamped metadata and lookup structures.
- Register watcher with scan barrier, targeted coalescing, overflow epochs, and
  bounded startup/manual reconciliation.
- Incrementally update path/title/stem/tag/tree/link buckets.
- Return combined note/backlink/version snapshots from one generation.
- Stop calling `ensure_index_fresh` from ordinary accessors.

### H07.3 — Search and derived persistence

- Introduce the search-index port and incremental purpose-built postings.
- Persist derived projection/search snapshots with applied generation/integrity.
- Add bounded rebuild/progress/stale-result semantics.
- Delete whole-vault content scan/sort and complete link/index rewrites.

### H07.4 — API pagination and deltas

- Add generated generation/version fields, opaque cursors, change/delta events,
  ambiguity, conflict, overload, stale/reset, and repair-required errors.
- Update SDKs/Home daemon adapter and compatibility fixtures.
- Keep v1 list behavior as a bounded adapter during migration; never present its
  500-note response as complete.

### H07.5 — Home lookup and virtual tree

- Build one lookup snapshot per daemon generation and migrate every wikilink,
  transclusion, preview, editor, environment, and view consumer.
- Precompute selection ancestors and remove recursive row scans/path-set builds.
- Flatten expanded nodes, virtualize, preserve accessibility/drag/reveal/recent
  behavior, and page browse modes.
- Delete `VaultNote[]`-based link resolution and fake-note conversions.

### H07.6 — Close migration and evidence

- Run P06/P07 and crash/concurrency cases on supported filesystem/OS matrix.
- Soak watcher overflow/external editing while reads, writes, search, and Home
  navigation continue under memory/event-loop budgets.
- Remove legacy scanners/writers/adapters/flags and retained rollback derived
  files after the release fence.
- Ship canonical behavior, conflict, recovery, and diagnostics docs.

H07.1 and H07.2 share the owner/generation schema and must agree before either
ships. Search can proceed after projection deltas stabilize. Home can build its
lookup/virtualization against generated fixtures while daemon pagination/deltas
land, but cannot assume local filesystem access when workshop is remote.

## Rollout and rollback

Derived indexes are disposable, so dual-read comparisons may validate new
projection/search results against the old scanner in sampled background work.
They may not run the old full scan on every production read. New content writes
have one authority: the mutation owner. Do not dual-write content.

Persist projection schema/generation beside legacy index files. On rollback,
an older build ignores/deletes new derived state and rescans authoritative
Markdown, but must understand any content/trash/move recovery markers or refuse
startup with an actionable version error. Keep operation/version history needed
to complete in-flight migrations through the release fence.

If watcher/index health regresses, enter labeled stale/reconciling mode and
serve/fail according to explicit policy; do not restore implicit scan-per-get.
If Home delta application fails, request a fresh generation snapshot and fall
back to paginated nonvirtual view—not an incomplete 500-note global array.

## Verification and exit criteria

### Correctness

- CM-006: N daemon writes with one `If-Match` yield exactly one commit and N−1
  typed conflicts; final bytes/version/receipt agree.
- CM-007: controlled external edits yield a preserved new version, conflict, or
  documented reconciliation; no observed content is silently clobbered.
- CM-008: concurrent write/move/delete/restore has one serializable recoverable
  outcome with no duplicate/orphaned index entry.
- CR-009/CR-010: every write/sync/rename/index-publication kill point recovers
  old or new complete content and truthful operation state.
- CR-011: corrupt/stale projection/search/link state is rejected and rebuilt
  from authoritative notes at a known generation.
- H02 traversal/symlink/junction tests pass at every temporary, destination,
  trash, watcher, and reconciliation open.
- Overlay shadow/unshadow and root switches retain deterministic source/version.

### PERF-003

P06 validates PERF-003 only when:

- warm get/list/backlinks/tag/tree perform zero recursive root walks;
- one unchanged note read performs bounded identity/content work without a
  second independent freshness scan;
- one note mutation touches O(content + changed edges/terms + path depth)
  derived work, not O(total notes/links);
- search retrieves postings/top-k without reading every body or sorting all
  notes/hits;
- external events cost O(changed paths) plus bounded overflow reconciliation;
- all blocking work stays off request executors and event-loop canary meets its
  retained budget; and
- atomic CAS overhead remains within the recorded write latency budget.

### PERF-006

P07 validates PERF-006 only when:

- one lookup snapshot/delta is built per vault generation, not per row/link;
- selection performs O(depth) ancestor preparation and O(1) membership per
  visible row, with zero recursive subtree visits;
- mounted/reactive rows remain bounded by viewport overscan;
- `L` wikilinks resolve with expected O(L) map probes and deterministic
  ambiguity, independent of total notes;
- 100k-note browsing is complete through pagination and stays within interaction,
  long-task, allocation, and heap budgets; and
- remote workshop behavior never invokes local path/file APIs.

Findings reach Shipped only after migrations, rollback, observability,
supported-platform evidence, generated client updates, and canonical docs ship.

## Canonical documentation at ship time

- vault engine/API docs: versions, preconditions, generations, pagination,
  watcher freshness, search indexing, overlay precedence, and error semantics;
- SDK docs: opaque ETags/cursors, conflict/retry/reset/repair outcomes;
- user guides: external-editor conflicts, reload/merge, trash/move recovery, and
  remote workshop filesystem authority;
- operator runbooks: watcher overflow, stale/rebuild state, failed operations,
  index/search repair, queue saturation, and migration rollback; and
- Home app reference: large-vault pagination, ambiguity, and conflict UX.

## Superseded code and concepts to delete

- `ensure_index_fresh` calls from ordinary accessors and second-resolution
  timestamp freshness;
- whole-index clones/path-set construction for one changed note;
- full `VaultLinkIndex::rebuild` and direct complete `links.jsonl` rewrite per
  mutation;
- path/title scans and repeated slugification for every wikilink;
- full-vault search body read/lowercase/all-hit sort;
- check-then-`fs::write`, check-then-create, and direct truncate writes;
- create-destination/event/delete-source move flow and trash overwrite;
- synchronous vault filesystem work in async handlers and PUT `body.to_vec()`;
- independent get-entry/backlinks freshness generations;
- Home `listVaultNotes(limit: 500)` as a complete corpus;
- per-row `new Set(vault.notes...)` and recursive `treeNodeContainsPath`;
- preview/link fake-note arrays and `VaultNote[]` wikilink resolver;
- whole-tree rebuild/render for selection and unvirtualized offscreen rows; and
- legacy derived-index writers/readers/flags after rollback fence.

## Code anchors

- `src/vault/store.rs`
- `src/vault/service.rs`
- `src/vault/links.rs`
- `src/vault/search.rs`
- `src/vault/path.rs` and `src/vault/roots.rs`
- `src/vault_handlers.rs`
- vault filesystem watcher/event paths
- `apps/medousa-home/src/lib/stores/vault.svelte.ts`
- `apps/medousa-home/src/lib/components/vault/VaultTreeNode.svelte`
- `apps/medousa-home/src/lib/components/vault/VaultMarkdownPreview.svelte`
- `apps/medousa-home/src/lib/utils/vaultTree.ts`
- `apps/medousa-home/src/lib/utils/resolveWikilink.ts`
- Markdown/transclusion/live-editor consumers of vault note arrays
