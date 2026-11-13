# P1a / A — corrected proposed session, draft and recovery contract

**Status: DOCS-ONLY correction; NOT implemented, tested or frozen.** This revision
supersedes the initial P1a proposal, not the sealed qualification evidence. Both
independent read-only reviews were completed and sender accepted their reports;
that acceptance was NOT a contract freeze. Principal architect + independent C
must review this revised contract, then sender must checkpoint, before runtime
implementation. No workers/reviewers are dispatched here. Names below are proposed
app-private shapes, not existing SDK/protocol APIs or permission to expand scope.

## 1. Binding, scope and verified existing contracts

- Current narrow failure-scope/assertion correction entry: clean
  `db962009a32b1529e07c37086be945f056cca845`, parent
  `1863288c18288db650c0197b2eeaa3642010a876`; sole parent diff CONTRACT 207+/30-,
  867 lines, SHA-256 `bdf8a503278dd00604f1bb77f38f59eac5bde5909505700239caf943d31aafee`.
  Recovered FULL original architect turn9 handback in the same session below:
  14,190 characters, digest
  `sha256:0eef872372b05c36c635cecc7a453f76650394c6e9e85688b733eeef18fe6e67`,
  digest-fenced continuation through end. Sender concurs with its two narrow
  corrections: selected failure-scope recommendation and routing/application
  assertion attribution. Accepted architect/C review deliverables are not freeze;
  original architect AND independent C must rereview this exact corrected revision
  before sender checkpoint. No new API/runtime behavior is established here.
- Prior acceptance-mechanism correction entry: clean
  `1863288c18288db650c0197b2eeaa3642010a876`, parent
  `50fd60ae7ed5d097b77fbe49e325656077332102`; sole parent diff CONTRACT 215+/48-,
  690 lines, SHA-256 `545f65696198aa45ab2f79fb13306a10edbb1eb9ebd9c6e652f06525cecab550`.
  Recovered the FULL principal architect reconciliation, same architect session
  below, turn8: 16,595 characters, digest
  `sha256:61a425675c97c0a83a0a51f2b89d8988662272dbce726a2836de36eba960d74a`,
  digest-fenced pagination through end. Assignment
  `peer_assignment_d8f52adf0a2f15394a3c4a4089c0afe3577e5d0a60c9b4f48efc282f1d85c4a1`,
  receipt `peer_terminal_946ff88aecb048f8b13d3db4c3f22dd63e371ae5a821bb11ff284fad5c2cb92b`.
  It reconciles C's complete 15,132-character turn3 report in the C session below:
  architect concurs with C, preserves A–D/finite retirement, and selects explicit
  controller-issued activation/settlement, NOT speculative destination follow.
  Sender accepted that reconciliation DELIVERABLE, not this contract or freeze.
- Prior A–D rereview-correction entry: clean `50fd60ae7ed5d097b77fbe49e325656077332102`,
  parent `9960e0d89ec00acaf61a397485e29d48d3a54219`; input CONTRACT was 523 lines,
  SHA-256 `a7e95f826ef453497ed4173696a3fb87f4d409b62bddd3755476a25789788965`.
  Both completed rereviews were read IN FULL: architect session
  `ses_peer_b83c19fefe5ef7896d50260fa236f2da9a5ad5c1f1aad7c314d8187a61d144a3`
  turn6 (20,433 characters), independent C session
  `ses_peer_9e755d6809eba8abd3275a5ed2717ebe4a8c1853d5de4b3e761f08713d788c3d`
  turn2 (16,386 characters). Sender accepted REVIEW DELIVERABLES, not freeze.
  A–D below are DESIGN PROPOSALS requiring architect+C rereview and checkpoint.
- Worktree: `/Users/theelevators/Library/Application Support/medousa/forge/worktrees/b906e0f57650/medousa-rndr-home-current-main-preview-requalification`.
- Execution candidate remains `bdef72eb7fbf8014067603f50bb15cfe0a3745f5`.
  External rndr pin remains `44f5dc7c0c91af56e4f3e29f97283b8e8d1e1139`; its
  checkout is not accessed. Production Home and all 55 sealed files/manifest stay
  unchanged. Original P1a base was `93f844648e68bd0e330a4b222f1ce89517f663be`.
- Only `apps/rndr-home/docs/p1-a-contract/CONTRACT.md` may change/commit. No source,
  manifests, lockfiles, SDK/framework/daemon, existing evidence or shared docs edits.
  No Cargo/build/test binaries/app/export/native execution, docs-verifier rerun,
  evidence-script execution, external rndr access, cache cleanup, network/provider,
  credentials/private data, push/deploy/merge. No build slot used; retain 8 GiB free
  floor and future single shared target/max two jobs/debug and incremental off.

App source references below are relative to `apps/rndr-home/src/`. These are
constraints, not claims that the proposed adapters already exist:

| Existing symbol | Verified constraint / proposed owner |
|---|---|
| `state.rs::Model` | One active Chat, loading/generation, global recovery vector/index today; architect serializes new shapes with A. No inactive transcript cache. |
| `controller.rs::update` | Replies applied before actions; action intents retain their order and native Submitted events are appended separately. Stale-generation exceptions are only Models/Modes/Unavailable. A owns transition decisions, not editor internals. |
| `lib.rs::Chat::{history,resume,begin,admitted,fail,apply}` | Stable session authority; busy/uncertain/nonblank Send guards. `apply` rejects !busy, wrong turn, nonincreasing sequence, but currently returns unit. `resume` clears chat.uncertain; controller must restore the ledger authority below. |
| `backend.rs::{Request,Reply,Command,Update,history,settled_history,follow,mode}` | New/Recent/Open/Send/Cancel/SetMode exist. Bounded bracketed history, execution-owned assistant filtering, non-atomic notices and bounded reconnect remain. Local correlation/typed failures need architect-owned shapes. |
| `composer.rs::{reset_prompt,guard_submit_boundary,Handles}` | One Prompt/Label entity. Reset inserts FieldTransactions synchronously, asserts reset_field_draft, removes marker, sets scroll=0. SubmitBoundary reads batch keys, ime_preedit and ime_commit; it is NOT persistent composition ownership. C owns mechanics, never session Model/SDK dispatch. |
| `main.rs::app` and connected startup | Input guard registered at front of Input; controller Update follows. Connected --session currently sends Open directly with generation 0/loading=true and no proposed pending context. Architect must wire an actual startup Navigation context, not exempt History from routing. |
| `shell.rs::{Ui,sync}` | B owns UI projections and frozen action emission. Synchronous history/live take/restore does not confer transition ownership. |
| `fixtures.rs::sync_observation` | Explicit FixtureObservation opt-in plus current five checks (no sender/history/live/recovery/Label text) is insufficient for new private identity/metadata. Early denial and isolated cases required, section 7. |

SDK read contract: `crates/medousa-sdk/src/sessions.rs::SessionsApi::list` accepts
only `limit: usize`; shared SessionHistoryListResponse has sessions/next_cursor.
Summary IDs, optional display_name, last_timestamp and turns suffice. Cursor output
is NOT an admitted cursor input. No raw HTTP, new endpoint, arbitrary-ID UI,
history_page substitution, limit-growth pseudo-pagination or SDK edit.

Current worker behavior is specifically NOT proof of controller acceptance:
`backend.rs:297–308,370–395` aborts old follow on navigation receipt and emits
History then reads mode/starts follow without controller feedback. Its `177–207`
uses raw terminality for settlement and a subsequent mode read. `lib.rs:78–84`
returns unit after validation; `controller.rs:77–85` separately tests terminality.
Sections 6.1–6.3 propose replacing these acceptance edges with app-private controls,
not new daemon/SDK APIs. None of those changes exists in this docs pass.

Historical preparation references remain PREPARATION_CLOSURE.md, TEAM_PREPARATION.md,
INDEPENDENT_REVIEW_0_6_0.md and the original review-prep correctness/UX REPORT.md
packets. Their bytes/claims are historical, not this pass's execution evidence.
Sender docs-verifier exit-1 discrepancy remains unresolved; no rerun here.

## 2. Ownership, reachability and bounded projections (architect R1 / C R1)

### 2.1 One live owner

`SessionKey = New(u64) | Stable(String)` uses exact lossless SDK IDs in this
process's fixed daemon connection: no title/prose/timestamp/case-fold identity.
Initial key New(0) has no Chat.session and cannot Send. Selected Stable equals
Chat.session even when only a local, unreconciled projection is displayed.
The selected key has NO entry in `inactive_drafts`. Only the mounted Prompt/Label
owns its mutable committed text. `DraftSnapshot { text, revision }` stores exact
UTF-8 inactive text only, never another editor, native state, caret, scroll or
transcript. Nonempty snapshots never expire/evict/truncate. Empty entries may be
omitted. Process exit loses memory; Reload here means history read, not restart.

There is at most one ordinary retained unsent New owner. `draft:unsent` explicitly
selects it, or allocates a fresh token if absent; already selected New is a no-op.
Transient create targets allocated from Stable own no text until migration and do
not consume an unrelated unsent owner. No automatic identity merge or text matching.
Every ownership change, including New -> Stable with unchanged text, advances the
editor opportunity epoch and invalidates captured ownership tokens.

### 2.2 Remote recent projection stays separate

