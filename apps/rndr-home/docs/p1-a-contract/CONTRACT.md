# P1a / A — proposed session, draft and recovery contract

**Status: design for review; NOT implemented and NOT tested.** Author is the new
senior engineer A, not the principal architect. Architect + C review, followed by
sender's explicit checkpoint, is required before P1b runtime implementation.
No reviewers/workers are launched by this handback. All names below are proposed
app-private shapes, not existing SDK/protocol additions or permission to edit them.

## 1. Binding and scope

- Exact clean entry/base: `93f844648e68bd0e330a4b222f1ce89517f663be`, parent
  `2cef83fdc3b977bc83b2800a8601f0167bb95a86`.
- Previously executed/reviewed candidate remains
  `bdef72eb7fbf8014067603f50bb15cfe0a3745f5`; this contract does not execute it.
- External rndr pin remains `44f5dc7c0c91af56e4f3e29f97283b8e8d1e1139`;
  its checkout is neither needed nor accessed. Production Home is untouched.
- Only this new docs namespace may change. No runtime, module/test registration,
  manifests, lockfile, shared docs, evidence seals, SDK/daemon/framework or caches.
- No Cargo/build/test/app/export/native/network/credential/provider execution.
  No build slot used. Future single shared target, maximum two jobs, debug and
  incremental off, and 8 GiB floor remain unchanged. No push/deploy/integration.

### Existing symbols that constrain this proposal

All app source paths below are relative to `apps/rndr-home/src/` at the base.

| Source symbol | Current contract / required adaptation, not an implemented change |
|---|---|
| `state.rs::Model` | One `chat`, `loading`, `generation`; global `recovery: Vec<(String,String)>` and index `pending_recovery`; no list or draft map. Replace global index association, do not layer another live editor over it. |
| `controller.rs::{update,dispatch}` | Replies precede actions; navigation/send generation fencing; `Models`, `Modes`, `Unavailable` are the only stale-generation exceptions. Enqueue is not admission. Selection needs transactional handling, operation correlation and contextual input capture below. |
| `lib.rs::Chat::{history,begin,admitted,fail,resume,apply}` | `session` is stable authority; `begin` rejects absent session/busy/uncertain/blank. `apply` checks turn identity and increasing stream sequence. Retain these; uncertainty is not text reconciliation. |
| `backend.rs::{Command,Request,Update,Reply,serve}` | Existing commands New, Recent, Open(String), Send(shared request), Cancel, SetMode. Recent reads `sessions().list(1)`, Open invokes `history(id)`. Add only an app-local bounded list projection/context. |
| `backend.rs::{history,settled_history,follow,mode}` | Three bounded bracketed history attempts, execution-owned assistant filtering, non-atomic notices, bounded settlement reads and SDK stream reconnection remain. No admission retry, text deduplication or stronger snapshot claim. |
| `composer.rs::{reset_prompt,guard_submit_boundary,setup,Handles}` | Same Prompt/Label entity is canonical. Reset synchronously inserts/removes `FieldTransactions`, resets scroll; six visual lines; ambiguous Enter/IME batch guard. C owns adapter implementation. |
| `shell.rs::{Ui,setup,sync}` | B owns navigation/projection. Synchronous history/live take/restore is not transition ownership. Current recovery projection is global and must become selected-session-only. |
| `main.rs::app`, `fixtures.rs::sync_observation` | Architect owns schedule/initialization/test registration. Existing offline opt-in/empty-state observation authority must not widen for new inactive drafts/recovery/list metadata. |
| `ui_tests::{failed_admission_reload_preserves_visible_recovery_and_newer_draft,confirmed_admission_retires_recovery_without_touching_newer_draft,terminal_waits_for_history_and_cancel_does_not_fake_settlement}` | Existing acceptance intent, read not run here. Later tests must cover the new session scope and operation routing. |

Read-only shared contracts: `crates/medousa-sdk/src/sessions.rs::SessionsApi::list`
accepts **only `limit: usize`**, returns `SessionHistoryListResponse`; `history`
and `active_turn` are already used by the adapter. `crates/medousa-types/src/
daemon_api.rs::SessionHistoryListResponse` contains `sessions` and `next_cursor`;
`session.rs::SessionHistorySummary` contains stable `session_id`, optional
`display_name`, `last_timestamp`, `turns`, `preview` and other metadata. A returned
cursor is NOT an admitted cursor-taking SDK list method. No raw HTTP workaround.

