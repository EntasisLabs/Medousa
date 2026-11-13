# P1a / A — corrected proposed session, draft and recovery contract

**Status: DOCS-ONLY correction; NOT implemented, tested or frozen.** This revision
supersedes the initial P1a proposal, not the sealed qualification evidence. Both
independent read-only reviews were completed and sender accepted their reports;
that acceptance was NOT a contract freeze. Principal architect + independent C
must review this revised contract, then sender must checkpoint, before runtime
implementation. No workers/reviewers are dispatched here. Names below are proposed
app-private shapes, not existing SDK/protocol APIs or permission to expand scope.

## 1. Binding, scope and verified existing contracts

- Correction entry: clean `9960e0d89ec00acaf61a397485e29d48d3a54219`, parent
  `93f844648e68bd0e330a4b222f1ce89517f663be`; original CONTRACT was 434 lines,
  SHA-256 `79d8ae3da2be2ad85c1f7bff6f208d1c82f0bef5be7bcb8bffcc3231b35b7457`.
- Worktree: `/Users/theelevators/Library/Application Support/medousa/forge/worktrees/b906e0f57650/medousa-rndr-home-current-main-preview-requalification`.
- Execution candidate remains `bdef72eb7fbf8014067603f50bb15cfe0a3745f5`.
  External rndr pin remains `44f5dc7c0c91af56e4f3e29f97283b8e8d1e1139`; its
  checkout is not accessed. Production Home and all 55 sealed files/manifest stay
  unchanged. Original P1a base was `93f844648e68bd0e330a4b222f1ce89517f663be`.
- Only this contract (optionally adjacent correction matrix) may change. No source,
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

`creation_outcome` is one explicit lifecycle, with checked `creation_token`:
None -> Creating(origin/target New token, operation) -> either successful committed
migration/None, KnownCreated{token,new_key,created_id}, or Unknown{token,new_key}.
It is not silently cleared by navigation, list, Reload, Unavailable or a generic
error. Refuse another New while any unresolved outcome exists. Offline preflight
that dispatched nothing creates no remote outcome. After dispatch, no-ID failure
or lost result is honestly Unknown, not proof of noncreation. SDK create success
followed by SDK history error OR exhaustion of bounded history attempts must carry
the known created ID in typed NavigationFailure. Disconnect after receiving a
known ID retains it; any late result after operation abandonment is not migration.

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
preedit; refuse/defer the transition while preserving editor and pending target
until composition is safely finished, or fail it without transfer. A committed
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
Menu/theme/copy need not all use E. Table completes, rather than replaces, gates:

