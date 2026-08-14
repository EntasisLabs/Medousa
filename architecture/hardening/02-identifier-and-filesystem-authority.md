# H02 — Identifier and filesystem authority

> **Status:** In progress — H02.1–H02.4 implemented; H02.5 identifier migration underway and platform evidence remains
>
> **Accountable owner:** daemon storage maintainers
>
> **Reviewers:** vault, runtime, Home/SDK, Windows, release engineering
>
> **Audit findings:** SEC-002 (Critical), SEC-003 (High), DATA-001 (Medium)
>
> **Release gates:** Gate A for SEC-002/SEC-003; Gate B for DATA-001
>
> **Required decision:** [ADR-014](../../docs/architecture/decisions/adr-014-identifier-and-filesystem-authority.md)
>
> **Verification:** [Security abuse matrix](verification/security-abuse-matrix.md), [crash/concurrency matrix](verification/crash-concurrency-matrix.md)

## Outcome

Untrusted identifiers cannot select filesystem locations. User-facing vault
paths remain useful but are confined by directory capabilities under hostile
symlink, junction, and replacement state. Session deletion removes its declared
data inventory or reports a retryable partial failure; it never lies.

## Implementation progress

The first H02.0 containment milestone is implemented:

- the compatibility grammar rejects normalization, separators, dots, controls,
  non-ASCII input, overlong input, and Windows device aliases;
- session-owned filenames and directories use the domain-separated `s1-` SHA-256
  storage key, preserving case-sensitive logical identity on insensitive filesystems;
- transcript, catalog, shared catalog, artifacts, media, extraction, verification,
  context-pack, tool-surface, and turn-ledger writes use the central layout;
- strictly valid legacy files/directories migrate by same-root rename on first
  write, while malformed entries remain untouched for H02.4 quarantine;
- primary session HTTP, interactive-turn, artifact, and media ingress rejects
  invalid identifiers without trimming or replacement; and
- ledger cleanup unlinks the exact file and propagates failure instead of calling
  recursive directory deletion and reporting success.

H02.4 now owns deletion through a durable per-session record and a typed surface
registry. The record is persisted before cancellation; pre-existing mutation
leases drain, and later transcript, catalog, artifact, media, extraction,
verification, context-pack, tool-surface, ledger, checkpoint, agent-mode,
metadata, shared-catalog, and turn-worker mutations fail closed. Concurrent
deletes reuse one opaque deletion ID. Every surface result is persisted before
the next surface runs, backend DELETE calls are followed by absence reads, and
the wire status distinguishes `complete`, `retryable_partial`, `blocked`, and
`deleting`. `deleted` is true only for `complete`; Home retains the row and
surfaces a retry when deletion is partial.

The `medousa session-storage` operator command now inventories all declared
file/directory layouts through no-follow capabilities. Dry-run is the default;
`--apply` copies only valid unambiguous legacy entries to their `s1-` key,
verifies content, journals each boundary, and retains the source for rollback.
Malformed, link-backed, wrong-type, ownership-ambiguous, and conflicting entries
receive bounded quarantine reasons and name digests. A planned copy can resume
after restart. Fresh-process fixtures populate every declared filesystem
surface, delete in a second process, and verify absence/tombstone enforcement in
a third.

H02.5 now has a shared typed authority-ID module. Environment profiles,
component state/runtime events, packages, model installs, pairing devices,
feeds, manuscripts and skill assets, provider caches/secrets, workshop scopes,
Grapheme refs/scripts, and turn-event journals derive full domain-separated
SHA-256 storage keys. Forge work items, attempts, reviews, repository locks, and
Detamu bindings use the same mapping inside the Forge domain. Home pairing
credentials, pairing token files/keyring accounts, and per-workshop process
state no longer embed remotely supplied workshop or device IDs.

The migrated daemon-owned stores use `StoreRoot` for bounded no-follow reads,
atomic writes, exact removal, and enumeration. Persisted `output_path`,
`body_path`, pairing credential relative paths, and model `local_path` fields
are metadata only; consumers rederive authority from the typed logical ID.
Hugging Face tree entries additionally pass a bounded cross-platform relative
path grammar before any model payload is created.