Preparation sources read: `PREPARATION_CLOSURE.md` final P1/A packet,
`TEAM_PREPARATION.md`, `INDEPENDENT_REVIEW_0_6_0.md`, and both original
`review-prep-{correctness-20261113-1304,ux-20261113-0bdc05e3}/REPORT.md` reports.
Their claims/bytes remain historical, not new execution evidence. Root docs-gate
Coder exit 0 is scoped; sender runtime exit 1 remains unresolved. No app-local
gate exists; neither rerunning nor diagnosing that discrepancy belongs to P1a.

## 2. Identity and single-draft invariants

1. `SessionKey = New(u64) | Stable(String)`. Stable strings are exact SDK session
   IDs within this process's single fixed daemon connection. Never normalize,
   case-fold, title-match, timestamp-match or compare prompt prose for identity.
   Future connection switching requires a separate contract; no cross-daemon keys.
2. Initial editor key is `New(0)`. New keys are monotonic process-local tokens,
   never serialized. There is one selected key, equal to `Stable(chat.session)`
   when selected chat has a session; selected New has no `Chat.session` and Send
   is disabled. New is an unsent editor identity, not a provisional daemon ID.
3. The selected key has **no text entry** in `inactive_drafts`. Its sole mutable
   text is the mounted Prompt/Label. Inactive snapshots own exact UTF-8 text and
   a revision token only; no hidden Prompt, mirror live string, second editor,
   persisted store or automatic draft synchronization. Empty inactive snapshots
   may be omitted; a fresh target defaults to empty. Snapshots never expire while
   nonempty. Identical Unicode/multiline prompts in A and B are independent.
4. New action while selected New requests existing `Command::New`, using that
   New token. On matching successful creation **and history load**, move the New
   identity to the returned stable ID; leave its currently live draft intact.
   Creation identity is the SDK `CreateSessionResponse.session_id`, not a turn
   `Admitted` event. A late/stale create result cannot migrate anything.
5. New action while selected Stable allocates a target New token with an empty
   draft, but does not select it optimistically. Commit it as Stable only when
   creation/history succeeds; source Stable draft is snapshotted then. There is
   never a merge with the source or an existing target Stable snapshot.
6. If New creates a session but history fails, report that known created ID in a
   typed navigation failure. Keep source selected/live draft untouched and retain
   the failed navigation's target-token/created-ID association in memory. An
   explicit Retry Open uses **Open(created stable ID)**, never another implicit
   New. If the source was that New token, migration occurs only on this successful
   retry. A user may abandon/retry later; no silent recreation or automatic retry.
   Failure before an ID is returned is honestly unknown; do not manufacture one.
   If the user has since switched away from the origin New key, successful Retry
   Open consumes that key's inactive snapshot as the target stable draft, while
   snapshotting the currently selected source separately. A target New reserved
   from Stable has no snapshot and defaults empty. If that New key is live at retry
   commit, keep its latest live text instead. Remove the migrated New snapshot/key
   exactly once; never copy current unrelated source text to the created session.
7. A creation/retry ID colliding with a separately owned inactive Stable draft is
   a contract conflict: refuse commit, retain both owners and show an error. No
   text equality shortcut, concatenation, replacement, or implicit merge.

An unsent snapshot must remain reachable: `draft:unsent` is an explicit local
navigation action, not an SDK request. It selects the sole retained New key if
one exists (initially New(0)), otherwise allocates a fresh New token. At most one
ordinary unsent editor/snapshot exists; transient create targets have no draft
until successful migration. New action from Stable still creates a fresh empty
target and does not consume an unrelated retained unsent snapshot. On local
unsent selection, snapshot the source latest text, restore the unsent text, replace
active transcript with an empty New projection, and keep stable snapshots/recovery.
Returning to Stable requires explicit successful Open, never a transcript cache.
This local navigation uses the same idle/not-uncertain/capacity gates, generation
+1 and atomic editor transfer, but is available disconnected and enqueues nothing.
It is a no-op when already selected New. B must expose this action whenever an
unsent snapshot exists; otherwise the initial draft could become unreachable.

List absence does not invalidate stable keys, drafts or recovery. Session deletion
is not introduced; failed Open of a missing ID leaves its draft available. Restart
loses all memory-only drafts/recovery (explicit limit); **Reload means history
reload in this running app**, not process persistence.

## 3. Proposed Model and app-private context shapes

Exact conceptual fields below are to be frozen with architect/C, not Rust added
by P1a. Existing unrelated model catalog selection/depth fields stay as-is.

