# Creative document service

Create's backend is available independently of the canvas renderer. These
contracts do not change existing Drawing notes.
The workshop daemon owns projects under `{dataDir}/create`, including when the
app connects remotely. Clients never resolve project asset paths on their disk.

## Documents and edits

The shared `medousa-types::creative` contract uses schema `medousa-create`, version
3. A project has a stable client-generated ID, a monotonically increasing revision,
metadata, a scene and an asset manifest. Supported scene nodes are ink, text,
rectangle/ellipse/line shapes, images, groups and named frames. Coordinates are
world coordinates, independent of camera position or export size. Node `x`/`y`
translate from the parent coordinate space; `rotation` is clockwise degrees about
the stored width/height box center. Width/height describe local geometry or layout bounds. Resizing
ink or groups does not implicitly rescale stored points/children: the command
adapter supplies those changes together in one transaction. Parent references
must resolve to a group/frame and form an acyclic hierarchy. Payloads, object
counts, geometry, text, colors, crops and asset references are validated before
publication. Unknown versions remain untouched and return HTTP 409.

`CreativeNode.name` is an optional layer label in version 3, limited to 256 UTF-8
bytes and nonblank when present. Unnamed nodes omit the field on serialization;
existing documents need no migration. The label does not change text content or
geometry. Updated app and daemon builds are needed to save labels; older daemons
reject unknown node fields. Grouping and ungrouping use existing ordered
`put_node`, `remove_node`, and `reorder` operations in one atomic transaction,
including explicit child coordinate updates before removing a group.

Text content additionally accepts optional `font_weight` (100–900) and
`font_style` (`normal` or `italic`). Legacy text omits both and renders with
weight 400 and normal style. These optional version-3 fields require an updated
daemon. Home measures text with its resolved font for rendering, editing,
selection, and hit testing, including older nodes with estimated stored bounds.
Text edits persist fitted width/height alongside the content in the same
transaction; a font change preserves the local top-left anchor of rotated text.

Line shapes additionally accept optional `line` geometry: local `start`/`end`
points (`x`, `y`), `start_arrow`, `end_arrow`, and `dashed` booleans (default false).
The field is valid only on `shape: "line"`; coordinates must be finite and within
±1e9. Omitting it preserves legacy endpoints `(0, 0)` to `(width, height)`.
Stroke color and thickness use the existing shape fields. Endpoint edits store
the geometry and fitted transform in one transaction; rotation uses the existing
node transform and ordinary undo history. These optional version-3 fields require
an updated daemon. Older daemons reject them; legacy lines remain readable.

Every human or accepted AI edit uses a `CreativeTransaction`: a stable request ID,
`expected_revision`, label and ordered operations. The whole batch succeeds or
fails. HTTP 409 means the current revision differs; retain the local draft and
read the latest document before explicitly reconciling it. Do not automatically
rebase a suggestion onto newer human work.

`remove_node` removes the target and all its descendants, irrespective of stored
node order. Explicit descendant removals later in the same batch are accepted;
an unrelated missing target still rejects the entire batch. Undo restores the
subtree and its original order. The server and optimistic canvas use the same
subtree semantics.

Create work uses the existing Forge execution service and its shared byte budget.
Simultaneous startup reads briefly retry admission before any store work begins;
they never replay a mutation after execution starts. Saturation remains bounded
and returns HTTP 503 when the admission retries are exhausted.

Retries must reuse the exact request ID and body. The last 128 mutation receipts
are retained. A duplicate returns the current document and the original
`applied_revision`, with `duplicate: true`; older requests fail their revision
check instead of reapplying. Creation retries reuse the original project ID and
creation body. Reusing an ID with different input is rejected.

Undo/redo also requires a stable request ID and expected revision. Revisions
continue increasing through history operations. History stores changed nodes and
ordering, with at most 100 entries and 4 MiB of serialized deltas; image bytes
never enter history. An individual edit larger than the history budget is rejected.
A new edit clears redo. Assets remain referenced across undo and branch changes.

## Assets and proposals

Import PNG, JPEG, GIF or WebP bytes using `CreativeAssetImportRequest.data_base64`.
The backend identifies the bitmap container by its signature; decoding/rendering
still needs its own pixel and memory bounds. It rejects SVG/HTML and arbitrary
URLs. Each immutable asset has a SHA-256 identity and belongs to its project.
Import publishes bytes before the manifest and recovers retries after an
interrupted manifest write. Reading verifies the digest. Import increments the
document revision: read the latest document before placing the image node.

The same asset-import operation accepts optional `context: { expected_revision,
selected_ids }` for a rendered selection snapshot. It stages a PNG up to 4 MiB
separately from the document manifest and history and returns a `CreativeAsset`.
The client uses the same SVG renderer as export, crops in world coordinates,
removes editor chrome, and bounds the rendition to 1,536 px per side. Up to 32
unsubmitted captures are retained in the working record. Submit its ID as
`CreativeJobRequest.visual_asset_id`; admission validates the saved revision,
selection IDs and fingerprint and consumes the staged descriptor. Identical
admission retries still return the original job. Snapshot payloads remain on disk.