Model payload downloads now retain the opaque model-directory capability from
directory creation through every streamed write and verification read. Shard
hashing uses a fixed 64 KiB buffer instead of allocating the full model file.
Nested model paths are walked without following links, and replacement fixtures
prove that an ambient models-directory swap cannot redirect an active download.

Turn-event journals now retain their opened directory capability for journal
append and atomic commit-marker publication. Recovery enumerates and reads
regular files through the same held root and rejects link-backed journals.
Replacement fixtures prove that append and commit continue in the originally
authorized directory after its ambient spelling is moved and replaced.

Installed manuscript YAML, prompt assets, imported skill trees, editor writes,
and script discovery now use held roots, bounded reads, and atomic writes.
Skill import streams each regular source file between held source and destination
roots with a per-file limit and rejects links or special entries. OpenShell skill
upload starts from the held asset directory and passes `.` to the child instead
of reopening an ambient asset path. Opaque manuscript filenames are accepted by
validation only when their embedded logical owner matches exactly.

Safe legacy layouts remain read-only compatibility inputs during the rollback
window. A compatibility candidate must be a valid single-segment platform name
and, where the record carries its logical ID, the embedded owner must exactly
match the requested ID. Ambiguous lossy component/runtime names and TUI scope
directories cannot be reassigned safely and remain untouched. Compatibility
removal, native Windows evidence, and the full cross-platform abuse matrix
remain H02.5 work.

H02.1 is in progress: `medousa-types` now owns the validated, non-public
`SessionId` representation and validated serde boundary; the daemon mints
`ses_` IDs with 128 bits of randomness; caller-selected creation IDs are denied;
and Home obtains ordinary and shared-room IDs from `POST /v1/sessions` instead
of minting filesystem-authoritative UUID strings locally. Transcript and catalog
store traits now require `SessionId` for lookup, append, upsert, and deletion;
compatibility string parsing is confined to their public adapters, and repair /
backfill paths skip malformed legacy identifiers instead of treating them as
store authority. Central storage-key and session file/directory constructors now
accept only `SessionId`; satellite adapters must parse before they can acquire a
path. The report-job user hash also has its own domain instead of masquerading as
a session storage key.

Opaque transcript filenames are intentionally not reinterpreted as logical IDs
during file-only catalog repair: the digest is irreversible, and hashing the
`s1-…` filename again silently points at the wrong transcript. H02.4 must add the
durable storage-key-to-session inventory before opaque transcripts can be
recovered without their catalog rows.

H02.2 has an approved capability-kernel spike using `cap-std` plus `cap-fs-ext`.
`StoreRoot` opens ambient authority once, then accepts only bounded ASCII
`StorePath` values and walks every ancestor with no-follow directory opens. The
spike covers read, append, atomic replace, nested directory creation, relative
rename, exact unlink, and handle-based recursive deletion. Hostile fixtures
prove rejection of link leaves and ancestors, outside-root canary preservation,
and continued authority over the originally opened root after its ambient path
is renamed and replaced.

The file-backed transcript/history, single-session catalog, shared-session
catalog, tool-surface, and turn-ledger stores now own lazy `SessionFileStore`
capabilities for their complete lifetime. Keyed reads, appends, atomic
replacements, legacy first-write renames, exact deletion, and root enumeration
no longer reconstruct ambient paths. Transcript and ledger JSONL encoding avoids
the former per-line `String` allocation; catalog scans read only validated
regular-file entries through the held root; tool-surface and catalog rewrites use
the same atomic replacement primitive. Obsolete public flat-file path helpers
have been removed, so new callers cannot bypass the capability owner.
Replacement-path fixtures prove that the production wrapper continues writing
to the originally opened directory after its ambient name is moved and replaced.

The extraction, context-pack, and verification satellites now own lazy
`SessionDirectoryStore` capabilities for their session directories and
root-level indexes. Payload lookup is derived from `(SessionId, logical object
ID)` input through a domain-separated `o1-` key; persisted `output_path` strings are
metadata, never read or deletion authority. New index rows contain only the
opaque relative object name, and JSONL scans avoid per-line `String`
allocations. Safe legacy session directories migrate by handle-relative rename
on first write. Existing platform-invalid object names and absolute index
metadata are neither followed nor reinterpreted as authority; recovering those
payloads waits for H02.4 inventory/quarantine.

