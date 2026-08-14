# ADR-014: Typed identifiers and handle-relative filesystem authority

> **Status:** Accepted
>
> **Date:** 2026-08-13
>
> **Decision owners:** daemon storage and vault maintainers
>
> **Related:** [H02 execution plan](../../../architecture/hardening/02-identifier-and-filesystem-authority.md), [security abuse matrix](../../../architecture/hardening/verification/security-abuse-matrix.md)

## Context

Medousa currently lets externally supplied strings serve simultaneously as
logical identifiers and filesystem path components. A non-empty `session_id`
can flow into transcript, catalog, artifact, media, extraction, verification,
context-pack, tool-surface, and deletion paths. Different stores either use the
raw value or apply incompatible lossy sanitizers. `Path::join` does not confine
an absolute path, parent component, platform separator, or link traversal.

Vault paths are intentionally user-visible relative paths, but their current
containment check is lexical. An in-vault symlink or Windows reparse point can
redirect a read, write, rename, trash, or restore outside the selected vault.
Canonicalizing and checking a string before a later operation would still
leave a time-of-check/time-of-use replacement race.

Session deletion compounds the problem: it invokes multiple best-effort
cleanup helpers, ignores errors, uses directory removal on a ledger file, and
returns `deleted: true` without proving that the session's complete data
inventory is gone.

These are authority defects. Escaping or broadening a path must be impossible
by construction, not merely unlikely because one HTTP router rejected one
spelling.

## Decision

### 1. Logical identifiers are typed and never used as paths

Authority-bearing identifiers use validated newtypes at ingress. Each type has
one canonical textual representation, maximum length, parser, serializer, and
property-test corpus. A raw `String` cannot cross from an API adapter into a
store API where an identifier type exists.

New chat sessions use a daemon-generated opaque `SessionId`; clients no longer
mint authoritative session IDs. During migration, a separate `LegacySessionId`
parser accepts only bounded, unambiguous legacy values. Display names and
external-provider correlation keys remain separate fields.

Every filesystem-backed store derives an opaque `StorageKey` from the typed
identifier through one versioned, collision-resistant encoding. Store paths
contain storage keys, not display IDs or caller strings. Lossy replacement such
as mapping punctuation to `_` is forbidden because it aliases distinct IDs.

### 2. Store-owned paths are constructed by store capabilities

Callers ask a store to load, append, enumerate, or delete a typed object; they
do not receive or construct authority-bearing `PathBuf` values. Each store owns
its root handle, layout version, storage-key mapping, atomic-write policy, and
cleanup behavior.

Roots are opened once from trusted configuration. Descendant operations are
performed relative to that directory capability. Absolute paths, `..`, empty
components, alternate separators, control characters, reserved platform
names, and unexpected links cannot change the selected root.

### 3. Vault paths are typed relative paths with a no-follow default

`VaultPath` represents normalized user-facing path segments, not an OS path.
Parsing rejects empty/dot/parent components, absolute or prefixed paths,
backslashes, controls, ambiguous Unicode normalization, Windows device/stream
aliases, trailing-dot/space aliases, and configured length/depth violations.

Authority-bearing vault operations walk from an opened vault or trash root and
do not follow symlinks, junctions, mount-point redirects, or other reparse
points in any ancestor or leaf. Reads, creation, overwrite, rename, trash,
restore, indexing, and recursive enumeration share this primitive.

Links may be displayed as vault content, but the default file API does not
traverse them. Supporting an external linked root in the future requires an
explicitly registered second root capability and a separate decision; a link
inside one root never silently grants another.

### 4. Validation and filesystem resolution are separate defenses

Typed validation prevents ambiguous names and produces stable API behavior.
Handle-relative/no-follow operations enforce confinement against hostile
filesystem state and races. Neither substitutes for the other.

Preflight `canonicalize` plus a later string-path operation is not sufficient
for mutation or deletion. Platform implementations use directory-relative
open/rename/unlink primitives with no-follow/reparse inspection. Where a
platform cannot provide the required atomic guarantee, the operation fails
closed rather than falling back to lexical prefix checks.