`sessions:list` makes exactly one SDK list(50), only on explicit action. Preserve
server order, collapse duplicate exact IDs to first, discard empty IDs, cap at 50.
Keep only exact ID, display_name (display capped at 120 Unicode scalars), timestamp,
turn count; discard preview/other body metadata. Never log raw IDs/text/responses.
`session_rows`, `list_revision`, Ready/Failed/Never and sanitized list_error are
independent of chat/recovery. Failure keeps last good rows; empty success clears
rows, not selection/chat/drafts. Successful refresh increments revision. Show
“Recent 50 only; older-page navigation unavailable in this slice” if next_cursor
or overreturn. No Load More, startup loop, search, background refresh or reconnect.
Row actions encode lossless hex UTF-8 IDs plus list revision; validate strict decode,
revision AND exact current membership, not index or display text. A list is not a
lease: missing/deleted history can still fail honestly.

### 2.3 Independent retained-key projection and local lossless retrieval

`retained_rows` is a derived, bounded union of inactive Stable draft owners and
Stable recovery owners, including recovery-only owners; not a filtered recent50.
With proposed counts 32 drafts and 64 recovery entries, it has at most 96 distinct
Stable IDs (selected owner may be omitted). Rows contain exact key, draft-present,
recovery count and availability status, NO text/transcript. Own checked monotonic
`retained_revision` changes on any membership/row-fact change. Actions require that
revision and exact current union membership. Do not import remote-list absence as
delete, remove a retained owner on failed Open, or claim stored bytes are reachable
without the following B UI paths. Unsent and creation-outcome UI are separate.

B exposes “Retained drafts / recovery” independently of remote Recent, with:

1. `retained:open:<revision>:<hex_id>`: same transactional remote Open as a recent
   row (section 6), never arbitrary ID/new endpoint. On missing/deleted/typed read
   failure source remains selected and retained bytes/row remain. Static error:
   remote history unavailable; local draft/recovery can still be retrieved.
2. `retained:local:<revision>:<hex_id>`: explicit bounded, offline local navigation
   to that exact retained owner; no Command. Validate idle/no loading, membership,
   capacity, editor/composition safety. Atomic latest-source snapshot/target draft
   move uses the SAME ownership-transfer transaction, but installs an empty local
   Chat/history projection marked “Local retained text only — history not loaded;
   Reload required before sending.” It sets target uncertainty in the ledger,
   never asserts remote existence/deletion or cached history. Selected-only Restore
   can then move one recovery entry into an empty editor; a nonempty draft must be
   deliberately cleared/handled by the user first. No automatic recovery-to-draft
   copy or command. B keeps explicit Reload and local unsent/retained paths visible.

**D — selected-key retained-local DESIGN PROPOSAL:** after captured row revision/
membership validation, selecting the exact already-selected key is NO-OP. No
snapshot, map insertion, native reset, history/projection replacement, uncertainty
change or generation increment; no transition-driven epoch increment beyond the
normal controller-entry opportunity tick. This covers selected Stable(S) with live
draft AND recovery. Selected rows may be displayed or omitted; selection never
means Restore. Restore stays separate and refuses nonempty editor. Busy/loading
gates still apply, so no-op is not a bypass. Only DISTINCT-owner local selection
installs the empty uncertain projection described above.

Local retained/unsent transfers are the narrow exception to uncertain navigation
refusal: allowed when !busy && !loading even if source uncertain, with its ledger
entry/recovery preserved, target Stable marked uncertain. Remote navigation and
Send remain refused until reconciliation. This exception permits retrieval, not
execution/admission. Local paths still obey all capacity/composition gates; at a
full bound they explain which deliberate draft action is needed, never evict text.
If composition safety cannot be established, section 4's blocker prevents promising
lossless ownership transfer. No transcript is retained on local or remote switches.

## 3. Capacity and creation transactions (both reviews R2 / R3)

### 3.1 Net capacity and reservations

Proposed bounds remain 32 inactive nonempty draft keys / 64 recovery entries total,
not byte-memory guarantees. No speculative snapshot, deletion, copies or eviction.
For a distinct-owner transfer, atomically validate:

`N_after = N_inactive - consumed_nonempty_target + nonempty_latest_source <= 32`.

Here consumed target means an exact guaranteed snapshot removed by THIS commit;
not a matching ID guess, empty entry, speculative future deletion, or unrelated
recovery. At a remote-navigation BEGIN reserve the worst case source contribution
of 1 even if currently empty: typing during loading must fit. Credit only a target
snapshot whose exact owner/revision and consumption are guaranteed by the pending
transaction. It is locked against other consumers while loading (editor typing
alone continues); revalidate at commit. At 32, an exact target exchange can proceed
without a temporary 33rd entry. An absent target requires a free reserved slot.
New from Stable has no target draft credit. Recent has unknown target at begin:
reserve a free slot, do not speculate that list(1) will free one. Typed failures,
RecentEmpty, Unavailable and WorkerClosed release reservation with no text loss.
No-op/refused actions never reserve or enqueue. Counters checked, never wrap.

Same-owner New migration and Reload require NO additional slot or snapshot. For
CreatedRetry with inactive origin New(D), credit only the exact consumed nonempty
origin-New snapshot, not an independently owned Stable(created_id) snapshot. If
origin New is currently live, migration keeps its latest text (same-owner case).
If another source is live, source contributes 0/1 and origin New is consumed once.
Synchronous local unsent/retained transfers need no across-frame reservation: use
actual latest source text and exact target consumption in the net equation.
Validate counters, keys, capacity, live entity and adapter/composition preconditions
before taking entries; commit exchange synchronously without observers seeing a
half-transfer. Recovery capacity is checked before Send/enqueue or destructive
clear. Refuse with honest user-directed clear/Restore guidance, never silently
remove pending/recovery to make room. Restore moves an entry into selected editor;
it is not a navigation slot allocation.

### 3.2 Creation lifecycle, exclusive identity and collisions

**B — bounded creation-outcome routing DESIGN PROPOSAL, not an SDK API:**
`creation_outcome` is ONE slot: None / Creating / KnownCreated / Unknown. Each
non-None state retains checked token, origin/target New key, original create op
AND dispatch generation. KnownCreated retains exact ID. Retry has its own navigation
op; it does NOT replace original receipt authority. No archive or tombstones.
Creating is allocated only for an Enqueued New. Matching Unknown moves Creating
-> Unknown; Known updates only for a nonempty exact SDK ID. A protocol-conflict
flag, if needed for contradictory IDs, belongs inside this same bounded slot.
Refuse New while unresolved. Proven no-dispatch Offline/WorkerClosed creates no
remote outcome; after Enqueued, no-ID failure/lost result is honestly Unknown.
Navigation/list/Reload/local retrieval/error/Unavailable never silently clears it.

Propose app-local `CreationOutcome{token,create_op,dispatch_generation,new_key,
result: Known(id) | Unknown}`. Backend emits Known immediately after SDK create
returns ID, BEFORE awaiting history, and repeats that correlated identity attached
to creation History/NavigationFailure. SDK history error AND bounded-history
exhaustion carry the known ID; never another create. No-ID failure emits Unknown,
not a noncreation claim. This adapter behavior does NOT already exist.

Demultiplex attached creation outcome FIRST, independent of navigation. This is
one explicit proposed NEW stale-generation exception, limited to outcome-only
bookkeeping against the exact unresolved slot's token/create_op/dispatch_generation/
new_key. Current selection, current generation and pending Navigation are NOT
receipt authority. A bare stale History/failure is still rejected. Matching Known
refines Creating/Unknown -> KnownCreated; identical duplicate is inert; Unknown
cannot downgrade KnownCreated. Conflicting IDs retain first ID and block migration
for diagnosis, never merge. Refinement CANNOT navigate, select, change editor/epoch/
generation/history/projection/uncertainty, start follow, release another op's
reservation, resurrect abandoned migration or enable Send.

| Creation boundary | Outcome-only effect; navigation is separately gated |
|---|---|
| Known before history success/failure/exhaustion | Retain ID; failure/refused commit leaves KnownCreated. Only independently eligible matching History may migrate once, then close slot. |
| Unavailable before identity | Enqueued Creating -> Unknown with SAME receipt authority; release own Navigation. Later matching Known retained even after local navigation/old generation; no automatic migration. |
| Unavailable after identity | Keep KnownCreated; Unknown/error cannot downgrade it. Lost pending Navigation prevents late History transfer/follow. |
| Late Known while Unknown unacknowledged | Refine visible outcome to KnownCreated, offer Retry-Open/abandon, preserve original text association regardless of local selection. |
| History/failure after pending loss, before acknowledgement | Extract exact live receipt only, retain ID, reject navigation part. Non-create stale results gain no exception. |
| After successful migration, explicit known abandon or unknown acknowledgement | Slot closes/None. Later receipts, INCLUDING known IDs, are deliberately ignored at the disclosed receipt-retirement boundary; never resurrection or a noncreation claim. |
| Newer token in slot | Old token/op/generation/key fails matching and has NO effect, even with ID; cannot overwrite new outcome/status. New requires prior closure. |