Coder turn checkpoints now use the same held-root directory capability for
bounded scans, atomic writes, legacy-directory migration, and recursive session
deletion. New turn snapshots use full domain-separated `o1-` keys instead of the
legacy truncated turn digest. Scans admit only validated regular-file entries,
cap each read before allocation, validate the embedded session/work scope, and
deduplicate current and legacy copies by logical turn ID before deciding whether
a checkpoint is resumable. This prevents an older active legacy copy from
resurrecting after a newer terminal/superseded write. Hostile link-backed session
directories fail closed with an outside-root canary intact.

Media payloads, cached text extracts, session deletion, and the root index now
use a held `SessionDirectoryStore` capability. Payload and extract lookup derives
from `(SessionId, media ID, MIME)` using separate `o1-` domains; absolute
`payload_path`/`extract_path` strings in legacy rows are metadata only and are
never followed. New rows store opaque relative names, payload/extract reads are
bounded, and index JSONL avoids per-line `String` allocation. Platform-invalid
legacy media filenames require H02.4 inventory/recovery.

Artifact payloads, aliases, the file index, fetch, search, maintenance, and
deletion now use the held `SessionDirectoryStore` capability. Payload lookup is
derived from `(SessionId, tool, direction, hash)` through a domain-separated
`o1-` key; persisted `payload_path` strings are compatibility metadata only.
Reads are bounded, content-addressed payloads avoid redundant rewrites, and
index JSONL avoids per-line `String` allocation. Hostile absolute metadata is
covered by an outside-root canary fixture. Indexless and platform-invalid
legacy nested layouts are not scanned or followed; H02.4 inventory/quarantine
must recover them explicitly.

All known internal session-directory trains now use capability-owned I/O. The
native Windows authority fixture creates a privilege-free NTFS junction in
process and exercises the shared capability surface against it; the fixture is
required by the Linux/macOS/Windows CI matrix rather than silently skipped.

The public artifact fetch contract no longer returns `payload_path`. Home copy
and share actions emit `medousa:artifact/{session_id}/{artifact_id}` references,
so remote workshops do not leak unusable daemon-local paths and clients cannot
mistake metadata for filesystem authority.

H02.3 now has a typed `VaultPath` grammar with bounded depth/bytes, canonical
Unicode, hidden/system-segment isolation, and cross-platform device, alias, ADS,
separator, and control rejection. Explicit user and project-overlay roots are
cached as held capabilities; existing root components and every later path walk
use no-follow directory opens. A cached-root replacement fixture proves writes
continue through the originally authorized handle rather than a replacement at
the ambient spelling.

Core note and calendar reads/writes, remote file preview, index/backlink
persistence, metadata scans, overlay reads, trash, restore, and registered-root
inspection now use those capabilities. Writes and generated indexes use atomic
replacement, previews are bounded, JSONL index reads avoid per-line `String`
allocation, and scanners admit only typed regular-file entries. Link-backed
leaves and ancestors fail closed with an outside-root canary intact. The old
`resolve_*_note_path`, `trash_path_for`, and lexical `ensure_within_root`
authority helpers have been removed.

The vault-Git boundary now uses the held vault directory as the child process
working directory on Unix (`fchdir` between fork and exec), so renaming and
replacing the configured root cannot redirect Git. Repository detection and
`.gitignore` setup are capability-relative; note paths use `VaultPath`; and
restore reads an exact commit blob before an atomic capability-relative write
instead of letting `git checkout` mutate the worktree by ambient traversal.
The unused POST/DELETE worktree endpoints were removed because their arbitrary
caller-supplied paths gave Git recursive create/delete authority outside every
Medousa-owned root. Worktree inspection remains read-only.

