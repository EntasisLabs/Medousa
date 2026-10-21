# Creative document service

Create's backend is available independently of the canvas renderer. The new main
view is still being built; these contracts do not change existing Drawing notes.
The workshop daemon owns projects under `{dataDir}/create`, including when the
app connects remotely. Clients never resolve project asset paths on their disk.

## Documents and edits

The shared `medousa-types::creative` contract uses schema `medousa-create`, version
3. A project has a stable client-generated ID, a monotonically increasing revision,
metadata, a scene and an asset manifest. Supported scene nodes are ink, text,
rectangle/ellipse/line shapes, images, groups and named frames. Coordinates are
world coordinates, independent of camera position or export size. Node `x`/`y`
translate from the parent coordinate space; `rotation` is clockwise degrees about
the local origin. Width/height describe local geometry or layout bounds. Resizing
ink or groups does not implicitly rescale stored points/children: the command
adapter supplies those changes together in one transaction. Parent references
must resolve to a group/frame and form an acyclic hierarchy. Payloads, object
counts, geometry, text, colors, crops and asset references are validated before
publication. Unknown versions remain untouched and return HTTP 409.

Every human or accepted AI edit uses a `CreativeTransaction`: a stable request ID,
`expected_revision`, label and ordered operations. The whole batch succeeds or
fails. HTTP 409 means the current revision differs; retain the local draft and
read the latest document before explicitly reconciling it. Do not automatically
rebase a suggestion onto newer human work.

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