| Action | Capture before replies (including eligibility NOW) | Apply revalidation / command |
|---|---|---|
| Send pointer / native Submitted | K,G,E,U, connected, !busy/!loading/!uncertain, Stable, valid model selection/catalog identity and depth, recovery capacity; pointer committed text at capture, native payload + SubmitBoundary.safe | Same K/G/E/U, model/depth validity, all Chat.begin guards, capacity, composition safety; allocate send op/recovery then Send; conditional clear only matching key/epoch/current text after Enqueued. Never relabel native payload. |
| Restore | K,G,E,U, !busy/!loading, Stable, exact empty Label, selected last RecoveryId and entry revision/pending association | Same owner/epoch and exact chosen entry still last/available, editor still empty, composition safe; MOVE exact entry, clear only matching pending pointer; no Command, uncertainty unchanged, offline allowed. |
| New | K,G,E,U, connected idle not uncertain, creation_outcome=None, target policy and worst-case capacity | Recheck all, no newly resolved retry allowed; reserve/start New. History later validates pending source/target, not a retargeted selected key. |
| Recent | K,G,E,U, connected idle not uncertain, reservation availability | Recheck all, reserve worst case; Recent list(1)+history. Known-created target detected on result refuses with Retry instruction; no implicit New. |
| sessions:list | K,G,U, connected idle not uncertain, list revision/state | Same source/generation/pending identity and eligibility; one ListSessions(50), no generation increment. Reply changes rows only. |
| remote row / retained remote Open | K,G,E,U, respective projection revision + exact membership/decoded ID, idle connected not uncertain, no unresolved-created target, guaranteed target credit | Recheck all projection/target facts and capacity; Open(id). Already selected ID is no-op, explicit Reload reconciles. No stale-list revival. |
| retained local / draft:unsent | K,G,E,U, exact retained revision/membership or exact unsent key/allocation state, !busy/!loading, actual net capacity; no unresolved-created target | Same facts, composition safe; atomic local navigation, generation+1 on owner change; zero command. Allowed offline/uncertain only as section 2 exception. |
| Reload | K,G,E,U, connected !busy/!loading, selected Stable exact ID; uncertainty allowed | Same K/G/E/U and gates; Open(selected ID), generation+1; no editor reset/snapshot. Preserve pending recovery. |
| Cancel | K,G,U, connected !loading, selected Stable and busy-or-uncertain, exact execution/projection owner and turn if known | Same execution/session context and gates; Cancel(id). No retarget after terminal/History reply. Ack never settles. |
| explicit SetMode | K,G,U, connected idle/not uncertain, exact catalog revision/valid mode ID and current mode watermark | Same owner/catalog/gates/watermark; allocate Mode op and new mode owner; SetMode(id,mode). Menu opening alone is not SetMode. |
| Retry known creation | K,G,E,U, exact creation_token/new_key/created_id, idle connected/not uncertain, migration capacity/collision checks | Exact lifecycle still KnownCreated and all gates; Open(recorded ID) under CreatedRetry only. Consume origin once, no New/Send. |
| Abandon known / acknowledge Unknown | K,G,E,U, idle, exact outcome token + kind, explicit acknowledgement | Same token/kind/owner/epoch/gates; local metadata-only resolution, retain text, no remote assertion or command. Previously ineligible New stays refused. |
| model/depth choice | Captured catalog validity/selection argument and current busy eligibility; K/G/U if next-turn session configuration | Recheck chosen catalog/value and original session eligibility; no silent use of post-reply different catalog index. No Send or draft reset. |
| appearance/menu/copy/scroll | Existing local argument/target identity and relevant eligibility; copy uses exact current message projection, not arbitrary new message | Revalidate relevant target; harmless local behavior need not E fence. Never navigation/recovery/SDK/Send side effects. |

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
| creation_outcome | None / Creating / KnownCreated / Unknown with checked token and exact origin/target association (section3). |
| session_rows / retained_rows | Separate derived bounded projections and independently checked revisions (section2), not transcript stores. |
| existing chat/history/mode/catalog/selection/depth/commands/generation | Canonical active projection, next-turn configuration and transport; no inactive transcript cache or duplicated live editor string. |

Architect serializes conceptual Request{generation,operation,command} and
Reply{generation,operation?,session?,update}. Backend stamps exact command/create
ID, never infers identity from prose. This is app-local, not SDK wire evolution.
Models/Modes/Unavailable are the ONLY current stale-generation exceptions. All
others require current generation first, then exact operation/session/phase checks;
None bootstrap context is not a wildcard for command results. Wrong session/op
with matching generation cannot mutate ledgers, editor, loading or status owners.

| State / owner | Lifetime and allowed outcomes |
|---|---|
| pending_ui_operation | Navigation{op,generation,from,target,purpose,reservation}, List{op,generation}, or Mode{op,generation,session,mode_owner}; loading iff Some. No draft clone. Its own typed success/failure only finishes it. |
| projection/follow owner | {op,generation,session,phase: Following / AwaitingSettlement / Settled} for committed navigation/accepted Send; survives clearing loading OR pending admission. Do not orphan mode/follow/history replies when those pointers disappear. New accepted navigation/Send replaces it, not arbitrary notices. |
| mode owner / watermark | Independent per active projection metadata authority. Follow mode read captures watermark at launch. Explicit SetMode advances owner before dispatch and supersedes earlier follow mode metadata, even if it arrives later. No global max-op rejection of valid stream/settlement. Matching Mode success or ModeFailure ends only its own pending Mode operation; older follow mode never clears it. Failure keeps last known mode marked honest, never rolls watermark backward. |
| pending admission | Per Stable ID {RecoveryId,generation,send_op}; only matching Admitted or explicit Restore removes its referenced entry. Reload keeps pointer and entry; later deliberate accepted Send can supersede pointer, NOT delete old recovery. |
| recovery / creation outcome / reservations | Memory ledgers separate from loading; recovery lifetime is not history lifetime. Outcome survives typed failure as section 3. Reservation ends on commit/failure/disconnect; release never deletes text. |