On Windows, the no-follow capability retains handles opened without
`FILE_SHARE_DELETE` for the entire configured-root component chain. That pins
the ambient spelling against rename/delete replacement; immediately before
`CreateProcessW`, Medousa reopens the chain without following reparse points and
compares the volume/file identity of the final directory. Git only receives the
string CWD after that check. Every Git subprocess uses the shared
`CREATE_NO_WINDOW` policy, null stdin, and non-interactive Git/GCM settings.
Portable MinGit extraction is now in-process with enclosed zip paths, removing
the PowerShell subprocess entirely. Windows-only fixtures cover root rename
locking, mismatched process-root identity, root-component reparse rejection,
and junction-backed read, list, append, write, rename, file-delete, and
recursive-delete denial. Junctions are created directly with the Win32 reparse
API, so the suite neither needs symbolic-link privilege nor shells out. The
`filesystem-authority` CI matrix runs shared root, held-journal, and held-model
payload fixtures on native Linux, macOS, and Windows; Windows additionally runs
in-process MinGit extraction. Host-provisioned mount/bind fixtures and retained
release evidence remain before H02.3 closure.

## Current evidence and blast radius

### Session identifiers

Session ingress generally trims and checks only non-empty strings. The daemon
create-session endpoint accepts a caller-supplied ID, and Home also creates
`medousa-home-{uuid}` locally before the first turn.

Known raw or independently sanitized path derivations include:

| Surface | Current shape | Risk |
| --- | --- | --- |
| Transcript/history | `history/{session_id}.jsonl` | traversal/read/write/delete |
| Single/shared catalog | `{catalog}/{session_id}.json` | traversal/alias/delete |
| Artifacts and aliases | `artifacts/{session_id}/...` | traversal and recursive cleanup |
| Media | `media/{session_id}/...` | traversal/write/read |
| Extraction | `extractions/{session_id}/...` | traversal/write/read |
| Verification | `verifications/{session_id}/...` | traversal and recursive cleanup |
| Context packs | `context-packs/{session_id}/...` | traversal/write |
| Tool/session surfaces | lossy sanitized filename | collisions/aliasing |
| Turn ledger | lossy sanitized filename | collisions and deletion mismatch |
| Session lifecycle | many best-effort deletes | expansion, omission, false success |

This table is a seed, not the final inventory. The implementation must discover
all typed and string paths, including storage backends and test-only migration
paths, before compatibility removal.

### Vault

The original lexical containment and ordinary-path implementation has been
removed from the core vault store. Remaining evidence work covers Windows
junction/reparse behavior, mount/bind boundaries, configured-root lifecycle,
and vault-Git subprocess authority; see the implementation progress above.

### Deletion

The old best-effort deletion path is removed. Durable tombstones, mutation
exclusion, the typed registry, per-surface results, retry, status lookup, and
fresh-process absence evidence are implemented. Locus is included only when
requested; failure remains visible and retryable instead of preventing local
surface cleanup.

## Invariants

1. Raw request strings never reach authority-bearing store path construction.
2. Each logical identifier has one canonical equality/serialization rule.
3. Distinct identifiers never map to one storage key.
4. Callers cannot supply absolute storage paths through identifier fields.
5. All root-contained operations are relative to an already opened trusted
   root and reject links/reparse points by default.
6. Validation, lookup, mutation, and deletion cannot be separated by a
   caller-controlled link replacement.
7. Invalid legacy data is quarantined, not reinterpreted or deleted.
8. Recursive deletion accepts a typed store-owned directory capability, never
   a raw ID-derived `PathBuf`.
9. Deletion is idempotent and reports required-surface failure.
10. Adding a session-owned store breaks a completeness test until registered.

## Non-goals

- preventing an authorized user from editing ordinary content inside a vault;
- following arbitrary vault links for convenience;
- replacing filesystem permissions, sandboxing, or backups;
- redesigning transactional store internals owned by H04/H07;
- changing display names or external-service correlation IDs into opaque IDs;
- silently repairing every historical malformed path.

## Identifier model

### Types

Add shared domain types in `medousa-types` or a dependency-light domain crate:

```rust
struct SessionId(String);
struct StorageKey(String);
struct VaultPath(Vec<VaultSegment>);
struct ArtifactId(String);
struct WorkId(String);
struct ProfileId(String);
```

Construction is through `parse`/`TryFrom`, not public tuple fields. Serde input
uses validated deserialization or explicit API conversion so a derived
`Deserialize` cannot bypass invariants. Domain/store functions take references
to these types rather than `&str`.

### New session format

- The daemon generates new chat session IDs at `POST /v1/sessions`.
- Use a versioned opaque representation with at least 128 bits of entropy and a
  fixed ASCII alphabet, for example `ses_<canonical-uuid-or-ulid>`.