| Field/type | Shape and invariant |
|---|---|
| `selected_key: SessionKey` | Committed editor owner, never the speculative target. |
| `inactive_drafts: BTreeMap<SessionKey, DraftSnapshot>` | `DraftSnapshot { text: String, revision: u64 }`; selected key absent; text-only, no focus/caret/scroll snapshots. |
| `editor_revision: u64`, `next_identity: u64`, `next_operation: u64`, `next_recovery: u64` | Checked monotonic counters, not wrapping; overflow refuses the operation without text loss. Revision is conservative invalidation, not exact keystroke count. |
| `pending_operation: Option<PendingOperation>` | `Navigation { operation, generation, from, target: Existing(id) / Create(new_key) / CreatedRetry(new_key,id) / Recent, purpose: Switch / Reload }`, `List { operation,generation }`, or `Mode { operation,generation,session }`. Stores no source draft clone. `loading == pending_operation.is_some()`; cancel/send streaming are not navigation loading. |
| `projection_operation: Option<OperationId>` | Last committed navigation or accepted Send owning active History/Mode/follow outputs; survives clearing loading/pending admission. Explicit mode requests route through their own pending operation instead. |
| `creation_retry: Option<CreationRetry>` | Last failed creation/history's `{new_key, created_id}` for explicit retry; no credentials/transcript copy. Refuse a second unresolved create until retry or explicit abandon; abandon of a selected New keeps its live draft. |
| `session_rows: Vec<SessionRow>` | At most 50 unique exact IDs; `SessionRow { id, display_name, last_timestamp, turns }`. No preview/transcript persistence. |
| `list_revision: u64`, `list_state: Never / Ready{more_unavailable:bool} / Failed`, `list_error: Option<StaticError>` | Last good rows survive failure; error and truncation separate from transcript status and uncertainty. |
| `recovery_by_session: BTreeMap<String, Vec<RecoveryEntry>>` | Stable IDs only; `RecoveryEntry { id: RecoveryId, text: String }`, insertion ordered, memory only. No recovery is created before stable session identity exists. |
| `pending_by_session: BTreeMap<String, PendingAdmission>` | `{ recovery_id, generation, operation }`, at most one unconfirmed send per session. Never a vector index. May outlive history reconciliation until explicit Restore or matching Admitted. |
| `uncertain_sessions: BTreeSet<String>` | Sticky session-scoped execution uncertainty; never cleared by list, selection elsewhere, Restore, Notice or enqueue success. Active `chat.uncertain` mirrors this authority at transitions. |
| Existing `chat`, `history_messages`, `mode`, `modes`, `models`, `selection`, `depth`, `commands`, `generation` | Remain canonical active projection/catalog/transport. No inactive transcript cache in P1. Failed navigation never replaces them except scoped error status. |

Bound process growth without evicting user text: proposed maximum 32 tracked
inactive nonempty draft keys and 64 recovery entries total. Refuse a navigation
commit/send **before any destructive mutation or command** if retaining its source
would exceed those counts; tell user to clear an unwanted draft or explicitly
restore recovery, never auto-delete. For navigation reserve required source slot
before enqueue; live edits during loading must still fit a reserved slot. Empty
keys/empty recovery vectors may be removed. Existing individual-message size
limitations remain: this is a count bound, not a byte-memory guarantee. Counters
and exact constants are explicit review questions in section 10.

### Request/Reply correlation (local adapter only)

Propose `Request { generation, operation: OperationId, command }` and
`Reply { generation, operation: Option<OperationId>, session: Option<String>,
update }`. Bootstrap catalog/unavailable may have `operation=None`; command
results/follow streams must carry the originating operation. Adapter stamps the
exact session from command/create/history, never infers it from prose. This is
**not** a shared SDK wire envelope. Send uses its own operation carried through
Admitted/stream/settlement; navigation uses its operation through History, mode
and reattachment. PendingAdmission is the send association, not loading state.
A successful navigation records `projection_operation` before clearing pending
navigation; Send replaces that context. Mode/follow/settlement replies still have
an owner after loading or the admission pointer has gone away. List results route
only to the exact pending List op, with session=None; navigation failures with no
known destination likewise route by pending op, not a fabricated session.
Informational bootstrap Notice with no operation may update status at its existing
valid generation, but cannot clear any pending operation. Cancel acknowledgement
is informational and never changes projection ownership or busy/uncertain state.

Propose `dispatch(...) -> DispatchOutcome { Enqueued, Offline, WorkerClosed }`
in place of today's unit result. Caller checks connection and capacity first;
only Enqueued permits synchronous conditional draft clearing. WorkerClosed on a
read/navigation rolls back loading/selection, preserves editor/projection and
reports disconnect; WorkerClosed on Send retains recovery/editor and marks
execution uncertain. No caller infers enqueue success from `chat.busy` alone.