### 6.2 Exhaustive reply routing / terminal effects

| Reply class | Acceptance and effect / what it cannot finish |
|---|---|
| History for pending navigation (Open/New/Recent/CreatedRetry/Reload/startup) | Exact pending op/generation/source still selected, expected target ID or typed discovery, collision/outcome/capacity/editor safety. Transaction below. Clear only matching Navigation; install projection owner before clearing. No recovery retirement. |
| SettledHistory | Exact selected session, projection/follow op+generation, phase AwaitingSettlement only. Consume once -> Settled, update canonical active history/chat projection, clear selected uncertainty after valid idle reconciliation; NEVER Prompt reset or unrelated loading clear. Duplicate, wrong-phase and old settlement ignored. |
| Mode | Either exact pending explicit Mode owner success (finish that Mode only), or exact projection/follow metadata op/session plus unchanged mode watermark; latter does not finish any pending UI operation. Late older metadata cannot overwrite later explicit SetMode. |
| ModeFailure | New typed matching mode failure finishes its OWN Mode only; follow metadata failure affects scoped metadata/status only, not navigation/history rollback or busy settlement. Generic Notice is not mode completion. |
| SessionsListed / SessionsListFailed | Exact pending List op, generation, session=None; replace rows/revision on success, preserve last good rows on typed failure; finish that List only. |
| RecentEmpty / NavigationFailure{created_id?,creation_outcome?} | Exact pending navigation context; release reservation, preserve selected editor/history/mode/live. Preserve known created ID including bounded-history failure; no-ID dispatched creation becomes Unknown. Non-create Open failure must not call Chat.fail on otherwise idle source. |
| Admitted | Exact selected send session/generation/op AND pending admission ID; update admitted turn, retire only matching entry/pointer once. No editor reset or loading clear. Wrong/stale/duplicate or restored pointer cannot remove another entry. |
| Stream | Exact projection/follow owner plus Chat turn and increasing sequence/busy validation BEFORE terminal side effects. Only ACCEPTED terminal transitions phase AwaitingSettlement, marks uncertainty and requests/accepts existing bounded reconciliation. `event.is_terminal()` alone is never authority. Wrong turn, duplicate/out-of-order or already-terminal event changes nothing. |
| send/follow Failure | Exact pending send or active follow owner, scoped to that session/phase; mark failure/uncertainty, retain recovery, no unrelated pending UI clear. Settlement read exhaustion stays uncertain; no inference replay. |
| Notice / cancel acknowledgement | Valid existing generation and applicable contextual owner for scoped status; informational bootstrap None cannot finish command. Not settlement, admission, uncertainty clear, mode completion or arbitrary loading clear. |
| Models / Modes | Existing stale-generation catalog exceptions only; replace catalog, not session/projection/uncertainty/recovery or pending loading. Captured catalog actions must still validate. |
| Unavailable | Existing global stale-generation exception: disconnect, busy/loading false, release pending navigation reservation, preserve editor/history/recovery/pending admission. KnownCreated/Unknown remain; dispatched Creating with no known ID becomes Unknown under its same token (never silently resolved). Mark affected selected in-flight execution uncertain, retain honest failed follow context for explicit reconciliation; no fabricated admission or successful settlement. |

All Update variants must be classified by this table; new typed variants include
SessionsListed, SessionsListFailed, RecentEmpty, NavigationFailure and ModeFailure.
Adapter may need an app-local accepted-event result/helper around Chat validation:
Chat.apply currently returns unit, so controller cannot safely infer acceptance
from event.is_terminal. Architect must serialize this acceptance contract without
weakening existing turn/sequence checks or claiming the current code already fixes it.

### 6.3 Transactional navigation and uncertainty authority

Remote Open keeps source selected, live editor/history/mode/chat unchanged during
loading. At matching valid history, snapshot LATEST source (not request-time clone),
consume exact target snapshot, replace same editor, select key, install canonical
history/chat/mode projection, then release reservation/clear own loading. No event,
await/shell sync/observation sees half a transfer. New same-owner migration instead
keeps latest live text, removes origin identity once and increments epoch. Local
navigation follows the same ownership exchange with explicitly empty unreconciled
projection. Failed/stale/collision/composition-refused commits leave all text and
selection intact; no optimistic mode=None or partial map take. Generation increments
once at remote navigation begin/accepted Send/local owner transfer; never rollback.
List/Restore/Cancel/theme/refusals do not increment generation; Restore advances E.