- The maximum textual length is fixed and checked before allocation-heavy work.
- Home, CLI, TUI, SDKs, and adapters request an ID rather than minting one.
- Display name and external correlation key are independent metadata.

### Legacy compatibility

The compatibility parser accepts only bounded ASCII IDs with a reviewed safe
alphabet and no separators, dots, prefixes, control bytes, or platform aliases.
Legacy IDs do not become filenames: `StorageKey::for_session(version, id)` uses
a collision-resistant digest/encoding and includes a layout/version domain
separator.

Do not use trim-and-accept or character replacement. If two legacy names would
compare differently at the API but collide on a case-insensitive filesystem,
they must still have different opaque storage keys.

### Boundary rollout

1. Parse at HTTP/command/tool ingress and return a stable `400 invalid_id`.
2. Convert internal store traits and central path constructors.
3. Convert SDK signatures/generated types and first-party clients.
4. Deny caller-selected IDs on new-session creation after clients migrate.
5. Remove `&str` compatibility overloads and local sanitizers.

## Store-root capability

Introduce a small filesystem-authority layer used by session stores and vault:

```text
StoreRoot (opened trusted directory)
  + StorageKey / VaultPath
  -> relative open/read/create/rename/unlink/enumerate
  -> no-follow result or typed confinement error
```

Required operations:

- open existing regular file without following final or ancestor links;
- create a new file under verified/created directories;
- atomic replace within one root;
- rename between two relative locations in explicitly compatible roots;
- unlink exact file or empty directory;
- recursive deletion that never traverses links or mount/reparse redirects;
- enumerate metadata without following links; and
- create nested directories one segment at a time without following links.

Unix implementation should use directory-relative file descriptors and
no-follow/openat-style primitives. Windows must open relative to an authorized
root where possible and inspect/reject reparse points with appropriate share
and disposition flags. Exact dependencies are chosen in the implementation
spike; using a capability filesystem crate is preferred if its locked version
proves the required Windows semantics.

Any fallback that performs `canonicalize`, drops the handle, and later calls an
ordinary path operation fails the mutation/deletion requirement.

## Vault path contract

`VaultPath` is slash-separated product syntax. Parsing occurs before touching
the filesystem and enforces:

- non-empty relative segments and bounded total bytes/depth;
- no `.`, `..`, absolute/root/prefix components, backslash, NUL, or controls;
- one documented Unicode normalization form;
- no Windows reserved device basenames, alternate data streams, or trailing
  dot/space aliases on any supported platform; and
- file-extension policy only where an operation requires it.

The same parser and root operation cover user vault, `.trash`, and project
overlay. The default policy is:

- link/reparse leaf: visible as unsupported metadata, never read as a note;
- link/reparse ancestor: operation rejected;
- mount/bind boundary detected during traversal: rejected unless the root was
  explicitly registered as its own authority;
- overlay: read-only through its own root capability;
- trash: private store root, not a magic user path that can redirect restore.

Indexing must not publish outside-root content. A hostile entry is skipped with
a bounded diagnostic and cannot be used through a stale index entry.

## Session storage layout

Create a versioned layout API rather than a global helper collection:

```text
SessionStorage
├── transcripts/{storage_key}.jsonl
├── catalogs/{storage_key}.json
├── artifacts/{storage_key}/...
├── media/{storage_key}/...
├── extractions/{storage_key}/...
├── verifications/{storage_key}/...
├── context-packs/{storage_key}/...
├── tool-surfaces/{storage_key}.json
└── ledgers/{storage_key}.jsonl
```

This diagram is conceptual; existing top-level roots may remain to minimize
migration. The invariant is a shared storage key and typed store ownership, not
one physical parent directory.

Path-returning helpers become private. Operations return data or typed errors.
If a path must be passed to a trusted subprocess, the store creates a scoped
lease/capability after confinement rather than accepting one back from a caller.

## Legacy migration and quarantine

Before mutation, build a read-only inventory containing root, entry type,
encoded name, decoded candidate ID, owning store, link/reparse state, size, and
collision group. Store the report under a versioned migration record without
raw secrets.

For each unambiguous ordinary entry:

1. Parse/validate the logical ID or match it to a catalog record.
2. Compute the new storage key.
3. Copy/transform to a temporary destination without following links.
4. Validate count, size, and content digests as appropriate.
5. Atomically publish the destination and migration record.
6. Retain or rename the source until the new layout survives restart.
7. Remove the legacy source with the same confined primitive only after the
   rollback window.

Quarantine malformed, colliding, case-aliased, lossy-sanitized, link-backed,
and ownership-ambiguous entries. Operator tooling can export or explicitly map
them; automatic cleanup cannot delete them.

## Complete session deletion

Replace best-effort orchestration with a deletion owner and registry:

```text
DeletionId + SessionId + requested memory policy
  -> persist deleting tombstone
  -> cancel/deny new session work
  -> invoke each registered surface deleter
  -> retain per-surface result
  -> verify from fresh handles/store reads
  -> mark complete or retryable_partial
```

Initial required inventory:

- active turn/ticket and stream attachment;
- transcript/history backend;
- single and shared catalog rows;
- session metadata and agent-mode state;
- artifacts/aliases, media, extraction, verification, and context packs;
- tool surface and session-surface state;
- turn ledger and recovery references;
- channel/session references;
- workspace/job references whose ownership contract requires removal; and
- Locus nodes when `purge_locus` was requested.

Each store implements idempotent `delete_session(&SessionId) -> Result`. Missing
is success; type mismatch, confinement error, I/O failure, or remaining data is
not. The API returns `complete`, `retryable_partial`, or `blocked` plus safe
surface reason classes. A compatibility `deleted` boolean may equal true only
for `complete`.

The immediate DATA-001 repair uses exact file unlink for the ledger and
propagates failure, but closure requires the complete inventory test.

## Concurrency, durability, and cancellation

- A deletion tombstone rejects new turn/session mutations before cleanup.
- Concurrent duplicate deletes join or retry the same deletion operation.
- Creation cannot reuse a tombstoned ID.
- Vault write/rename/delete operations retain the root/parent handles through
  commit; a watcher/index refresh cannot weaken authority.
- Atomic writes use a temporary file in the same authorized directory, sync
  according to H04 policy, then handle-relative rename.
- Link/reparse swaps at every traversal step must yield either safe success on
  the originally authorized object or a confinement error—never outside effect.
- Cancellation leaves a recoverable migration/deletion record, not an
  unreported half-state.

## Delivery slices

### H02.0 — Stop dangerous session paths

- Add strict ingress validation for all currently accepted session ID forms.
- Centralize collision-free storage-key derivation.
- Route raw session path constructors through the central layout.
- Replace ledger directory deletion with exact file deletion and surface errors.
- Add traversal/absolute/separator tests to every session satellite.

### H02.1 — Typed session boundary

- Add non-public `SessionId` representation and validated serde/API adapters.
- Make daemon session creation authoritative and migrate Home/SDK/CLI/TUI.
- Convert store traits and request-scoped runtime APIs from `&str`.
- Remove local lossy filename sanitizers.

### H02.2 — Root-capability filesystem layer

- Spike and select Unix/Windows primitives or a capability crate.
- Implement read, create, replace, rename, unlink, recursive delete, and walk.
- Add link/junction/mount/reparse and replacement-race fixtures.
- Move session storage operations onto it.

### H02.3 — Vault confinement

- Introduce `VaultPath` and cross-platform alias validation.
- Open explicit user, trash, and overlay root capabilities.
- Convert read/write/create/delete/rename/trash/restore/index operations.
- Reject links/reparse points and remove lexical containment as authority.

### H02.4 — Migration and deletion inventory

- [x] Build dry-run inventory and collision/quarantine reporting.
- [x] Implement restartable legacy storage migration.
- [x] Add deletion tombstones, surface registry, truthful result schema, and retry.
- [x] Add fresh-process completeness and injected-failure tests.

### H02.5 — Broader identifier inventory and closure

- [x] Audit externally influenced path-bearing IDs across daemon, crates, Home,
  installer, CLI, and TUI.
- [x] Add shared typed IDs and domain-separated opaque storage mappings.
- [x] Migrate critical profile/feed/component/environment/pairing/package/model,
  provider, manuscript/skill, Grapheme, turn-journal, workshop, and Forge IDs.
- [x] Stop treating persisted path strings as read/delete authority on migrated
  surfaces.