## 4. Draft snapshot/reset API requested from C

Proposed synchronous app-private API in `composer.rs` (names subject to freeze):

- `snapshot_prompt(world, entity, revision) -> DraftSnapshot`: exact Label text,
  no mutation and no `FieldTransactions`. Called only at a committed ownership
  transfer, never every idle frame. A owns when; C owns mechanics.
- `replace_prompt(world, entity, text)`: wraps existing `reset_prompt`; retains
  entity, synchronous marker lifetime, resets Prompt scroll to zero, resets native
  draft/caret through the existing reset function. No remount, retained marker,
  async callback or new framework API. A advances revision before calling.
- `clear_prompt_if_current(world, entity, expected_key, expected_revision,
  submitted_text) -> bool`: controller validates key/revision, then C verifies
  current Label equals submitted snapshot solely as a destructive-clear guard.
  Equality is **not identity, admission or recovery reconciliation**. Mismatch
  leaves text untouched. Clear only synchronously after successful local enqueue,
  never later on Admitted/History. Failed enqueue retains the editor and recovery.

Revision strategy deliberately needs no new framework change detector or live
text shadow: at the start of each controller Update after native Input, checked
increment `editor_revision` even if text did not change. Also increment before
every programmatic replacement or editor-owner change, including New-to-Stable
migration. Thus a revision is an **editor opportunity epoch**, not exact text edit
count; an edit-away-and-back in a later frame cannot reuse an old clear token.
Inactive snapshot revisions are provenance only; restoring text allocates a fresh
active epoch, never reinstates an old counter. Theme/menu/copy do not replace the
Prompt; ordinary update epochs may advance but identity and text remain unchanged.

Before applying replies, capture all identity-sensitive intents and safe Submitted
snapshots with the current `{selected_key, editor_revision}`. Then keep existing
reply-before-action application order. Any reply commit/reset invalidates captured
Send/Restore/editor-sensitive actions; reject them, do not relabel them for the
new session. Within the FIFO, New/Open/Reload starts loading before a subsequent
Send can run; Restore increments revision before any subsequent captured Send.
Explicit pointer Send snapshots current text at this capture boundary; native
Submitted retains its payload, accepted only under `SubmitBoundary.safe` and
matching context. Never claim framework Submitted is Enter-time text. No Send on
navigation/restore/menu/theme/copy/scroll. C's active IME tests remain required.

## 5. Exact proposed action → Command → Update table

`connected` means a present worker sender; controller checks it before command
creation/state mutation, not only shell button enablement. A closed channel is a
separate enqueue failure. All navigation/list/mode actions require `!busy &&
!loading`; uncertainty additionally refuses New/Recent/Open/list/mode, leaving
explicit Reload of selected stable ID available. Local Restore is allowed while
uncertain but never clears it. Cancel retains existing busy-or-uncertain guard.