### 5. Deletion is an enumerated, truthful store operation

Session deletion is keyed by a validated `SessionId` and owns a versioned
inventory of every session satellite. It cancels applicable active work,
records a deletion tombstone/operation, invokes typed deletion on each owning
store, and reports per-surface outcomes.

The API does not return complete success while required data remains or a
cleanup error was ignored. Deletion is idempotent and retryable. A fresh-process
verification must find no transcript, catalog/meta row, shared membership,
artifact, media, extraction, verification, context pack, tool surface, ledger,
channel reference, or requested memory node for the deleted session.

Adding a new session-owned surface requires updating the deletion inventory and
its completeness test in the same change.

### 6. Legacy data is inventoried, migrated, or quarantined

Migration scans known roots without following links. It decodes only layouts
whose mapping is unambiguous, writes the new representation atomically, and
records source-to-destination evidence before removing the old entry.

Invalid, colliding, truncated, link-backed, or otherwise ambiguous entries are
quarantined and reported. They are never silently renamed into a valid ID,
merged with another session, or recursively deleted. Migration is restartable
and preserves the last readable representation until the replacement commits.

### 7. The boundary applies beyond sessions

H02 begins with `SessionId` and vault paths because they contain confirmed
exploits, then inventories every externally influenced identifier used in a
path. Profile, feed, component, environment, pairing/device, proposal,
package, Forge work/attempt, artifact, media, and similar IDs adopt the same
typed-ID/storage-key rule according to risk.

Externally configured absolute roots use a separate `ConfiguredRoot` type and
explicit operator authorization. They are never parsed as an object ID or
relative vault path.

## Consequences

### Positive

- Traversal strings cannot broaden session storage or recursive deletion.
- Symlink/junction replacement cannot redirect vault authority outside a root.
- All stores agree on identifier equality and filename mapping.
- Deletion becomes auditable, retryable, and honest about partial failure.
- Future path-bearing surfaces have a reusable boundary rather than another
  sanitizer.

### Costs and migration

- Typed IDs touch HTTP DTO conversion, daemon internals, SDKs, Home, CLI, TUI,
  adapters, persistence, and tests.
- Home's current client-generated `medousa-home-{uuid}` sessions migrate to the
  daemon create-session operation.
- Platform-specific confinement requires Unix and Windows implementations and
  tests; a POSIX-only symlink fix is incomplete.
- Existing arbitrary or lossy-sanitized filenames require a cautious inventory
  and quarantine workflow.
- Store APIs will stop exposing convenient raw paths to callers.

## Verification

Acceptance and implementation are governed by FS-001 through FS-008 and the
deletion cases in the [security abuse matrix](../../../architecture/hardening/verification/security-abuse-matrix.md)
and [crash/concurrency matrix](../../../architecture/hardening/verification/crash-concurrency-matrix.md).
The matrix runs on Linux, macOS, and Windows with outside-root canaries and
concurrent link/reparse replacement.

## Code anchors

- `src/interactive_turn_runtime.rs`, `src/daemon/interactive.rs`,
  `src/daemon_handlers.rs` — raw session ingress and creation
- `src/session.rs`, `src/session_store.rs`, `src/session_catalog.rs`,
  `src/shared_session_catalog.rs` — transcript/catalog paths
- `src/artifact_store.rs`, `src/media_store.rs`,
  `src/artifact_extraction.rs`, `src/verification_store.rs`,
  `src/context_pack.rs` — session satellites
- `src/session_lifecycle.rs`, `src/agent_runtime/turn_ledger.rs` — deletion and
  ledger mismatch
- `src/vault/path.rs`, `src/vault/store.rs`, `src/vault/roots.rs` — lexical
  containment and vault operations
- `crates/medousa-types/`, `crates/medousa-sdk/`,
  `apps/medousa-home/` — shared contract and clients