- [x] Move model payload streaming, manuscript/skill file operations, and the
  engine turn journal onto held capabilities through the final read/write to
  close replacement races.
- [ ] Delete compatibility parsers and legacy layouts after the rollback window.
- [ ] Complete native Windows junction/reparse and supported-platform evidence.
- [x] Update the canonical data-directory/upgrade guidance.

#### H02.5 compatibility ledger

| Surface | New authority | Legacy policy |
| --- | --- | --- |
| Environment/component/runtime/feed/provider cache | typed ID + opaque key + `StoreRoot` | safe exact-owner read; ambiguous lossy entries untouched |
| Packages/models | catalog-validated typed ID + opaque directory | known-catalog safe directory read during rollback |
| Pairing and Home workshop credentials/tokens | device/workshop key + no-follow daemon store | safe name plus embedded-owner verification; Home migrates bounded token spellings |
| Forge/Detamu | typed Forge ID + domain key | strict safe item directory only; snapshot/event owner verification |
| Manuscripts, skill assets, Grapheme refs/scripts | typed ID + opaque file/directory | safe direct name only; persisted path metadata ignored |
| Turn-event journals | typed turn ID + opaque journal/marker | old lossy journals are not guessed because ownership can be ambiguous |
| TUI workspace scope | opaque scope directory | old sanitized scopes remain untouched because the file does not prove ownership |

## Verification

Run FS-001 through FS-008 from the security matrix against every operation and
root on Linux, macOS, and Windows. Required input classes include parent/absolute
paths, both separators, encoded aliases, controls, Unicode normalization,
overlong input, Windows reserved/ADS/trailing-dot-space names, symlinks, hard
links where relevant, junctions/reparse points, mounts, and concurrent swaps.

Every destructive test places byte-identical canaries inside and outside the
root and verifies them from a fresh process. Unit tests of the parser are
supporting evidence only.

Deletion additionally requires:

- every satellite populated, deletion run, and fresh-process absence proof;
- failure injection before/after each surface and successful retry;
- concurrent turn/create/delete races;
- missing, corrupt, wrong-type, and link-backed entries;
- Locus purge on/off semantics; and
- legacy migration interrupted at every commit boundary.

## Exit criteria

H02 reaches **Validated** when:

- no external raw string constructs a session or vault authority path;
- all known session satellites use the typed ID/storage-key mapping;
- vault operations use verified handle-relative/no-follow primitives;
- the cross-platform FS matrix and swap races pass;
- invalid legacy entries are quarantined without collision or outside effect;
- deletion reports partial failure, is retryable, and passes the complete
  fresh-process inventory test;
- store/API/SDK/Home/CLI/TUI compatibility tests pass; and
- secret/path diagnostics remain bounded and redact user content.

SEC-002, SEC-003, and DATA-001 become **Shipped** only after migration tooling,
rollback, packaging, and canonical documentation are released.

## Observability and operator diagnostics

Record bounded reason classes for invalid IDs, confinement rejection, hostile
entry type, migration quarantine, deletion surface failure, and layout version.
Do not log raw hostile paths, note content, full IDs, or outside-root targets by
default. A local explicit diagnostic export may include escaped relative names
and digests sufficient for recovery.

The migration command supports dry-run, emits counts/bytes/collision groups,
and never removes source data in dry-run. Deletion status is queryable by
opaque deletion ID and safe to retry.

## Canonical documents changed at ship time

- `docs/engine/http-api.md` and session/vault/artifact/media guides;
- SDK reference and migration notes for daemon-generated session IDs;
- vault, data-directory, backup, deletion, and upgrade runbooks;
- configuration reference for registered vault/project roots; and
- contributor/API contract rules for typed path parameters.

## Removal ledger

Delete after migration:

- raw `history/{session_id}.jsonl` and satellite `join(session_id)` helpers;
- turn-ledger and tool-surface lossy sanitizers;
- caller-selected new session IDs and Home local ID generation;
- `ensure_within_root`, lexical prefix checks, and preflight-only containment;
- ordinary path-based vault mutation/deletion after a confinement check;
- best-effort deletion helpers and unconditional `deleted: true`;
- legacy layout readers after the rollback window; and
- duplicated path validators superseded by typed IDs and `VaultPath`.