| Action identity | Controller admission and command | Result / effect |
|---|---|---|
| `new` | connected, idle, not uncertain, creation-retry resolved, capacity reserved; `New` | Matching `History(id,messages,active)` commits create/migration. Typed `NavigationFailure{created_id?}` rolls back selection. No Send. |
| `recent` | same navigation gates; `Recent` retains SDK `list(1)` then Open-by-ID internally | Matching History commits exact returned ID; new typed `RecentEmpty` finishes operation without selection/draft change. Failure preserves source. It is explicit open-most-recent, not list refresh or implicit New. |
| `sessions:list` | connected, idle, not uncertain; `ListSessions` fixed limit 50 | `SessionsListed(rows,more_unavailable)` replaces only list metadata; `SessionsListFailed` preserves old rows/selection/draft. No generation increment. |
| `sessions:open:<list_revision>:<hex_utf8_id>` | revision must equal last successful list revision; strict hex decode and exact ID membership, no ordinal/text matching; connected + navigation gates; `Open(id)` | History must equal requested ID before commit; mismatch is navigation failure. Current selected ID is no-op, use Reload for reconciliation. Missing/stale/tampered ID refuses without command. |
| `reload` | connected, idle; selected Stable required, allowed when uncertain; `Open(selected_id)` | Matching History replaces history/live projection only; never resets editor, migrates key or removes recovery. Failure retains projection/draft and uncertainty. |
| `sessions:retry-created` | explicit user retry of retained created ID; same idle/connect gates as navigation (selected New allowed); `Open(created_id)` | Matching History commits recorded New-to-Stable target transaction. No New/Send replay. |
| `draft:unsent` | idle, not uncertain, draft capacity reserved; local transfer to retained New key or fresh token; connected worker not required | No Command; generation +1 only if changing owner; retain source snapshot and restore exact unsent text in same editor. |
| `sessions:abandon-created` | idle, explicit local action | Clears retry metadata only; no command, no text deletion, no assertion about remote creation. |
| `send` / safe native Submitted | connected, !loading, selected Stable, valid model index, captured key/epoch current, `Chat::begin` guards !busy/!uncertain/nonblank; recovery capacity; `Send(Box<InteractiveTurnRequest>)` | Allocate recovery ID/pending operation before enqueue, generation +1. Matching session/generation/operation Admitted retires only its own pending entry. Failure retains it and blocks new Send until reconciliation. |
| `recover` (label Restore) | !busy, !loading, **exactly empty** live editor, current key/epoch; selected Stable with recovery | Move last entry of selected session to Prompt; remove exactly its ID and only matching pending pointer. No Command, including offline. Uncertainty unchanged. |
| `cancel` | connected, !loading, selected stable, busy or uncertain; `Cancel(id)` | Existing Notice is request acknowledgement, not settlement/admission. Does not clear recovery/uncertainty or fake idle. |
| existing mode action | existing catalog validity, connected, !busy/!loading, selected stable, not uncertain; `SetMode(id,mode)` | Server-effective Mode only; operation-correlated Notice finishes only its own mode request. Never clears a navigation/list loading state. |
| model/depth/menu/appearance/copy/scroll | Existing local behavior and gates; no new SDK action | No navigation, Send, recovery mutation or draft replacement. Unsupported controls remain disabled/honest. |

Proposed new Update variants are `SessionsListed`, `SessionsListFailed`,
`RecentEmpty`, `NavigationFailure{created_id:Option<String>}`. Existing History,
SettledHistory, Mode, Models, Modes, Admitted, Stream, Notice, Failure, Unavailable
remain with the context envelope. History is routed by pending navigation context
or active stream reconciliation, not assumed to mean any current selection.
Backend maps read failures to typed list/navigation outcomes instead of treating
all reads as uncertain turn admission. Other Failure remains send/follow failure.
No new action accepts title/preview/index as session authority. Hex encoding is
only lossless action transport; display abbreviations never select sessions.

### Bounded list, errors and pagination

One explicit list action performs exactly one `client.sessions().list(50)` read.
No startup loop, background refresh, next-page fetch, search, hidden history load,
reconnect or provider activity. Preserve server order; collapse duplicate exact
IDs to first occurrence, discard empty IDs, cap at 50 even if a server overreturns.
Project only ID, optional display name (truncate display to 120 Unicode scalars,
never ID), timestamp and turn count. Drop previews/other verification metadata;
do not log raw response bodies or IDs/text in diagnostics. Stable ID is retained
losslessly for Open, not parsed as a local path. Non-atomic list is not a lease.

Set `more_unavailable` if response has `next_cursor` or overreturns. Show “Recent
50 only; older-page navigation unavailable in this slice.” **No Load More**:
SDK list lacks cursor input; do not silently use history_page (different API),
increasing-limit pseudo-pagination or direct HTTP. Empty success is a valid empty
list, not an instruction to clear chat/create a session. Read failure retains last
good rows with a fixed sanitized retry error; retry is explicit. Successful refresh
increments list_revision, invalidating old row actions but retaining selection,
drafts/recovery and current chat even if its ID is absent. A loading list blocks
all command navigation/send for this serial slice; editor typing stays available.

## 6. Transactional transitions and failure handling