`promote-media` copies an existing uploaded/generated session image into project
ownership while holding a session mutation lease. Source-session deletion cannot
race the read, and a later deletion cannot remove the project copy. It reuses the
existing media store, generation port and credential flow.

Selection context contains selected nodes and their descendants, asset descriptors, document
revision and a fingerprint, with a 512 KiB limit. It does not contain bitmap bytes
or the whole chat. The existing `cognition_store_read` action `creative.read`
exposes that context. `cognition_store_write` action `creative.promote_media` promotes an image from
its bound conversation, and action `creative.propose` persists a
typed suggestion; it never accepts the suggestion on the user's behalf.

Proposals capture an expected revision and selected IDs. Operations may edit
selected nodes or create children in the captured selection; project-wide metadata
and ordering changes are excluded. Accepting a ready proposal is one validated,
undoable transaction. A revision conflict leaves the proposal available for
inspection or cancellation. Up to 32 proposals and 2 MiB are retained per project;
terminal proposals are evicted before ready ones. Cancellation advances the
revision fence, so evicting an old terminal entry cannot resurrect it on retry. This is the proposal foundation,
not a separate inference worker or image-generation scheduler.

## Durable creative jobs

`POST /{id}/jobs` admits a `CreativeJobRequest` with a stable ID, expected
revision, selected IDs and prompt. It atomically captures the original document
and bounded selection before returning `queued`. An identical retry returns the
same job, session and turn; changing input or owner under that ID is rejected.
Job reads/decisions are scoped to the bound profile. Trusted local app/CLI and
root credentials resolve the active workshop profile; that identity is captured
on admission. Personal-mode operator Portals use the existing workshop-owner
resolver too. Shared/member credentials must carry their own profile and cannot
fall back to the local operator. Admission and decisions also
require `workshop.interact`, in addition to content authority.

Optional `parent_job_id` continues an owned job's conversation within the same
project. Clients cannot supply an arbitrary session ID. A follow-up captures the
latest saved revision and selected objects. With `refine_proposal: true`, its
parent must have a current ready preview with the same selected IDs. The captured
context includes that preview's edits and staged assets. Result validation checks
the incremental changes against that effective scene, then normalizes the full
proposal against the unchanged human base. Applying the replacement remains one
ordinary transaction and undo step. Earlier ready/stale parent previews become
`superseded`; their results cannot be accepted. New IDs without a parent start
fresh conversations. One executing/queued request per conversation is admitted.

Before dispatch, Forge-admitted media work copies the snapshot and selected
original images into the existing session media store using stable job-scoped
identities. Normal runtime `media_refs` deliver vision context, and the prompt
maps source asset IDs to session media IDs for image-generation references.
The admitted media refs are fenced along with the job's prompt, profile, mode
and tool ceiling. The combined snapshot/original-image count is at most five.
Each turn persists its user input in the same existing runtime conversation;
session turn admission serializes follow-ups until the previous turn finishes.

A small durable outbox index finds pending work without polling idle project
records. Each entry is reconciled independently, so a damaged project cannot
block unrelated dispatch. The daemon dispatches through the existing native Assistant runtime,
model routing, image-generation port, turn streams and cancellation token. It
runs at most two creative turns concurrently and uses an exact tool ceiling:
store read/write, schema discovery, image generation and turn control (finish only). During a creative turn,
store actions can only read its captured selection, propose its result or promote
media from its bound session. Generated media and reference lookups use the active
turn session, never the shared runtime's bootstrap label. It cannot create unrelated worker jobs or mutate
vaults/code. No additional provider client, credential flow or model executor is
introduced. Execution has a 30-minute runtime deadline.

The persisted state moves from `queued` to `dispatching` before runtime admission,
then `running`. A synchronous capacity rejection can safely return to `queued`.
After restart, queued jobs dispatch normally. A dispatching/running job without a
live runtime ticket becomes `interrupted`; the daemon never blindly replays an
execution that might have called a provider before crashing. Retry an interrupted
or failed job with a **new ID**. This is durable, at-most-once dispatch, not an
exactly-once provider guarantee. A persisted proposal survives a lost runtime
acknowledgement or later error.

In a creative job, `creative.read` returns the immutable admitted context.
`creative.propose` must use the job ID, original revision and original selected
IDs. Its operations are validated against the captured document and locks. The
result is stored atomically as `ready` or `stale`, independently of later human
edits. Later edits/undo also turn ready jobs stale. Stale results remain readable
but cannot be applied; submit a new job against current work. A turn that ends
without producing a proposal becomes `failed`.

Generated images promoted during a job are staged as immutable project-owned
assets without advancing the document revision. Accepting a ready job publishes
its asset manifest and edits in one ordinary undoable transaction. A duplicate
accept returns the original applied revision, even after undo, without applying
again. Project assets survive source-chat deletion and undo.