**Exact acknowledgement boundary:** only an eligible explicit local action at APPLY,
U=None, matching token AND kind/known ID, retires this slot. Before confirmation UI
must say: “Text is retained. This stops tracking this creation; any later result,
including a session ID, will be discarded. A remote session may exist. A later New
creates another session, not a retry.” Known abandon additionally shows known ID
in the private outcome UI before closure, and does not delete remotely. Unknown
acknowledgement cannot claim an ID will never arrive. If Known arrives in reply
pass before captured Unknown acknowledgement applies, kind mismatch refuses that
action: show Known and require a new deliberate decision. Thus an accepted known
identity is not silently lost under exact-pending-only routing or relabeled Unknown.
AFTER explicit retirement, late identity is intentionally ignored as disclosed;
there is no promise of permanent retention. No late notice overwrites newer state.
No unbounded archive, automatic list/reconnect or invented SDK API recovers retired
receipts. Architect+C must explicitly accept/revise this finite retirement tradeoff.
All abandon/acknowledgement bullets below include this FULL disclosure/boundary.

- New on selected New creates using that owner; success + valid history migrates
  identity only, preserving latest live draft. New on selected Stable reserves a
  fresh empty target New token; success snapshots source and selects empty Stable.
- KnownCreated exposes only explicit `sessions:retry-created:<token>` using
  Open(exact recorded ID), never New/Send. History success performs exactly one
  origin-New -> Stable move; repeated Retry after completion is stale/no-op.
- A collision includes an independently owned LIVE Stable(created_id) OR inactive
  Stable(created_id), regardless of text equality or emptiness. Retry refuses
  commit and preserves both owners; never insert a selected-key draft-map entry.
- Until KnownCreated resolves, EVERY ordinary Open path to that ID (remote row,
  retained remote/local, Recent result, startup Open if applicable) refuses with
  “Use Retry created session” and keeps all owners intact. Recent may discover ID
  only after its read; reject before ownership commit, release reservation. No
  second migration rule. A target selected through any unexpected path is still
  independently live ownership and fails collision validation.
- Counterexample: New(D) -> create S/history fails -> ordinary Open S -> type E ->
  Retry cannot overwrite D or E: ordinary Open S is refused, so E remains with D's
  current owner or another explicit owner, never a secretly selected S. Retry must
  still validate live/inactive collisions. No text equality reconciliation.
- KnownCreated explicit `sessions:abandon-created:<token>` acknowledges remote
  session may exist, ends automatic migration association, keeps all live/inactive
  text under original owner, and performs no remote deletion. The text remains
  reachable through unsent/retained paths; later deliberate ordinary Open is a new
  independent selection, NOT resurrection of abandoned migration.
- Unknown exposes NO Retry-New shortcut. `sessions:ack-unknown:<token>` explicitly
  acknowledges “Creation outcome unknown; a session may exist. Text is retained.
  A later New is a NEW deliberate creation, not retry/replay.” Only then may a later
  separate New action begin. Captured earlier New cannot become eligible because
  acknowledgement happened first in application order (section 5). Explicit abandon
  of Unknown has this same acknowledgement semantics, not a noncreation assertion.

## 4. C mechanics, opportunity epochs and active composition blocker

A validates owner/epoch and chooses transactions; C owns same-entity editor
mechanics only. Proposed synchronous API (not yet frozen): snapshot_prompt obtains
exact committed Label text only at ownership transfer; replace_prompt wraps the
existing reset_prompt; clear_prompt_if_current runs only after A's key/epoch check
and current Label == submitted snapshot, as a destructive-clear guard ONLY.
Identity/admission/recovery never compare prose. Conditional clear occurs only
synchronously after successful enqueue, never on later Admitted/History. Failed
enqueue keeps editor/recovery. Reset retains entity, synchronous FieldTransactions
insert/remove, scroll zero, native draft/caret reset; Reload/theme do not reset.
Prevalidate existing entity conditions before taking map entries; no claim of
arbitrary panic recovery, new detector, framework callback or changed framework API.

At controller entry after native Input increment checked `editor_epoch` each Update
(even unchanged), and again before programmatic replacement or owner migration.
This is conservative opportunity invalidation, NOT keystroke provenance. Inactive
revisions are provenance, never reused as live epochs. Capture tokens before
replies; a reply-driven New -> Stable invalidates them even with identical text.
Native Submitted payload is NOT certified Enter-time text. Retain SubmitBoundary's
ambiguous-batch guard; same-frame exact native edit chronology is not promised.

**Active uncommitted IME policy and concrete blocker:** no ownership transfer,
programmatic replacement or destructive clear may discard/reattribute active
preedit. **C — DESIGN PROPOSAL: FAIL ownership commit WITHOUT transfer, never
indefinitely defer/cache History.** At capture AND immediately before remote
New/Recent/Open/Retry dispatch require proven safe SOURCE editor/composition
preflight plus capacity/ownership gates. Otherwise no command (especially create),
reservation or source mutation. This is not destination validation or a promise
that composition will remain safe until a reply arrives.
At matching successful History recheck latest source native/editor safety and all
destination predicates. Unsafe/unknown means fail commit: release ONLY its own
reservation/pending Navigation/loading, preserve source identity/native editor,
text/scroll/caret/history/projection, and require a new deliberate Open (created
ID uses Retry-Open). No map take/snapshot/reset, automatic retry, cached History
replay or replayed New. A committed
Label snapshot does NOT preserve preedit. Verified app code exposes batch
Input.ime_preedit / ime_commit to SubmitBoundary and reset_field_draft via a
transaction marker; it does NOT establish a persistent active-preedit query,
lossless preedit snapshot/restore API, nor proof an empty later batch means native
composition ended. Do NOT invent such an API or use ime_enabled as proof. Thus
cross-frame active-preedit transfer safety is an unresolved C/architect prerequisite:
they must verify an existing input/native contract (without framework edits) that
can enforce refusal through composition completion, including reply commits while
loading. Until then ownership transfers/conditional destructive clears cannot be
claimed safe or enabled on a promise of lossless composition. No policy here
silently commits/cancels IME or considers committed-only snapshots sufficient.

Future C cases separately exercise nonempty preedit+Enter, commit+Enter, enabled
but idle, and preedit spanning updates then navigation/History/Restore/clear. Assert
actual native draft and Label plus FieldTransactions absent, entity identity and
scroll/caret outcomes. No native tests or physical certification in this pass.

Smallest READ-ONLY dependency investigation for SENDER TO AUTHORIZE separately:
verify exact pinned rndr `44f5dc7c0c91af56e4f3e29f97283b8e8d1e1139` source identity;
inspect Input preedit/commit/reset lifecycle and cross-frame clearing, Prompt/
TextField draft/composition storage/reset (historical `rndr_core/src/prompt.rs`
and `field.rs`; locate actual definitions), and native runner IME commit/cancel/
empty-preedit/focus-loss/disable delivery through Input -> Update -> observation.
Determine whether an EXISTING public state/event contract proves composition ended,
including quiet frames and reply commits. A “saw preedit” latch needs proven clear
semantics. If insufficient, return precise missing capability and retain BLOCKED.
No external read, framework edit/API, build/native run/test is granted by this
recommendation or performed here. Documentation does not fix the detector.

## 5. Per-action capture / apply fences (C R4)

At controller entry capture action intents and safe native Submitted BEFORE reply
application, with capture-time eligibility. Then apply replies before actions.
Retain intentional FIFO/multiplicity WITHIN action intents; append native Submitted
separately in their event order. Do NOT claim a globally chronological FIFO across
those streams. No dedup of intentional user actions, no post-reply retargeting.
An ineligible captured action stays ineligible even if a reply makes idle/connected
or changes selected identity, list/catalog/retry state. Each apply rechecks current
eligibility AND captured owner/context. Earlier actions may invalidate later ones;
never revive them against a new owner. A deliberate future Update can capture anew.

Common capture `K,G` = exact source selected key and generation; `E` = editor epoch;
`U` = current pending-UI-operation identity (including None). Capture required
eligibility booleans plus relevant immutable arguments, not draft shadows. Checked
operation IDs assigned only when an eligible action is actually applied. Validate
K/G/U for all session-mutating/SDK actions; E where text/ownership can change.
Menu/theme/copy need not all use E. **A — explicit eligibility DESIGN PROPOSAL:**
`idle` means EXACTLY `!busy && !loading`; `loading` iff pending_ui_operation is Some.
Every row requires capture predicates AND apply predicates; “recheck all” means
ALL stated capture predicates remain true, plus captured context/arguments. False
at capture is permanent for that event, even if replies complete loading.
New pending allocations require U=None at BOTH capture and apply: never overwrite
Some, even when equal to captured U. No outcome acknowledgement/abandon while loading.
Typing and harmless local menu/theme/copy/scroll may continue, never steal pending
ownership. New/Recent/remote Open/Retry require proven SOURCE composition preflight
at both stages, separate from destination commit on reply. Idle does NOT prove
composition ended. `source/Chat agreement` means Stable(id) == Chat.session or
New with Chat.session=None. Table completes, rather than replaces, gates:

| Action | Capture before replies (including eligibility NOW) | Apply revalidation / command |
|---|---|---|
| Send pointer / native Submitted | K,G,E,U=None, connected !busy&&!loading&&!uncertain, Stable exact Chat.session, valid model/catalog/depth, recovery capacity + composition safety; pointer committed text, native payload + SubmitBoundary.safe | Recheck ALL + K/G/E/U=None and Chat.begin guards, capacity/composition; allocate send op/recovery then Send; conditional clear only matching key/epoch/current text after Enqueued. Never relabel payload. |
| Restore | K,G,E,U=None, !busy&&!loading, Stable exact Chat.session, empty Label + proven composition safety, selected last RecoveryId/revision/pending association | Recheck ALL + K/G/E/U=None, entry still last/available, editor empty/safe; MOVE exact entry, clear matching pending pointer only; no command, uncertainty unchanged, offline/uncertain allowed. |
| New | K,G,E,U=None, connected !busy&&!loading&&!uncertain, source/Chat agreement, creation_outcome=None, source composition preflight, target policy/worst-case capacity | Recheck ALL + K/G/E/U=None; only then reserve/allocate Navigation/create token/dispatch New. No resolved-retry revival. Reply validates destination independently; already-created ID survives refused commit. |
| Recent | K,G,E,U=None, connected !busy&&!loading&&!uncertain, source/Chat agreement, source composition preflight, worst-case reservation available | Recheck ALL + K/G/E/U=None; allocate Navigation/reserve, Recent list(1)+history. Known-created result refuses commit with Retry instruction/releases own reservation; never implicit New. |
| sessions:list | K,G,U=None, connected !busy&&!loading&&!uncertain, source/Chat agreement, exact list revision/state | Recheck ALL + K/G/U=None; allocate List, one ListSessions(50), no G increment. No editor/transfer preflight; reply changes rows only. |
| remote row / retained remote Open | K,G,E,U=None, respective revision + exact membership/decoded ID, connected !busy&&!loading&&!uncertain, source/Chat agreement, no unresolved-created target; distinct target requires source composition preflight/capacity/guaranteed credit | Recheck ALL + K/G/E/U=None; selected exact ID no-op, no allocation/reset/command. Otherwise allocate Navigation/reserve/Open(id), separate reply commit. Explicit Reload reconciles; no stale-list revival. |
| retained local / draft:unsent | K,G,E,U=None, exact retained revision/membership or unsent key/allocation state, !busy&&!loading, source/Chat agreement, no unresolved-created target; distinct owner requires actual net capacity/composition safety | Recheck ALL + K/G/E/U=None; selected exact key is section2 NO-OP, no snapshot/reset/map/history/uncertainty/G change. Distinct owner commits atomic exchange, G+1; zero command. Offline/uncertain exception only as section2. |
| Reload | K,G,E,U=None, connected !busy&&!loading, selected Stable exact Chat.session; uncertainty allowed | Recheck ALL + K/G/E/U=None; allocate own Navigation/Open(selected ID), G+1. No editor/ownership reset/snapshot; preserve native composition/recovery. Reply changes projection only, no deferred ownership transfer. |
| Cancel | K,G,U=None, connected !loading, selected Stable exact Chat.session and busy-or-uncertain, exact execution/projection owner/turn if known | Recheck ALL + K/G/U=None; Cancel(id), no pending allocation. Busy deliberately allowed; loading is not. No retarget after terminal/History; ack never settles. |
| explicit SetMode | K,G,U=None, selected Stable(id) EXACT Chat.session=id, connected !busy&&!loading&&!uncertain, exact catalog revision/valid mode ID/current mode watermark | Recheck ALL + K/G/U=None + same catalog/watermark; only then allocate own Mode op/new mode owner, SetMode(id,mode). New/mismatched session or ANY List/Reload/Navigation/Mode loading refuses at capture/apply. Menu opening is not SetMode. |
| Retry known creation | K,G,E,U=None, exact KnownCreated token/new_key/created_id/original receipt authority, source/Chat agreement, connected !busy&&!loading&&!uncertain, source composition preflight/capacity/collision gates | Recheck ALL + K/G/E/U=None + exact lifecycle; allocate CreatedRetry Navigation/reserve/Open(recorded ID). Commit rechecks latest safety; refusal/failure retains KnownCreated. Consume origin once on success; no New/Send. |
| Abandon known / acknowledge Unknown | K,G,E,U=None, !busy&&!loading, exact outcome token/kind/known ID if any, FULL section3 receipt-retirement acknowledgement; offline/uncertain allowed | Recheck ALL + K/G/E/U=None + token/kind/ID; close only that outcome slot, no editor/composition/ownership transfer or pending allocation. Keep text; no command/deletion/noncreation assertion. Known refinement invalidates Unknown ack. Earlier ineligible New never revives. |
| model/depth choice | K,G,U=None, !busy&&!loading, source/Chat agreement, exact catalog revision/valid value; offline/uncertain allowed | Recheck ALL + K/G/U=None + catalog/value; no catalog-index retarget. Local next-turn config only, no pending allocation/Send/reset. |
| appearance/menu/copy/scroll | Exact local argument/target; allowed busy/loading/offline/uncertain; copy captures exact message/projection identity | Same target still exists/unchanged for copy; menu/theme/scroll only own local state. No pending allocation/navigation/recovery/SDK/Send/outcome resolution; cannot steal navigation association. No E needed. |

Refused/offline remote actions enqueue nothing and do not change generation,
selection, recovery or outcome metadata; static notice only. WorkerClosed after
attempted enqueue is distinct: navigation rolls back/releases reservation, Send
retains recovery/editor and marks execution unknown; never infer successful enqueue
from busy. Proposed DispatchOutcome = Enqueued / Offline / WorkerClosed. Local
Restore/menu/appearance/copy/retained retrieval/unsent never reconnect or defer Send.

## 6. Correlated replies, lifetimes and transitions (architect R4 / C)

### 6.1 Separate owners, not one maximum operation number

Conceptual Model ledger shapes to serialize (not implemented fields):

| Field | Shape / invariant |
|---|---|
| selected_key / inactive_drafts | SessionKey / BTreeMap<SessionKey,DraftSnapshot{text,revision}>; selected absent. |
| editor_epoch / next_identity / next_operation / next_recovery | Checked monotonic u64 counters; never wrap or reuse old tokens. |
| recovery_by_session | BTreeMap<StableId,Vec<RecoveryEntry{id,text,revision}>>, insertion ordered, max64 total; revision protects captured entry identity, not text matching. Empty vectors removed. |
| pending_by_session | BTreeMap<StableId,PendingAdmission{recovery_id,generation,send_op}>; at most one current unconfirmed pointer per session, no vector indices. |
| uncertain_sessions | BTreeSet<StableId>; active Chat mirrors selected membership as below, never inferred from text. |
| pending_ui_operation / projection_owner / mode_owner | Independent operation/phase/watermark lifetimes in the next table. |
| creation_outcome | One None/Creating/KnownCreated/Unknown slot; exact token/New key/original create op/dispatch generation; Known ID if received. Receipt authority survives lost Navigation/local owner change until explicit retirement. No archive; section3.2. |
| session_rows / retained_rows | Separate derived bounded projections and independently checked revisions (section2), not transcript stores. |
| existing chat/history/mode/catalog/selection/depth/commands/generation | Canonical active projection, next-turn configuration and transport; no inactive transcript cache or duplicated live editor string. |

Architect serializes conceptual Request{generation,operation,command} and
Reply{generation,operation?,session?,update}. Backend stamps exact command/create
ID, never infers identity from prose. This is app-local, not SDK wire evolution.
Existing Models/Modes/Unavailable exceptions remain. **B proposes ONE additional
exception:** exact unresolved creation-outcome receipt in section3.2, ONLY to refine
that slot, never navigation/follow authority. An attached receipt is demultiplexed
before the enclosing History/failure's navigation generation check. ALL remaining
reply effects require current generation then exact operation/session/phase checks.
None bootstrap is not a wildcard. Wrong session/op cannot mutate editor/loading/
projection/status owners. This is a reviewable app-local routing change, not an
assertion that existing code or the prior global rule already permits it.

| State / owner | Lifetime and allowed outcomes |
|---|---|
| pending_ui_operation | Navigation{op,generation,from,target,purpose,reservation}, List{op,generation}, or Mode{op,generation,session,mode_owner}; loading iff Some. Allocate only from None; no draft clone/overwrite. Own typed completion, commit-refusal cleanup or Unavailable releases it, never unrelated result. |
| projection/follow owner | ProjectionId{generation,operation,session}, exact expected active turn/stream descriptor or None, phase Following / AwaitingSettlement / Settled / FatalHold / Unresolved; at most one accepted-terminal identity and settlement decision. Committed idle History is Settled; accepted active History/Admitted is Following. Survives clearing loading/admission pointer, not retirement/replacement. Unresolved is a nonbusy transport hold requiring explicit Reload, never Send permission. FatalHold is the accepted fatal stop below, not AwaitingSettlement. |
| mode owner / watermark | Independent per active projection metadata authority. Follow mode read captures watermark at launch. Explicit SetMode advances owner before dispatch and supersedes earlier follow mode metadata, even if it arrives later. No global max-op rejection of valid stream/settlement. Matching Mode success or ModeFailure ends only its own pending Mode operation; older follow mode never clears it. Failure keeps last known mode marked honest, never rolls watermark backward. |
| pending admission | Per Stable ID {RecoveryId,generation,send_op}; only matching Admitted or explicit Restore removes its referenced entry. Reload keeps pointer and entry; later deliberate accepted Send can supersede pointer, NOT delete old recovery. |
| recovery / creation outcome / reservations | Memory ledgers separate from loading; recovery lifetime is not history lifetime. Outcome survives typed failure as section 3. Reservation ends on commit/failure/disconnect; release never deletes text. |

#### Required app-private controls and bounded worker context

- `ProjectionId { generation, operation, session }` is the exact committed
  navigation or accepted Send owner; checked operations never wrap/reuse.
- `ActivateProjection { owner, mode_watermark, active: None | { turn_id, stream_path } }`
  permits projection mode read and, only when active, existing SDK follow. Descriptor
  is from THAT accepted History/Admitted, never an older cached active snapshot.
