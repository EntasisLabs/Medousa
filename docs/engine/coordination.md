# Coordination proposal approval (native preview)

Logical coordination channels are not Telegram/Slack delivery channels. Proposal
snapshots retain authority-qualified channel/session references, exact context
ranges and digests, a governed Forge work item, and independent peer custody.

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
Denied and recorded peer assignments leave this inbox. Each indexed proposal is
validated against its immutable snapshot and current channel owner. Source
visibility is fully rechecked on approval and dispatch, not inferred from channel
membership. Corruption or exceeded scan budgets fail closed. The index is
create-only; an interrupted index write is repaired by exact proposal replay.

Approval records precede grant compilation. Dispatch repairs partial compilation
from an already approved snapshot before executing. Retries use persistent
assignment/command fences; uncertain dispatch is not proof of failure and must
not trigger another spawn. Accepted custody is not verified Forge completion.

The daemon advertises `coordination.operator_proposals.v1` only when the local
coordination host is composed. An unavailable host returns 503; scope, expiry,
visibility, and immutable-write conflicts return 409. This increment exposes no
proposal-creation HTTP route, model grant-issuance tool, autonomous follow-up
authority, or mobile embedded-host composition.