| Transition | Before reply / commit | Success | Failure / stale |
|---|---|---|---|
| Stable A → B | Reserve draft slot, allocate navigation op, generation +1, loading true. Keep A selected, chat/history/mode and editor live; close menus. No speculative snapshot/reset. | Require matching op/generation/target ID. Synchronously snapshot **latest** A text, remove B inactive snapshot, replace editor with B text or empty, select Stable B, replace history/chat/mode projection, clear loading. | No editor reset or key/projection change. Keep latest A edits and recovery. Show scoped error; keep incremented generation (never roll backward). |
| A → B → A | Each completed switch follows above; active key absent from inactive map. | Exact UTF-8 drafts restored independently, scroll zero each ownership transfer, same editor entity. | No partial transfer/merge on failed second switch. |
| New → created stable | Keep New editor live during request. | Successful ID/history moves key; no draft rewrite needed, fresh revision. If target draft conflict, refuse commit. | Source New editor retained; retain known created ID for explicit Open retry. |
| Stable A → new created stable | Keep A live while reserved target New is empty. | Snapshot A, select new stable with empty text; migrate target token, not A. | A selected/live; target retry metadata retained if known. |
| Reload same stable | No draft snapshot/reset; generation +1, retain recovery/live until valid History. | Replace canonical history/live; no editor operation. No active turn: clear selected uncertainty; active turn: resume existing SDK follow, busy true and Send blocked. | Preserve previous projection and uncertainty. No automatic Send or pending retirement. |
| List | No generation increment; pending list operation/loading identifies request. | Replace only rows/state/revision; no selected text/key change. | Preserve last good list and editor/chat. Stale reply does not clear a newer loading operation. |
| Send → admission/failure | Recovery keyed by stable session and unique ID, pending stores send generation/op; no inference replay. | Matching Admitted removes exactly associated entry/pointer, sets admitted turn; never touches editor or another session. | Uncertain for that session, recovery retained. Enqueue failure is conservative unknown, sender disconnected; never auto-retry. |
| Explicit Restore | Require selected Stable, empty current editor and no busy/loading. | Transfer one selected-session last entry into same editor, fresh revision/scroll zero; remove only its pending association if it points at that ID. | Nonempty/newer draft, stale context or absent entry: no-op/error, no command and no removal. |

Commit is synchronous in one controller call with prevalidated counters/capacity,
keys and valid existing Prompt before taking snapshots out of the map. No event,
await, shell sync or observation step may see a half-transfer. If reset adapter
cannot guarantee its existing live entity preconditions, refuse **before** taking
any entry; do not claim arbitrary panic recovery/atomic daemon snapshot support.
Loading does not freeze typing: snapshot source at success, not at request start.
Failed Open must not call `Chat::fail` on an otherwise known idle A, erase A's mode,
or remove A's live messages. Reload of already uncertain A preserves that state
on failure. No optimistic `mode=None` until successful different-session commit.
Mode follow-up errors affect metadata/status only, never undo a successful History.

Generation increases once at navigation begin, once at accepted Send, and once
on explicit local `draft:unsent` owner transfer; not on list, Restore, cancel,
theme or a refused/disconnected SDK-backed action. Never decrement on rollback. Reply routing first preserves exactly existing stale exceptions:
**Models / Modes / Unavailable only**. No SessionsListed, History, Mode, Notice,
Failure, Stream or Admitted exception is added. Then check operation/session
context before mutation. Even matching-generation wrong-session/op replies are
ignored. Bootstrap None context is not a wildcard for a command outcome.
Unavailable remains global: disconnect, busy/loading false, preserve editor,
selected projection and all recovery; abandon pending UI operation without commit.
If a send/follow could be in flight, mark selected stable session uncertain; do
not manufacture admission. Catalog updates cannot finish pending operations,
change selected identity, clear uncertainty or remove recovery; their exception
is not permission for these side effects. Preserve existing stream turn/sequence
checks, non-atomic reads and bounded reconnect policy unchanged.

## 7. Recovery and uncertainty are separate ledgers

A pending pointer identifies **one recovery ID in one session**, never “last global
prompt” or a vector position. Sending from B must not retire A's recovery, even
with identical text. Only one in-flight send exists in this serial UI, but old
unconfirmed recovery may remain across reconciled sessions.

- Admission failure, fatal stream or lost stream makes selected stable execution
  uncertain; Send blocked until explicit matching history reconciliation (normal
  admitted stream settlement still uses existing bounded settled_history).
- While uncertain and not busy, only explicit Reload reconciles execution;
  New/Recent/row navigation/list/mode are refused. Cancel may request cancellation,
  not certify it. Restore may transfer text locally but uncertainty stays set.
- Matching explicit History with no active turn clears uncertainty, **not** recovery
  or its pending pointer. A subsequent send is deliberate user intent, never
  replay; old pending pointer may then be superseded while its old recovery entry
  remains individually recoverable. Pending is not “all recoveries in session.”
- A late old Admitted after Reload's generation increment cannot retire anything,
  even if the old pending pointer remains. A matching-generation/op/session
  Admitted whose pointer was explicitly restored cannot delete another entry.
- Successful history with active turn reattaches the existing execution; busy
  prevents navigation/Send. It is not a new admission and does not retire recovery.
- Recovery UI projects only the selected session (count/last-entry Restore and
  notice); inactive recovery exists but is not attributed to another chat. No
  draft is reconstructed from history, turn prose, list preview or recovery text.