`uncertain_sessions` is authoritative per stable ID; active chat.uncertain MUST
mirror selected membership after EVERY relevant Chat replacement/resume/apply/fail.
History(no active) or valid AwaitingSettlement -> Settled clears selected membership;
History(active) inserts/preserves membership through Chat.resume (which currently
clears its local bool), busy=true and follow owner installed. This intentionally
keeps reattached execution uncertain until durable settlement; busy also blocks
Send. Matching admission does not itself reconcile uncertainty. Accepted terminal
sets membership until settled read; failed bounded settlement retains it. Local
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
| Replies/lifetimes | Each section6 class: old generation, wrong op/session, duplicate/phase. Only Models/Modes/Unavailable exceptions. Explicit Mode success AND typed failure finish own Mode; late follow Mode after newer SetMode cannot overwrite; stream/settlement still accepted under older independent projection owner. Clear loading/admission pointer then valid follow outputs still route. |
| Terminal/settlement | Wrong-turn terminal, duplicate/nonincreasing seq, terminal after idle do NOTHING; accepted terminal marks AwaitingSettlement exactly once. SettledHistory only matching selected awaiting owner consumes once, never Prompt reset or unrelated List/Mode loading clear. Notice/cancel/catalog never settles; bounded read failure retains uncertainty; execution-owned assistant filtering remains. |
| Recovery/uncertainty | Fail admission -> newer draft -> Reload idle keeps recovery/pending, no editor change; Reload active mirrors ledger through resume and blocks Send until settlement. Later deliberate Send supersedes pointer not recovery; old admitted generation rejected, matching admitted retires own ID only. Restore exact move offline/uncertain; nonempty draft refusal; no command or admission inference. |
| Startup/offline | Connected --session actual pending startup context accepts only matching History and fails to intact New. No worker remote actions zero enqueue/state movement; closed worker preserves drafts/recovery/outcome and releases reservation, no automatic reconnect/resubmit. |
| Privacy isolated | Every section7 state alone Deny, default/missing state Deny; explicitly opted-in empty audited fixture only Public. Gate and registered cases precede first new state under observe. No actual capture/private data needed. |
| C composition | Separate future nonempty preedit+Enter, commit+Enter, enabled-idle; active preedit across Updates then action/reply transfer refused safely. SubmitBoundary stays. Assert actual native draft/Label, marker lifetime/absence, scroll/caret, entity, no command on unsafe submission. Unverified API is blocker, not synthetic pass. |

P2/P4 480pt/real-trigger popup evidence and P3 physical input remain deferred.
Keyboard/IME/clipboard/accessibility/compositor, huge messages, live daemon/provider
and full parity remain unverified. No test count or physical certification implied.

## 9. Dependency / ownership sequence — no dispatch authorization

After revised principal architect + independent C review AND sender checkpoint:

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
   active accepted-stream helper and shared regression/module registration.
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
   No later byte limit may truncate/evict. Review reservation validation/composition
   deferral together so typed-during-loading source is always protected.
3. C/architect persistent-active-IME safety is a concrete unresolved prerequisite
   under verified app APIs (section4). No detector/framework API is invented and no
   runtime promise of safe ownership transfer can bypass it. Verify existing input
   contracts in later authorized pass or explicitly hold implementation blocked.
4. Architect must serialize accepted-stream validation result/helper, pending UI vs
   follow phases, independent mode watermark and typed ModeFailure/creation outcomes;
   startup --session needs actual pending context. No global max-op filtering.
5. Approve Known/Unknown lifecycle, explicit acknowledgement/abandon copy with B,
   ordinary-open refusal for unresolved created ID and live/inactive collision tests.
6. Architect + D must audit ALL private state including native composition and
   session-only metadata; absence of a safe query stays Deny. Early isolated gate
   cases and registration are a prerequisite, not deferred visual qualification.
7. Revised contract MUST return through principal architect + independent C and
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
