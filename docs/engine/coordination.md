# Coordination proposal approval (native preview)

Logical coordination channels are not Telegram/Slack delivery channels. Proposal
snapshots retain authority-qualified channel/session references, exact context
ranges and digests, a governed Forge work item, and independent peer custody.

## Conversational proposal tools

The full-daemon General host's `peers` discovery domain contains
`cognition_active_work_discover`, `cognition_peer_discover`, and
`cognition_peer_propose`. All require an admitted owner turn and use its
authenticated principal/session. Active-work discovery lists the current
workshop's non-terminal Forge projects owned by that principal and joins visible
ACP sessions by exact Forge work id. It reports owned/adoptable, terminal, and
cancelled session state without granting authority or changing custody. Results
are capped at 128 projects, expose no repository path, and explicitly report
that current-workshop coverage is not yet a complete mesh inventory. Callers
must not represent it as cross-workshop discovery.

Peer discovery reports local ACP availability and the current chat's project
binding and committed range.
Proposal input contains only `request_key`, `runtime`, `instructions`,
`after_entry_seq`, `through_entry_seq`, `continue_owner`, and the optional exact
`existing_agent_session_id` returned by discovery; unknown fields are rejected.
The host derives owner, workshop, session, channel, Forge work, range digests,
manifest, grant reference, and independent coordination execution session.
Project binding must explicitly name the current execution runtime.

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
changed intent under the same key fails closed. Neither tool approves or launches
work. Worker/remote-delegated/mobile ceilings are unchanged; embedded mobile
composition, cloud adapters, and cross-workshop proposals remain unsupported.

## Native operator approval

These native-only routes require `admin.execute` plus a bound owner identity
(`content.read` and `workshop.interact` are rechecked by the service):

| Method | Route | Behavior |
|--------|-------|----------|
| GET | `/v1/coordination/proposals?session_id=...&after=...` | Owner-session inbox; optional opaque proposal-id cursor |
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
proposal-creation HTTP route, model grant-issuance tool, autonomous follow-up
authority, or mobile embedded-host composition.