- Restore is **move, not copy**: exact one entry removed, pending cleared only if
  associated with it; no command even with no worker. It does not certify whether
  the daemon accepted that prompt. Reload before choosing to send restored text.
- Transport absent: all SDK-backed actions enqueue zero commands and leave
  generation/selection/draft/recovery/pending state unchanged apart from a static
  offline notice. Local menu/theme/copy/Restore remain local; there is no hidden
  reconnect, queued deferred send or worker recreation.

## 8. Synthetic acceptance plan — designed, NOT written/registered/run

Later A tests go to new `session_tests.rs` only after architect registration;
adapter additions in `backend_tests.rs`. C owns adapter/input tests in new
`composer_tests.rs`; architect alone updates shared tests/wiring. No live daemon,
private sessions, credentials or provider calls. Use synthetic App/channel replies
and SDK test transport conventions; registration is required before counting tests.

| Case | Required observations / negative assertions |
|---|---|
| A → B → A Unicode/multiline | `α\n世界🙂` and `β\n独立` round-trip byte-for-byte; same Prompt entity; inactive selected key absent; scroll zero and marker absent after each reset. |
| Independent identical prompts | A and B both `same\n🙂`; clear/restore/admit A never removes or merges B's text/recovery. Identity exclusively keys/IDs. |
| Initial New and Stable → New | Unsent initial multiline survives successful create key migration; Stable source retains own snapshot, newly created target is empty; no Send. New → Recent → draft:unsent restores the original unsent text locally (also offline); no orphaned snapshot and no SDK command from unsent selection. |
| Create succeeded/history failed | A/New stays selected, latest typed draft intact; known created ID displayed as retry target; explicit retry emits Open(that ID), never New/Send. Collision with existing Stable snapshot refuses merge. |
| Failed Open and malformed target | Selected identity, mode/history/live/editor unchanged, scope error shown, generation not rolled back; unexpected History ID rejected; invalid/stale row action enqueues nothing. |
| List success/empty/failure/refresh | One list(50); optional cursor is truncation indicator only; duplicate/empty IDs handled, overreturn capped. Last good rows retained on error, empty list does not clear chat. Missing selected ID retained; refresh invalidates old row action. |
| Busy/loading/uncertain | Table-test direct forged actions as well as disabled buttons; navigation refused, no command/generation movement. Uncertain Reload available, no Send before valid history; local Restore does not unlock Send. |
| Stale/misrouted replies | Old generation and same-generation wrong op/session for History/Mode/Failure/Stream/Admitted/List do not select/reset/remove recovery/finish newer loading. Exactly catalog Models/Modes and Unavailable exceptions retained. |
| Typing while Open pending | Source text edited after request is snapshotted at successful commit; on failure it stays live. No request-start clone overwrites it. Reply commit invalidates earlier captured Send/Restore from source. |
| Submit/no newer overwrite | Accepted send clears only current captured key/revision/text. Edits-away-and-back across epochs, queued editor action, ambiguous later key or reply-driven switch cannot clear/send the wrong draft. Failed enqueue retains text/recovery. |
| Reload/recovery | Failed admission → newer draft → explicit reload(no active) keeps recovery/pending, leaves newer draft and clears uncertainty only; active/history error does not certify idle. No Send from reload or reattachment. |
| Restore move | Empty selected editor moves last selected entry; exact ID removed; only own pending association cleared, others remain; no command; uncertainty preserved; nonempty/newer draft refusal and offline local move. |
| Admitted own pending only | Matching generation+op+session removes only pointed ID; wrong/stale/duplicate admission and pointer already restored cannot remove others or reset newer editor. Superseded old pending recovery remains. |
| Cancel/terminal/non-atomic history | Cancel Notice never settles; terminal retains existing settlement reconciliation, failed bounded reads stay uncertain. Preserve execution-owned assistant filtering/sequence checks, no text deduplication or Send retry. |
| Disconnected/capacity/overflow | All remote actions zero enqueue and no destructive mutation; closed sender handled conservatively. Reserved draft-slot limit and recovery limit refuse before loss; checked counter overflow refuses without wrap. |
| Editor/observation | Same entity on switch/reload/theme; scroll reset only on replacements, reload preserves scroll; no FieldTransactions at any exit. Offline inactive nonempty draft/recovery/list metadata must not become Public. |

P3 still requires separate nonempty ime_preedit+Enter and ime_commit+Enter cases,
not enabled-but-idle IME; no command, preserved composition/draft outcome, no marker
leak. P2/P4 480pt and real-trigger popup evidence remains deferred, not closed by
this plan. Physical keyboard/IME/clipboard/accessibility, compositor, live daemon,
provider, huge messages and full parity remain unverified.