`POST /{id}/jobs/{job_id}/decision` uses `CreativeProposalDecision`. With
`accept: true`, both the supplied revision and admitted revision must equal the
current document revision. With `accept: false`, it cancels the job regardless of
newer document edits; `expected_revision` is unused for cancellation. Cancellation
is persisted before signaling that exact runtime turn. Late results/admission
acknowledgements cannot resurrect cancelled work. Accepted edits use ordinary
undo rather than cancellation. Deleting a project cancels queued/executing jobs
and keeps their records readable to their owner.

Working job storage retains at most 32 recent/unresolved jobs and 8 MiB of metadata, contexts,
results and active validation snapshots. Admission snapshots are limited to 2 MiB,
selection contexts and results to 512 KiB each, prompts to 16 KiB, and staged job
assets to 16 per job within the shared project asset budget. Snapshots are released
on completion/cancellation/failure. Finished jobs are archived under their original
IDs before the working set grows beyond 32 entries or 6 MiB, preserving headroom
within the overall 16 MiB project-record ceiling. Reads by ID and identical
admission retries resolve archived jobs too; changed input or owner cannot replay
an old execution. Lists return the recent working set; full conversations remain
in the existing runtime transcript. If only unresolved jobs occupy capacity,
admission asks the caller to finish or dismiss one. Images remain on disk outside
job metadata. These fields and the `superseded` status require an updated daemon;
older requests without visual/continuation fields remain supported.

## HTTP surface

All routes require native authenticated content authority. Reads, including the
POST context request, require `content.read`; mutations require `content.write`.
There are no public preview or unauthenticated asset routes.

| Method | Path below `/v1/create/projects` | Request → response |
|---|---|---|
| GET | `/` | `CreativeListQuery` → `CreativeListResponse` |
| POST | `/` | `CreativeCreateRequest` → `CreativeDocument` |
| GET | `/{id}` | → `CreativeMutationResponse` |
| POST | `/{id}/context` | `CreativeContextRequest` → `CreativeSelectionContext` |
| POST | `/{id}/transactions` | `CreativeTransaction` → `CreativeMutationResponse` |
| POST | `/{id}/history` | `CreativeHistoryRequest` → `CreativeMutationResponse` |
| POST | `/{id}/delete` | `CreativeDeleteRequest` → `CreativeDeleteResponse` |
| POST | `/{id}/assets` | `CreativeAssetImportRequest` → `CreativeAsset` |
| GET | `/{id}/assets/{asset_id}` | → `CreativeAssetData` |
| POST | `/{id}/promote-media` | `CreativePromoteMediaRequest` → `CreativeAsset` |
| GET | `/{id}/proposals` | → `CreativeProposalListResponse` |
| POST | `/{id}/proposals` | `CreativeProposalRequest` → `CreativeProposal` |
| POST | `/{id}/proposals/{proposal_id}/decision` | `CreativeProposalDecision` → `CreativeMutationResponse` |
| GET | `/{id}/jobs` | → `CreativeJobListResponse` |
| POST | `/{id}/jobs` | `CreativeJobRequest` → `CreativeJob` |
| GET | `/{id}/jobs/{job_id}` | → `CreativeJob` |
| POST | `/{id}/jobs/{job_id}/decision` | `CreativeProposalDecision` → `CreativeJob` |

Listing uses stable ID order with an exclusive cursor and limits of 1–100. The
client may order the returned summaries by timestamp or pinned status. Deleted
projects are tombstoned; source, history and assets are preserved. Automatic blob
collection, cross-workshop packaging, renderer selection and the canvas UI are
follow-up work. Do not manually remove assets still referenced by history.

## Persistence and budgets

Storage uses held `StoreRoot` capabilities, confined paths, advisory per-project
cross-process locks and atomic file publication. Reads observe a complete old or
new record. Blocking work is admitted through `ForgeExecutionService`, never
performed directly on an async worker. Create reserves a full 8 MiB execution
admission slot; at the default shared queue budget one Create operation runs at a
time. Busy admission returns HTTP 503 and clients retain their pending request.

Scene source is bounded at 8 MiB, 10,000 nodes and 250,000 total ink points. Assets
are bounded at 25 MiB each, 256 descriptors and 512 MiB per project. Records are
bounded at 16 MiB. These are source/storage limits, not decoded-renderer budgets.
Base64 transport is on demand; asset bytes must not be cached in document sessions.

Home's headless session pool shares one session per workshop/document between
panes and embeds, retaining up to eight sessions by default. Only unused clean
sessions can be evicted. A full pool with active/dirty work refuses another
project rather than dropping a draft. Failed saves and history requests retain
their exact identity for retry; refreshing cannot silently replace a pending edit.
The existing v1/v2 parser remains authoritative for old ink before pure scene
conversion. Adoption IDs include workshop, vault root, note path and a stable
anchor. Accepting adoption must persist the resulting document reference in the
note using its existing revision check; the conversion helper alone never edits
the source note. Notes rendering and that adoption UI have not shipped yet.