- `RevokeProjection { owner }` retires only the exact local worker owner. It is
  distinct from remote `Cancel`; stopping observation proves neither cancellation
  nor non-admission.
- `ReconcileTerminal { owner, turn_id, terminal_seq }` permits exactly one existing
  bounded settlement operation, only after actual accepted nonfatal terminal.
- `StreamApplyResult = Rejected | AcceptedNonterminal | AcceptedTerminal { turn_id,
  seq, outcome }` MUST be returned by Chat's single validation/application path.
  A second dry run, optional helper or separate raw is_terminal test is insufficient.

Dispatcher services controls independently of network reads using owned cancellable
I/O tasks and tagged completions; never await controller acknowledgement or block
its command loop in history/mode/follow/settlement I/O. Read/create/list operations
end after emitting correlated result/failure, retaining no rejected History payload.
Bound command I/O to current authorized pending operation/admission; no unbounded
per-reply tasks, task archive or retained inactive payloads. Controls retain controller
enqueue order on the app-local route; closure terminates that worker context rather
than replaying queued permission into a replacement worker.
Keep at most one active projection task group: exact owner/active descriptor,
activation-consumed state and settlement-consumed state. An activation-only checked
operation high-water mark suppresses duplicate/retired activation replay, including
activation queued before retirement is serviced. Ordered activation then exact
revocation cannot later restart that owner; disconnect/shutdown closes its context.
This is NOT a global reply maximum:
newer List/explicit Mode operations do not invalidate valid stream/settlement.
Wrong-owner Revoke/Reconcile controls are inert and cannot replace/cancel current
work. Only a valid fresh higher-operation Activate may replace the worker owner;
retired/duplicate Activate is inert, not a new permission.

Controller retirement/replacement, generation-changing local navigation, disconnect
and shutdown revoke exact old owner; worker aborts owned mode/follow/settlement
work and drops late task results. A newer accepted activation replaces old work;
retired/duplicate activation never restarts it. Already-launched I/O may finish
before revoke is serviced, so controller also fences EVERY late result; no claim
of instantaneous remote cancellation. New/Open dispatch can retire the old generation
without erasing source text/history or granting destination authority. Failed
navigation never resurrects old follow; a deliberate fresh read is required.
List, explicit Mode, catalog updates and pending-admission removal do not revoke
projection. Actual worker/feed/control closure ends tasks with no activation/
reconciliation replay under the separate Unavailable/closure policy: preserve drafts/
recovery/creation outcomes, release only affected pending reservation/loading, mark
affected in-flight execution uncertain and keep honest failed context for explicit
Reload. Disconnection never grants Send or bypasses unrelated gates. A scoped follow
failure is not global closure or revocation of independent Mode (section6.2).
Local lifecycle controls do not allocate/steal pending UI ops.

### 6.2 Exhaustive reply routing / terminal effects

| Reply class | Acceptance and effect / what it cannot finish |
|---|---|
| CreationOutcome, standalone or attached to creation History/NavigationFailure | Section3.2 exact live token/create_op/dispatch generation/New key only; narrow stale-generation outcome exception. Retain known ID despite lost pending/current selection. Retired or newer token: no effect. Cannot navigate, release pending, mutate editor/projection or enable Send. |
| History for pending navigation (Open/New/Recent/CreatedRetry/Reload/startup) | Receipt FIRST, then current generation/exact pending op/source/target, collision/outcome/capacity/editor/native safety. Synchronous successful commit installs owner/expected active descriptor before clearing OWN Navigation/reservation; only THEN issue ActivateProjection with current mode watermark. Refused commit FAILS, never defers/activates. History emission itself grants no mode/follow authority or recovery retirement. |
| SettledHistory / SettlementFailure | Exact selected session/generation/ProjectionId AND accepted terminal turn_id/terminal_seq, AwaitingSettlement only, with matching dispatched reconciliation. Success consumes once -> Settled and updates canonical history; only valid idle reconciliation clears selected uncertainty. Exhaustion/error consumes result opportunity -> Unresolved, keeps uncertainty/live text, no automatic retry. NEVER Prompt reset or unrelated loading/recovery clear. Duplicates, wrong identity/phase and late results are inert. |
| Mode | Either exact pending explicit Mode owner success (finish that Mode only), or exact unrevoked projection metadata owner/session plus unchanged mode watermark; latter does not finish any pending UI operation. Late older metadata cannot overwrite later explicit SetMode. Any separately authorized completion in nonfatal Unresolved is metadata-only under the failure-scope rules below. |
| ModeFailure | New typed matching mode failure finishes its OWN Mode only; follow metadata failure affects scoped metadata/status only, not navigation/history rollback or busy settlement. Generic Notice is not mode completion. |
| SessionsListed / SessionsListFailed | Exact pending List op, generation, session=None; replace rows/revision on success, preserve last good rows on typed failure; finish that List only. |
| RecentEmpty / NavigationFailure{created_id?,creation_outcome?} | First demultiplex exact creation receipt under section3.2, EVEN without pending context; known ID survives SDK error/exhaustion/pending loss. Navigation cleanup separately requires exact current pending op/generation: release OWN reservation/loading, preserve source editor/history/mode/live. No-ID dispatched create -> Unknown only from matching Creating/Unknown, NEVER downgrades KnownCreated. Non-create Open failure never calls Chat.fail on idle source. |
| Admitted | Exact selected send session/generation/op AND pending admission ID; install expected admitted turn/stream descriptor and projection owner, retire only matching recovery entry/pointer once, THEN ActivateProjection. No editor reset/loading clear. Worker emits admission but cannot follow before this acceptance. Rejected/stale/duplicate/restored-pointer result neither follows nor retires other recovery; remote execution may nevertheless exist, never automatic resubmit. |
| Stream | Exact selected session/generation/ProjectionId and Following phase, THEN actual Chat.apply validation/application result below. Rejected has zero mutation/terminal effects/settlement. AcceptedNonterminal applies once only. AcceptedTerminal alone records identity, phase, uncertainty/status and settlement decision; no raw is_terminal authority. |
| send/follow Failure or scoped EOF | Exact pending send or active ProjectionId/session/generation and applicable phase. Following transport failure/EOF -> nonbusy Unresolved/uncertain, live text/recovery retained; NOT accepted terminal or settlement. Once terminal phase advances, late follow error/EOF cannot override it. No unrelated pending clear or inference replay. |
| Notice / cancel acknowledgement | Valid existing generation and applicable contextual owner for scoped status; informational bootstrap None cannot finish command. Not settlement, admission, uncertainty clear, mode completion or arbitrary loading clear. |
| Models / Modes | Existing stale-generation catalog exceptions only; replace catalog, not session/projection/uncertainty/recovery or pending loading. Captured catalog actions must still validate. |
| Unavailable | Existing global stale-generation exception: disconnect, busy/loading false, release pending navigation reservation, preserve editor/history/recovery/pending admission. KnownCreated/Unknown remain; dispatched Creating with no known ID becomes Unknown under its same token (never silently resolved). Mark affected selected in-flight execution uncertain, retain honest failed follow context for explicit reconciliation; no fabricated admission or successful settlement. |

All Update variants must be classified by this table; new typed variants include
SessionsListed, SessionsListFailed, RecentEmpty, NavigationFailure, ModeFailure and
the proposed app-local CreationOutcome receipt/envelope (section3.2 exception),
SettlementFailure, scoped EOF and owner-scoped activation/control failure outcomes.
The latter never masquerade as navigation failure after commit or accepted terminal.
All stream replies carry ProjectionId; settlement success/failure additionally
carries accepted turn/terminal sequence. Activated Mode replies carry owner and
launch watermark. Scoped activation/control failures require exact owner and phase;
post-commit activation failure is section6.3, never NavigationFailure/rollback.
No new stale-generation exception is granted for any control/stream/failure.

After controller session/generation/owner/Following checks, actual Chat.apply MUST
return Rejected without mutation for !busy, wrong turn or nonincreasing sequence.
Otherwise it applies ONCE and returns AcceptedNonterminal or AcceptedTerminal with
accepted identity/outcome. Only AcceptedTerminal permits terminal status/uncertainty/
phase effects. Atomically record that identity, mirror uncertainty and consume the
one settlement-dispatch opportunity. Nonfatal Following -> AwaitingSettlement emits
exactly one ReconcileTerminal; there is no automatic retry on dispatch failure.
Wrong-owner/turn/sequence/generation, duplicate or terminal-after-idle yields zero
phase/status/uncertainty effects and zero settlement requests.

Worker MUST NOT call settled_history or stop an otherwise valid follow merely on
raw/rejected terminality. Follow ends on accepted-terminal control, exact revocation
or genuine transport completion/failure. ReconcileTerminal is honored once for exact
active owner AND active turn, stops its follow and invokes existing bounded reader
(three attempts, existing delays/filtering/non-atomic notices retained). No new
admission/inference. Duplicate controls do nothing; matching result consumes once.
ReconcileTerminal dispatch failure consumes the one dispatch opportunity, enters
nonbusy uncertain Unresolved and invokes ZERO readers; no automatic retry. Settlement
exhaustion/error consumes its result opportunity and retains terminal identity/decision,
uncertainty/live text in Unresolved; follow already stopped, no second reader.
Actual closure follows section6.1; no unrelated loading/owner is cleared.