## 9. Explicit shared-interface requests and ownership

No shape changes authorized until architect/C review and sender checkpoint.
Architect serializes atomic changes with each file owner, refreshes bases and then
freezes the interface; no concurrent Model/Ui/Handles/action edits by lanes.

| Request | Owner / consumer / checkpoint |
|---|---|
| SessionKey, DraftSnapshot, correlated operation/recovery shapes, new Model fields and defaults | Architect serializes with A in state.rs; main.rs initialization owned by architect; B reads projections and C accepts snapshot interface. |
| Request/Reply context, ListSessions and typed Update outcomes | Architect freezes with A/backend/controller; backend_tests later owned by A. SDK/shared types unchanged; no daemon endpoint. |
| Draft snapshot/replace/conditional-clear API and pre-reply context capture | Architect + C review; C owns composer.rs mechanics, A controller transition calls. No new schedule required: epoch/capture at controller entry after existing Input guard. |
| Stable row actions, reachable local unsent draft action, list/error/truncation/creation-retry UI, selected-only recovery | B owns shell.rs and Ui handles, architect serializes any handle shape. No extra Handles changes expected. B emits frozen strings only, never SDK or Model transitions. |
| New session/composer test modules and existing regression adaptation | Architect alone registers modules and edits existing ui/interaction/qualification tests; A/C supply packet-local tests later. An unregistered module is not a pass. |
| New private-state observation denial inputs | D with architect authority review must add checks for inactive nonempty snapshots/recovery and retained list metadata atomically before new private state can be enabled under observe, not merely before fixture capture. This is stricter denial, not capture expansion; no changes in P1a. |

B's synchronous move/restore projection remains synchronous and does not own
selection, recovery, admissions or draft writes. C never dispatches SDK commands.
D's synthetic fixtures do not become ordinary transition writers. No new generic
store, framework, persistence layer, protocol or competing editor resource.

## 10. Decisions, tradeoffs and review questions

**Proposed decisions to accept or revise explicitly:** transactional selection
commits only on history success; allow typing during loading; no caret/scroll
restoration across switches (reset to zero); Reload leaves editor/scroll alone;
uncertainty restricts navigation to Reload; retain recovery until matching
Admitted or explicit move; list fixed 50 with honest unavailable pagination;
local editor revision uses conservative per-Update epochs; process-only memory.
These choose safety/small scope over optimistic navigation, complete catalog,
process-restart recovery, exact edit counts or per-session caret persistence.

**Open for architect + C review, before implementation:**

1. Approve concrete 50/32/64 limits and reserved-slot refusal UX; count bounds are
   not byte-memory limits. No eviction or text truncation is permitted. A later
   byte-budget policy would require an explicit lossless refusal contract.
2. Validate C's synchronous adapter preconditions and conservative epoch capture
   against native Submitted timing and theme/menu action FIFO. Same-frame native
   edit provenance is limited by the framework; ambiguous guard stays, no stronger
   Enter-time guarantee is claimed. Approve capturing context before replies while
   preserving reply-before-action application order.
3. Approve correlation envelope and typed navigation failures, including created-ID
   retry UX. Existing New path collapses create/history errors; preserving known
   identity needs that bounded local adaptation. Unknown create outcome cannot be
   recovered automatically under existing APIs; show honest uncertainty, no retry
   replay. Decide explicit abandon label/copy with B, not an implicit merge policy.
4. Approve stricter uncertain navigation/list/mode refusals and offline local Restore
   as intentional UI/controller changes. They are not accidentally introduced by
   a broader stale-reply exception. Confirm pending supersession after explicit
   history reconciliation leaves the old recovery entry recoverable.
5. Architect/D must agree private-state denial checks before enabling new state: inactive
   memory and list metadata are new private state not covered by the old five
   conjuncts. No export/capture expansion or physical certification is proposed.
6. P1b implementation dispatch must name refreshed exact base, shared-shape owner
   sequence and registered tests. No implementation follows from this handback.

## 11. Handback validation boundary

Only documentation/source reads and bounded Git/hash/whitespace/scope checks are
allowed/performed for P1a. Full commit/base/path delta and exact validation commands
are returned with handback. No designed acceptance case above has been executed;
no Rust/source/test registration is delivered. All 55 sealed review files and
execution-candidate source identity must remain byte-identical to the base.
Sender retains orchestration, independent review decisions and native integration.
