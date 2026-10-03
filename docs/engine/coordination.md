# Coordination proposal approval (native preview)

Logical coordination channels are not Telegram/Slack delivery channels. Proposal
snapshots retain authority-qualified channel/session references, exact context
ranges and digests, a governed Forge work item, and independent peer custody.

## Conversational proposal tools

The portable host exposes read-only `cognition_active_work_discover` in ordinary
conversation modes. Assistant additionally exposes local
`cognition_peer_discover` / `cognition_peer_propose` and the location-neutral
`cognition_peer_delegate`. All require an admitted owner turn and use its
authenticated principal/session. Active-work discovery lists each reachable
workshop's non-terminal Forge projects and joins visible ACP sessions by exact
Forge work id. The local workshop uses the admitted principal; paired workshops
use their active workshop identity after authenticating the signed mesh request
and directional execution policy. It reports owned/adoptable, terminal, and
cancelled session state without granting authority or changing custody. Results
are capped at 128 projects per workshop and expose no repository path.

Federated reads use `/v1/mesh/active-work` over the existing pinned pairing and
signed request/result envelope. Configured workshops that cannot answer remain
explicit unavailable rows; they are never represented as empty. Consequently
`coverage.complete_mesh` is true only when every configured authorized workshop
answered. Inventory authority does not imply adoption or execution authority.

Assistant also exposes `cognition_assistant_placement`. Its `requirements`
describe required capabilities, requested/forbidden executors, adapters,
runtimes and workshops, exact governed work, an optional exact adoptable ACP
session, and context locality. Source locality is derived from the admitted
workshop. The query ranks observed workshop and local ACP candidates with
rejection reasons; unavailable targets remain visible. Missing capability or
target evidence cannot satisfy a constraint, and incomplete discovery remains
explicit. General and Teacher do not receive this tool. Ranking neither issues
a grant nor starts or adopts work; native execution admission still applies.

## Assignment inspection

Session-independent intent and resource relationships use the separate
[work-domain registry](work-units.md). Its bounded `work.coordinate` controller
registers native executor/reviewer proposals before effects and observes their
terminal receipts without an owner-chat continuation. Existing native approvals,
exact grants, and source-session visibility still admit every new stage. A
registered proposal's dispatch operation queues the stage with that controller;
its existing response can contain `binding: null` while approval/dependency or
native custody is pending. Ordinary unregistered proposals still dispatch directly.

On a full workshop daemon, `cognition_runtime_query` exposes `assignment.list`,
`assignment.get`, `assignment.events`, and `owner.events`. The admitted turn supplies the
principal; these actions do not accept an owner or principal override. Every
read also rechecks visibility of the assignment's source session.

`assignment.list` accepts optional `kind`, `terminal`, `limit` (1–100, default
20), and `after_assignment_id`. Its `next_cursor` is the next
`after_assignment_id`. `assignment.get` takes an exact `assignment_id`.
`assignment.events` accepts `assignment_id`, optional `limit`, and an opaque
`cursor`; use its returned `next_cursor` unchanged. Event cursors distinguish
observations sharing a native sequence. Use `cognition_schema` for the typed
action parameter schemas.

`owner.events` accepts optional `limit` and `cursor` and returns durable event
state: pending, started, consumed, or blocked, with exact acknowledgment or
blocker evidence. Only events owned by the authenticated principal with a
currently visible source session are returned. Approval observations are
inspectable but do not spend the grant reserved for terminal-result intake;
waking on approval requires a separate event-scoped continuation grant.

The ledger projects registered internal workers, external peers, jobs,
workflows, recurring schedule definitions, and scheduled workflow occurrences.
It reports current-workshop coverage explicitly; it is not an inventory of
every job on every paired workshop. Native engines retain execution authority.
Recurring definitions and their occurrences have distinct identities. A
terminal execution is not evidence that code was reviewed or verified.

Snapshot replay preserves original observation timestamps, and immutable
identity conflicts fail closed. Event folding is qualified by workshop
authority, so equal native ids on different workshops cannot mix histories.
Projections from async services use bounded store admission. Worker projections
follow the native workspace journal commit; projection failure does not change
the worker's native execution state.

An interrupted projection with a terminal observation but no committed terminal
marker reports `unknown` until native snapshot replay reconciles it. A corrupt
or conflicting terminal marker fails closed.

## Conversational peer proposals