**Failure scope — selected architect recommendation, subject to architect + C rereview:**

| Failure | Required scoped outcome |
|---|---|
| Following EOF/failure | End that follow -> nonbusy uncertain Unresolved; later Stream/settlement rejected by phase even for matching owner. No automatic restart/read. This does not globally revoke independent Mode. |
| Reconcile dispatch failure | Consume opportunity -> nonbusy uncertain Unresolved, terminal identity/decision retained; ZERO readers, no retry. Follow has no further application authority; later Stream/settlement rejected by phase. |
| Settlement exhaustion/error | Follow already stopped by reconciliation; consumed terminal identity/decision and result opportunity retained -> nonbusy uncertain Unresolved. No second reader; later Stream/settlement rejected by phase. |
| Active-projection activation failure AFTER commit | Retain committed owner/text/history/identity/recovery -> nonbusy uncertain Unresolved, explicit Reload required; no rollback/resend/recreate. |
| Idle History metadata-only activation failure | Retain valid idle reconciliation, mark metadata unavailable. Failed mode metadata alone introduces neither execution uncertainty nor blanket Reload-required hold. Actual worker disconnection instead follows separate Unavailable/closure policy; no Send escape through disconnection or unrelated gates. |
| Accepted Fatal | Separate FatalHold below: exact worker revocation and ALL worker outputs fenced, not merely failed follow. |

Unresolved is not Following or AwaitingSettlement and matching identity cannot revive
it. Independently authorized metadata completion still permitted in nonfatal
Unresolved remains metadata-only: it cannot erase the unresolved notice's substance,
change hold/busy/uncertainty/terminal decision, revive follow, or finish unrelated
loading. Exact explicit Mode completion may finish only its own pending Mode;
projection metadata finishes none. Do not globally revoke independent Mode on every
scoped follow failure. FatalHold's all-worker-output fence remains separate.

Accepted Fatal instead consumes terminal decision into FatalHold (not awaiting a
reader), sets busy=false/uncertainty and preserves live text/history/recovery. Send
remains blocked; explicit Reload is required with inspect/copy warning. Issue exact
RevokeProjection to stop that worker group, NOT remote Cancel or ReconcileTerminal;
Controller retains FatalHold identity/terminal decision but retires worker authority,
so all later worker mode/follow/settlement outputs are fenced, not merely Stream.
If revoke cannot enqueue, FatalHold still fences them and worker closure stops tasks. Duplicates cannot upgrade this hold to settlement.
This is the architect's fatal-hold policy expressed as a phase/stop proposal for
rereview, not a new automatic fatal recovery decision.

Remove existing automatic post-settlement mode read. Only committed-projection
activation and explicit SetMode read/change mode here; any future refresh needs
separate reviewed correlation. Explicit SetMode advances watermark BEFORE dispatch;
older activated metadata cannot overwrite it or finish its pending Mode. Never use
a global maximum to reject independently valid follow/settlement.

### 6.3 Transactional navigation and uncertainty authority

Remote Open keeps source selected, live editor/history/mode/chat unchanged during
loading. At matching valid history, snapshot LATEST source (not request-time clone),
consume exact target snapshot, replace same editor, select key, install canonical
history/chat/mode projection, then release reservation/clear own loading. No event,
await/shell sync/observation sees half a transfer. New same-owner migration instead
keeps latest live text, removes origin identity once and increments epoch. Local
navigation follows the same ownership exchange with explicitly empty unreconciled
projection. Selected-key retained-local is instead the section2.3 NO-OP.
Failed/stale/collision/composition-refused commits leave all text and selection
intact; no optimistic mode=None or partial map take. Generation increments
once at remote navigation begin/accepted Send/local owner transfer; never rollback.
List/Restore/Cancel/theme/refusals do not increment generation; Restore advances E.

**C — exact failed-commit lifecycle DESIGN PROPOSAL:** for a matching Open/New/
CreatedRetry (also Recent/startup ownership transfer) History that cannot safely
replace/migrate due to active or unknown composition, FAIL, not defer. Release
only matching reservation and pending Navigation/loading; preserve source native
composition/editor/history/identity and all unconsumed target/origin snapshots.
Keep the already-incremented navigation generation (never roll it back), but do
not install destination projection/follow/mode owner or change its uncertainty.
No cached successful History, deferred response owner, timer completion or automatic
fresh read. User must deliberately Open again; known create uses Retry-Open ONLY.
For New, demultiplex Known ID FIRST and retain KnownCreated even though navigation
commit failed. Retry refusal likewise retains the original KnownCreated association.
Non-create failure, typed error, collision and disconnect release the same own
reservation without text loss; disconnect first resolves creation outcome per B.
Unknown composition at dispatch fails SOURCE preflight with no command/outcome;
composition may begin after dispatch, hence reply safety is independently required.
Reload has no ownership/editor replacement and preserves native composition; it
must not smuggle a reset through projection installation.

**Chosen non-speculative History -> activation edge:** navigation worker does ONLY
existing bounded read/create/list work, emits correlated History/typed failure and
ENDS that read operation. It neither reads destination mode nor starts destination
follow, and never waits for acceptance acknowledgement. Apply uniformly to New,
Open, Recent, CreatedRetry, Reload and connected startup. Sending History is NOT
acceptance; rejected History yields NO activation or waiting worker.

Controller first demultiplexes exact receipt, validates current generation/pending
operation/source/target/reservation/collision/native safety, then commits the complete
transaction synchronously: install ProjectionId and expected active turn, mirror
uncertainty, release ONLY own Navigation/loading/reservation. Only after this commit
enqueue ActivateProjection with exact accepted descriptor and CURRENT mode watermark.
No await/shell/observer-visible half-transfer. Idle History activates metadata only;
active descriptor authorizes existing SDK follow. Reload uses same activation edge
without editor reset or recovery/pending-admission retirement. Send likewise emits
Admitted with exact descriptor, but only accepted controller admission activates
follow; remote admission and local follow permission remain distinct.

Refusal has no activation, map take/reset or destination authority. Defensive
routing also rejects any uncommitted destination Mode/Stream/terminal/settlement;
only B's exact creation receipt may survive. Deliberate later Open/Retry gets fresh
operation/History/descriptor/watermark, never cached History or replayed New.

Activation enqueue failure AFTER commit retains COMMITTED owner/editor/text/history/
identity and recovery, not source rollback. For an ACTIVE projection, mark follow/
metadata unavailable and affected execution nonbusy but uncertain (Unresolved),
blocking Send pending explicit Reload; no recreate/resend/rollback. For IDLE History's
metadata-only activation failure, retain valid idle reconciliation and mark metadata
unavailable; do not invent execution uncertainty or blanket Reload-required hold
solely from failed mode metadata. Actual worker disconnection follows the separate
Unavailable/closure policy, never a Send escape through disconnection or unrelated
gates. These post-commit failures differ from pre-commit dispatch refusal/WorkerClosed
in section5. Exact retirement/revocation follows section6.1; failed navigation cannot
resurrect retired old-generation follow. Apply section6.2's scoped failure rules.

`uncertain_sessions` is authoritative per stable ID; active chat.uncertain MUST
mirror selected membership after EVERY relevant Chat replacement/resume/apply/fail.
History(no active) or valid AwaitingSettlement -> Settled clears selected membership;
History(active) inserts/preserves membership through Chat.resume (which currently
clears its local bool), busy=true and expected owner installed; follow START still
requires explicit activation. ACTIVE-projection post-commit activation failure becomes
nonbusy Unresolved without clearing membership; IDLE History metadata-only activation
failure retains valid idle reconciliation without inventing uncertainty. This intentionally
keeps reattached execution uncertain until durable settlement; busy also blocks
Send. Matching admission does not itself reconcile uncertainty. Accepted nonfatal
terminal sets membership until valid settled read; FatalHold and failed bounded settlement
retain it and live text, with no automatic read/retry. Local
retrieval sets it. List, Notice, Mode, Cancel ack, Restore, catalog or mere enqueue
never clears it. Do not clear a different session's uncertainty on selection.

Reload keeps current editor, scroll, recovery AND pending admission. Idle history
can clear uncertainty without retiring recovery; active history resumes without
new admission. Old-generation Admitted after Reload is rejected even if pointer
remains. A deliberate later Send after reconciliation may replace pending pointer;
the superseded entry remains individually recoverable. Selected-only Restore moves
last entry with exact ID, no command/admission even offline, does not unlock Send.
No transcript/recovery reconstruction from prose, list preview or duplicate text.
Startup connected --session must allocate actual pending Navigation/reservation/
source New context and stamped operation in the later architect wiring pass; do
not make startup an unrestricted History exception. Startup failure keeps New text.

## 7. Early exhaustive fail-closed observation gate (architect R5 / C)

Architect + D must land denial AND isolated tests atomically WITH OR BEFORE first
new private state is reachable under observe, not a later P4 task. Default Deny;
Public only for explicitly opted-in AUDITED EMPTY New/offline fixture. Existing
five conjuncts alone are not sufficient. Fail closed on absent/unknown state.