Peer discovery reports Medousa Coder and local ACP availability, saved coding
runtime preferences, and the current chat's project binding and committed range.
Proposal input contains only `request_key`, optional `runtime`, optional
`forge_work_id`, `instructions`,
`after_entry_seq`, `through_entry_seq`, `continue_owner`, and the optional exact
`existing_agent_session_id` returned by discovery; unknown fields are rejected.
The host derives owner, workshop, session, channel, Forge work, range digests,
manifest, grant reference, and independent coordination execution session.
An explicit `forge_work_id` names a project owned by the authenticated user on
this workshop; the source chat need not be bound to it. When omitted, the legacy
chat binding must explicitly name the current execution runtime. Omitted
`runtime` follows saved preferences (default Medousa Coder); explicit selection
wins. Exact retries retain the original runtime even after preferences change.

Medousa Coder uses a dedicated native turn and Forge lease through the same
immutable assignment, approval, claim, and receipt contracts as external peers.
Its turn correlation is persisted before admission. Native Coder tools recheck
the exact grant, source visibility, project ownership, and registered work scope.
Checkpointed and input-needed turns are interrupted receipts, not completion.
Native peer assignments currently execute in their admitted Coder turn; child
worker spawning is denied until those workers can inherit the exact grant.
Additional parallel work requires separate scoped assignments.
Revocation cancels the native turn. Uncertain native claims are never relaunched
from disk. Codex/Cursor/Hermes continue through ACP; Medousa is not sent to the
ACP session endpoint or external-agent Bot adapter.

Discovery includes visible, unowned local ACP sessions bound to the same Forge
work. Adopting one never sends another prompt or restarts its provider. Approval
records coordination custody and attaches the durable terminal observer to the
running session. Terminal state is shared across the prompt pump and registry,
so completion racing adoption is replayed into the assignment exactly once.
Cancelled, foreign-project, hidden, wrong-runtime, or already-owned sessions
fail closed. The initial adoption seam is process-local; a provider absent from
the live registry after daemon restart is not invented from Forge metadata.

Proposals use a one-hour expiry and a stable owner/session/authority-scoped
request key. Exact retries reuse the immutable snapshot and repair its index;
changed intent under the same key fails closed. Local discovery and proposal
tools do not approve or launch work. On embedded mobile, `cognition_peer_delegate` accepts only an exact
agent-selectable runtime and Forge work id returned by active-work discovery. It
sends a signed, bounded, digest-checked context grant to
`/v1/mesh/peer-proposals`. The destination rechecks directional `assistant.work`
policy, agent targeting, exact runtime, work ownership, and context provenance
before creating a request-scoped shadow session and the normal immutable local
proposal. The transfer carries no approval or execution grant. Exact retries
reuse the shadow; another request key from the same phone chat gets an independent
immutable shadow. Worker and remote-delegated ceilings remain unchanged.
An existing destination-owned trusted Assistant policy may approve and launch
the remote proposal; otherwise the destination returns an approval card.
Discovery, pairing, and model-written instructions never create that policy.

## Returning a paired-workshop result

New mobile proposals persist an exact source association before sending intent,
then bind it to the verified signed proposal response. The destination records
the authenticated sender and expected assignment identity before dispatch.
This association is independent of whichever chat the user currently has open.

`POST /v1/mesh/peer-assignment-results/query` accepts a signed `TaskRequest`
containing `schemaVersion`, `sourceDeviceId`, and `sourceRequestDigest`. A finalized
source also supplies the complete `proposalId`, `proposalRequestDigest`, and
`assignmentId` tuple. Partial tuples and mismatches are rejected. Only the
originally authenticated source peer can query that exact association. The signed
`TaskResult` echoes the source request digest and returns the saved immutable
proposal, when available, plus either a pending `completion: null` or the native
receipt, committed owner acknowledgment, assistant decision, and content digest.
The proposal response also echoes the source request digest, preventing a valid
response for a different request from being attached to this chat.

If the original proposal response was lost, the source recovers its saved proposal
through this read-only query before accepting a completion. Recovery never sends
the proposal again or starts an executor. A missing destination record stays
pending rather than being treated as evidence that execution failed.

Home performs a bounded rotating sweep on foreground, reconnect, and periodic
refresh. It verifies the pinned workshop signature and the saved proposal,
assignment, session, receipt, and decision identities before appending to the
canonical source transcript. The append carries the remote decision reference
and a deterministic execution correlation. Exact replay after a restart is a
no-op; conflicting content or revoked source visibility fails closed. The source
marks delivery complete only after the transcript commit, and merges committed
history into the visible chat without replacing an active response stream.

This is foreground result retrieval, not background push or Live invitation
delivery. Proposals without owner continuation retain their terminal result in
the delegation card. Older proposals without a durable source association are
not retroactively bound by title, session-name matching, or model inference.

## Native operator approval

These native-only routes require `admin.execute` plus a bound owner identity
(`content.read` and `workshop.interact` are rechecked by the service):

| Method | Route | Behavior |
|--------|-------|----------|
| GET | `/v1/coordination/proposals?session_id=...&after=...&selected_proposal_id=...` | Owner-session inbox, including paired request-scoped shadows projected through their immutable source chat; optional opaque proposal-id cursor and exact selected-assignment observation |
| POST | `/v1/coordination/channels/{channel_id}/proposals/{proposal_id}/approve` | Approve the immutable snapshot; compile exact grants |
| POST | `/v1/coordination/channels/{channel_id}/proposals/{proposal_id}/deny` | Immutable denial |
| POST | `/v1/coordination/channels/{channel_id}/proposals/{proposal_id}/dispatch` | Dispatch an already approved snapshot |

POST bodies are exactly `{}`; unknown fields are rejected. The deciding owner
comes from authenticated identity, not JSON. Channel authority comes from the
destination daemon, not the caller. Approval cannot be reversed into denial or
vice versa. A revised proposal requires a new assignment identity.

Inbox pages contain at most eight proposal/decision records and one MiB of JSON.
Denied assignments leave this inbox. Recorded custody remains visible to the
owner as active work until its terminal receipt arrives. Each indexed proposal is
validated against its immutable snapshot and current channel owner. Source
visibility is fully rechecked on approval and dispatch, not inferred from channel
membership. Corruption or exceeded scan budgets fail closed. The index is
create-only; an interrupted index write is repaired by exact proposal replay.

The optional `selected_proposal_id` returns an owner/session-scoped
`tracked_proposal` independently of the pending page and its cursor. This
projection retains custody and the immutable terminal receipt after local work
leaves the pending inbox. An unrelated session, owner, or unassociated source
shadow cannot inspect it. Selecting an assignment grants no execution authority.
The pending page keeps its one-MiB budget; the combined response with progress
and the extra selected record is capped at two MiB.

Assigned records may include an optional `progress` observation with
`observed_at`, `last_activity_at`, `current_activity`, `last_activity`, and
`last_activity_status`. Execution states are `accepted`, `running`, `blocked`,
`awaiting_receipt`, and `unobserved`. Activity statuses are `running`,
`succeeded`, `failed`, and `blocked`; an action failure is not an assignment
failure. Only `receipt` establishes the assignment outcome.

Native progress joins the exact execution-session/turn ticket, Forge attempt,
and bounded Coder activity. External progress inspects the exact ACP custody,
pending permissions, and its matching Forge attempt/lease. Progress includes
no raw reasoning, tool payloads, or activity from other attempts. Filesystem
inspection is admitted through `ForgeExecutionService`. Missing process-local
custody after a restart is `unobserved`, even when durable attempt metadata
still says running; inspection never retries or dispatches work.

Approval records precede grant compilation. Dispatch repairs partial compilation
from an already approved snapshot before executing. Retries use persistent
assignment/command fences; uncertain dispatch is not proof of failure and must
not trigger another spawn. Accepted custody is not verified Forge completion.

Owner-result intake also reconciles the safe post-commit restart boundary. If
the process-local turn ticket is gone but the canonical owner transcript already
contains a non-empty assistant decision attributed to the exact intake execution,
the daemon acknowledges that durable decision instead of leaving the receipt
stuck. A missing ticket without that committed evidence remains unresolved and
is never rerun or acknowledged automatically.

The daemon advertises `coordination.operator_proposals.v1` only when the local
coordination host is composed. An unavailable host returns 503; scope, expiry,
visibility, and immutable-write conflicts return 409. This increment exposes no
model grant-issuance tool or autonomous follow-up authority. The signed mesh
proposal route is peer-only and destination-policy-gated; it is not a native
operator proposal-creation endpoint.