Required Public predicate is the conjunction of all audited emptiness conditions:
explicit FixtureObservation/offline fixture provenance; no worker/connection;
selected audited New (not Stable), Chat.session=None; empty committed Label AND
native draft/preedit/private input state under verified C contract; no history/live
or rendered private message projection/status; no inactive snapshots (including
identity-bearing empty ones), recovery entries/maps or pending admission; no
uncertain IDs; no KnownCreated/Unknown/Creating outcome or retry/created ID metadata;
no session-bearing pending UI/projection/follow/mode-owner context or reservation;
no identity-bearing ActivateProjection/RevokeProjection/ReconcileTerminal controls,
queued envelopes, exact active turn/stream descriptors, accepted-terminal tokens,
settlement decisions, worker task/completion context or retired-owner metadata;
no retained keys/rows/revisions with private metadata, no remote list IDs/rows/error
payload metadata; no other private identity/text in shell/menus/handles/events.
Catalog/theme data may remain only as explicitly audited synthetic fixture values,
never an implied exception for a private session. Scalar counters alone are not
content, but emptiness cannot be inferred from counters or a hidden widget.

Each nonempty/private conjunct independently forces Deny with all other state
empty, notably Stable selected with empty editor/history, inactive empty identity,
recovery-only owner, pending-only pointer, uncertain-only ID, known/unknown creation,
session-bearing navigation/projection/mode context, retained-only row and remote
list-only metadata. Gate must evaluate after mutations and before any observation
can consume content; no public intermediate frame. If persistent native preedit
emptiness cannot be established, remain Deny; no invented query/authority expansion.
No private fixtures/captures or production observation access authorized here.

## 8. Synthetic acceptance / registration plan — ALL UNEXECUTED

No cases written, registered or run now. Later A supplies session_tests.rs and
backend_tests.rs additions; C supplies composer_tests.rs; architect alone serializes
module registration and existing ui/interaction/qualification adaptations. D's
early isolated observation tests register with the gate. Use synthetic App/channels
and SDK test transport conventions only, no live daemon/provider/credentials.

| Future group | Required counterexamples and assertions |
|---|---|
| Identity/editor | A -> B -> A Unicode/multiline and identical prompts independent; selected absent from map, same entity, native draft/Label exact, marker absent, scroll reset on transfer only. Reload/theme keep editor/scroll. |
| Reachability | >50 remote sessions with inactive Stable draft and recovery-only owner outside list; independent retained projection, stale/tampered revision/member actions refused. Deleted/missing history fails without text loss, explicit offline local retrieval reaches exact draft then selected-only Restore, zero SDK/Send. Local uncertainty escape never enables Send. Unsent New -> Recent -> unsent restores exact bytes offline. |
| Capacity | 32 with nonempty exact target exchange succeeds; 32 with absent/empty/unknown Recent target refuses. Empty source at request then typed nonempty before History stays covered; typed failure/Unavailable releases reservation. Same-owner New/Reload no slot; CreatedRetry credits only consumed origin-New, not Stable collision. Local transfer uses actual text; recovery64 and counter overflow refuse before loss/command. |
| Creation | SDK create succeeds/history SDK error and bounded-history exhaustion retain ID. New(D)->S failure->ordinary Open S rejected with Retry->type E->Retry preserves ownership/no selected-map entry. Both live and inactive Stable collisions, inactive-origin retry after navigation, repeated retry, abandon with retained text, Unknown acknowledgement then only later separately captured New; no hidden replay/deletion claim. |
| Capture matrix | For EVERY section5 row: capture eligible/ineligible then reply switches owner, New->Stable unchanged text, changes pending op, idle/busy/uncertain, catalog/list/retained revision, recovery ID or creation token. No revival/retargeting. Intent multiplicity/order retained; native submissions separately appended, no global FIFO/Enter-time assertion. Reply-before-actions preserved. New/Open before Send blocks Send; Restore invalidates later captured Send. |
| Replies/lifetimes | Each section6 class: old generation, wrong op/session, duplicate/phase. Existing Models/Modes/Unavailable exceptions PLUS narrowly scoped B creation-outcome-only exception; all navigation remains fenced. Explicit Mode success AND typed failure finish own Mode; late follow Mode after newer SetMode cannot overwrite; stream/settlement still accepted under older independent projection owner. Clear loading/admission pointer then valid follow outputs still route. |
| Terminal/settlement | Wrong-turn terminal, duplicate/nonincreasing seq, terminal after idle do NOTHING; accepted nonfatal terminal marks AwaitingSettlement exactly once; accepted fatal enters FatalHold with no settlement. SettledHistory only matching selected awaiting owner consumes once, never Prompt reset or unrelated List/Mode loading clear. Notice/cancel/catalog never settles; bounded read failure retains uncertainty; execution-owned assistant filtering remains. |
| Recovery/uncertainty | Fail admission -> newer draft -> Reload idle keeps recovery/pending, no editor change; Reload active mirrors ledger through resume and blocks Send until settlement. Later deliberate Send supersedes pointer not recovery; old admitted generation rejected, matching admitted retires own ID only. Restore exact move offline/uncertain; nonempty draft refusal; no command or admission inference. |
| Startup/offline | Connected --session actual pending startup context accepts only matching History and fails to intact New. No worker remote actions zero enqueue/state movement; closed worker preserves drafts/recovery/outcome and releases reservation, no automatic reconnect/resubmit. |
| Privacy isolated | Every section7 state alone Deny, default/missing state Deny; explicitly opted-in empty audited fixture only Public. Gate and registered cases precede first new state under observe. No actual capture/private data needed. |
| C composition | Separate future nonempty preedit+Enter, commit+Enter, enabled-idle; active preedit across Updates then action/reply transfer refused safely. SubmitBoundary stays. Assert actual native draft/Label, marker lifetime/absence, scroll/caret, entity, no command on unsafe submission. Unverified API is blocker, not synthetic pass. |

Additional assertions from BOTH completed rereviews — all UNEXECUTED:

- A: SetMode captured during EACH List, Reload, Open/New/Recent/Retry Navigation,
  and Mode loading stays ineligible even if reply clears loading. New-selected and
  Stable/Chat.session mismatch emit nothing. For every section5 row exercise both
  capture and apply loading gates, no Some overwrite or reservation orphaning.
- B: create ID then SDK history error/exhaustion; Unavailable before/after standalone
  identity; attached Known after pending loss, local navigation and old generation
  still refines SAME unacknowledged slot only. Known duplicate/Unknown downgrade/
  conflicting ID handled without migration. Before vs after acknowledgement boundary,
  reply-before-captured-ack kind change, known abandon and newer token late results:
  disclosed retired receipts ignored, no newer outcome overwrite/hidden New replay.
- C: proven persistent preedit across Updates, then quiet batch with successful
  Open/New/Retry History: fail ownership commit, own loading/reservation released,
  source native/editor/history/identity preserved, no map take or target follow/
  Mode/Stream/terminal/settlement leakage. Known ID survives; only deliberate fresh
  Open/Retry can proceed. Repeat with collision, failure and disconnect, no cached
  History replay or replayed create. Unsafe dispatch emits no side-effectful command.
- D: selected same Stable(S) retained-local with live draft + recovery and valid
  projection: no reset/snapshot/map entry/history or uncertainty/G change and no
  extra transition-driven epoch increment (normal entry tick still applies);
  Restore remains separate and refuses nonempty editor.
- Preserve both reviews' ordinary committed-text/caret/scroll/entity/marker checks,
  conditional-clear mismatch/edit-away-back/failed enqueue, separate preedit+Enter,
  commit+Enter, enabled-idle and quiet-frame preedit Restore/clear cases. Unknown
  native state independently Deny with empty Label; no intermediate Public frame.
  No assertion can discharge the unproved composition API by documentation alone.

P2/P4 480pt/real-trigger popup evidence and P3 physical input remain deferred.
Keyboard/IME/clipboard/accessibility/compositor, huge messages, live daemon/provider
and full parity remain unverified. No test count or physical certification implied.

Additional architect/C acceptance-edge assertions — ALL UNEXECUTED:

- Every New/Open/Recent/CreatedRetry/Reload/startup read emits no destination mode/
  follow before accepted activation; delayed/stale/refused History starts none and
  leaves no acknowledgement waiter. Exact descriptor from accepted result only.
- Composition/collision refusal preserves source native composition/text/caret/
  scroll/identity/history and exact target/origin snapshots, releases only own
  pending/reservation, retains receipt-first KnownCreated; no cache/create replay.
- Successful ACTIVE commit then activation enqueue failure preserves committed text/
  identity/history/recovery -> nonbusy uncertain Unresolved/Reload-required; no
  rollback/resend/recreate. IDLE History metadata-only activation failure retains
  valid idle reconciliation, metadata unavailable, no invented execution uncertainty
  or blanket Reload hold. Actual disconnection uses Unavailable/closure, never Send
  escape through disconnection or unrelated gates.
- Duplicate activation/revoke, retired activation replay, wrong-owner controls,
  local navigation, disconnect at EACH edge, shutdown and late task outputs never
  revive owners. Activation-only high-water never invalidates stream/settlement on
  newer List/Mode; unrelated List/Mode and admission-pointer removal do not kill work.
- Accepted vs rejected Admitted gates follow independently of remote admission and
  retires exact recovery only; stale remote execution is not non-admission proof.
- Combined routing/application path rejects wrong owner/generation/phase; actual
  SINGLE Chat.apply rejects idle/wrong-turn/nonincreasing sequence (including
  duplicates): no effects and ZERO settlement calls. No dry run or second pass.
  Accepted nonfatal consumes ONE dispatch opportunity: successful dispatch allows
  ONE bounded reader, dispatch failure ZERO readers/no retry. Raw terminality gives
  none. Rejected candidate alone cannot stop follow; genuine Following EOF/failure
  ends follow -> nonbusy uncertain Unresolved; later Stream/settlement rejected by
  phase, no automatic restart/read.
- Fatal -> FatalHold/nonbusy/uncertain plus exact local revoke, zero reconciliation,
  no live-text erasure; late EOF/error/duplicate cannot change terminal decision.
- Settlement controls/results duplicate, wrong terminal identity, exhaustion,
  activation/control channel closure and late success after replacement never clear
  unrelated uncertainty/loading. Settlement exhaustion/error preserves consumed
  terminal identity/decision in nonbusy uncertain Unresolved, follow already stopped,
  no second reader. Independently permitted metadata in nonfatal Unresolved cannot
  erase unresolved notice substance, change hold/busy/uncertainty/terminal decision,
  revive follow or finish unrelated loading; scoped follow failure does not globally
  revoke Mode. No automatic reader retry or post-settlement mode read. Explicit
  SetMode watermark races remain fenced independently of projection.
- EACH new control envelope, stream descriptor, terminal token and worker/completion/
  retirement context independently forces Deny before observation, all else empty.
  Unknown private native state stays Deny; no intermediate Public frame.

## 9. Dependency / ownership sequence — no dispatch authorization

First sender must separately authorize the smallest pinned-source read-only
composition investigation in section4; if unresolved, hold affected runtime work.
This contract itself authorizes no such external access. After original principal
architect + independent C rereview of the exact corrected binding (A–D PLUS explicit
activation, revocation, Chat result, terminal/fatal/settlement lifecycle) AND sender
checkpoint; this document launches no reviewer/worker:

1. Architect serializes/finalizes Model/Ui/Handles/action/backend/acceptance-result
   shapes, defaults, operation/phase/mode-watermark contracts and test registration
   ownership with A/B/C/D. Refresh exact bases, no concurrent shared-shape edits.
2. Architect + D land early fail-closed gate + isolated registered cases with or
   before first private state enabled under observe; include defaults/empty fields
   introduced at step1 atomically if observable. Not deferred to P4.
3. C verifies existing composition contract/blocker and provides synchronous
   snapshot/reset/conditional-clear prerequisite, with actual native state tests.
   A cannot promise transition safety before this prerequisite is resolved.
4. A implements reviewed transitions/ledgers/capacity/typed backend routes and
   packet-local tests against serialized shapes, not competing editor ownership.
5. Architect wires startup --session, schedule/capture ordering, initialization,
   required single Chat.apply result, explicit post-commit/post-admission controls,
   independently serviced cancellable worker context and shared regression/module
   registration; no raw terminal-triggered settlement or post-settlement mode read.
6. Subsequent B/C UI composition exposes BOTH recent50 and retained local/remote
   paths, unsent/retry/acknowledgement, selected-only recovery and honest errors.
   UI projection move/restore stays synchronous and does not write ownership.

Architect may adjust sequencing to ensure early denial and compile-coherent shared
shapes but cannot waive review/blockers. C never dispatches SDK or owns session
Model; B emits frozen actions only; D fixtures are not transition writers. No new
generic store/framework/persistence/protocol, authority or second editor resource.
Sender retains orchestration, reviews, terminal callback and native integration.

## 10. Correction mapping, proposed decisions and unresolved checkpoints

| Review/request | Corrected normative location |
|---|---|
| architect R1 + C R1 reachability | 2.2/2.3 distinct recent/retained revisions, B remote and lossless local retrieval, honest missing/deleted error; 5/8 fences/cases. |
| both R2 capacity | 3.1 net equation, guaranteed release/worst-case reservation, exact exchanges32, same-owner/CreatedRetry/local accounting, failure release; 8. |
| both R3 creation | 3.2 live/inactive collision, exclusive Retry-only unresolved ID, one-time migration, Known/Unknown acknowledgement/abandon lifecycle; 5/8. |
| C R4 actions | 4/5 exhaustive capture/apply table, eligibility before replies, no retarget/revival, separate action/native ordering; 8. |
| architect R4 + C lifetimes | 6 mode owner vs projection owner, typed Mode completion, accepted terminal/settlement phase, uncertainty mirror/resume, pending preservation, actual startup context; 8. |
| architect R5 + C privacy | 7 exhaustive early deny predicate/isolated cases, 9 ordering; no observation authority expansion. |
| C mechanics/composition | 1/4 same entity and C/A ownership, opportunity-not-keystroke epoch, verified batch-only contract and exact persistent-preedit blocker, future distinct native assertions. |
| architect rereview A | 4/5 explicit idle=!busy&&!loading; U=None allocation, exhaustive capture+apply source/Chat/catalog/watermark/loading predicates; separate source preflight vs destination commit; no revival. |
| architect rereview B + C interruption | 3.2/6 exact single-slot receipt protocol, explicit outcome-only stale-generation proposal, known ID independent of pending, disclosed acknowledgement retirement/late-known tradeoff, no navigation authority. |
| architect rereview C + C composition lifecycle | 4/6.3 FAIL without transfer, own reservation/loading cleanup, source preservation, no uncommitted follow leakage/cache replay; KnownCreated Retry-Open only. Detector remains BLOCKED. |
| architect rereview D | 2.3/5 selected-key retained-local strict NO-OP; Restore separate/nonempty refusal. |
| architect turn8 + C acceptance-edge reconciliation | 1 exact input/full report recovery; 6.1 control/result/descriptor shapes and bounded lifecycle; 6.2 actual Chat result gates once-only nonfatal settlement/fatal hold; 6.3 non-speculative post-commit/post-admission activation and failure; 7/8 early Deny and unexecuted assertions; 9 exact rereview/checkpoint. |
| architect db962009 rereview + sender narrow corrections | 6.1–6.3 explicit follow/dispatch/settlement/active activation versus idle metadata failure scope; independent metadata limits in nonfatal Unresolved, separate FatalHold/closure policy; 8 combined routing versus SINGLE Chat.apply assertions. Selected recommendation remains subject to original architect + C exact-revision rereviews and sender checkpoint, not new API/runtime behavior or freeze. |
| both rereviews assertions/dependency boundary | 4 narrow pinned-source investigation recommendation only; 8 additional unexecuted assertions; 7 unknown native state stays Deny; 9 no dispatch authority. |

Proposed core preserved: transactional history commit for remote navigation; one
live editor, text-only inactive memory; selected-only Restore MOVE/no command/no
admission; no transcript cache; offline safety; honest SDK recent50; counts32/64;
loading permits typing with worst-case slot reservation; no text identity/eviction.
New local retained retrieval is explicitly unreconciled (not history success).

**Unresolved decisions / review blockers — no freeze implied:**

1. Principal architect + C must accept/revise local retained retrieval's empty,
   uncertain projection and narrow uncertain local-navigation exception; B must
   expose it. Without that path, missing/deleted retained bytes are NOT demonstrably
   reachable. Capacity refusal must not be mislabeled lossless automatic retrieval.
2. Approve counts50/32/64 and net-exchange refusal UX; they are count, not byte limits.
   No later byte limit may truncate/evict. Review reservation validation with the
   proposed failed-composition-commit cleanup; typed-during-loading source protected.
3. C/architect persistent-active-IME safety is a concrete unresolved prerequisite
   under verified app APIs (section4). No detector/framework API is invented and no
   runtime promise of safe ownership transfer can bypass it. Verify existing input
   contracts in later authorized pass or explicitly hold implementation blocked.
4. Architect must serialize required single Chat.apply result, ProjectionId,
   Activate/Revoke/Reconcile controls, exact descriptor/terminal tokens, independent
   mode watermark, bounded task lifecycle and typed outcomes in section6. Current
   backend behavior does NOT supply acceptance. Rereview FatalHold/exact local stop
   and Unresolved failure phases explicitly; no automatic fatal settlement choice.
   Startup --session needs actual pending context; no global max-op filtering.
5. Explicitly approve/revise A–D DESIGN PROPOSALS, especially B's outcome-only
   stale-generation exception and finite receipt-retirement disclosure (late known
   IDs intentionally ignored AFTER exact acknowledgement/abandon), C's fail-not-defer
   cleanup/follow suppression, and D's strict no-op. Known/Unknown UI copy, ordinary
   Open refusal and live/inactive collisions need original architect+C rereview.
   These are proposed contract decisions, not frozen interfaces or implementation.
6. Architect + D must audit ALL private state including native composition and
   session-only metadata; absence of a safe query stays Deny. Early isolated gate
   cases and registration are a prerequisite, not deferred visual qualification.
7. This exact corrected contract MUST return through ORIGINAL principal architect
   + independent C rereview (including selected acceptance mechanism), then
   sender checkpoint before implementation dispatch. Sender specifies refreshed
   exact base and serialized owners/test registration. Report acceptance != freeze.
   Existing docs-verifier discrepancy/physical certification remain outside this pass.

## 11. Handback validation boundary

This pass permits only bounded doc/source reads and read-only Git/hash/whitespace/
scope validation plus authorized docs commit. Exact head/parent/contract hash and
changed paths returned in handback. Every pre-existing object/mode outside approved
contract docs, all 55 sealed evidence files/manifest and execution-candidate source
must remain unchanged. No evidence scripts or verifier/test/build are run; no tests
written/registered/executed and no runtime implementation/freeze/stage-completion,
merge or integration claim. Sender retains review/orchestration/terminal callback.
