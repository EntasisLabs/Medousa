# Reviewing delegated work

On a supported workshop, ask Medousa to have Codex, Cursor, or Hermes work on
the project bound to this chat. Medousa can discover local agent availability
and prepare a delegation proposal. In Assistant mode on mobile, it can also
prepare that proposal on an authorized paired workshop selected from active-work
discovery. Cloud adapters and autonomous chains are not yet connected.

You can also ask what work is active without first opening a project-bound chat.
Medousa can list your non-terminal governed projects and visible agent sessions
across the current and authorized paired workshops. Offline or unavailable
workshops remain visible as unavailable; Medousa does not treat them as empty.

If Codex, Cursor, or Hermes is already running through Medousa on the same
governed project, Medousa can offer that exact live session for adoption. Adoption
attaches ownership and waits for its real terminal result; it does not send the
task again. Sessions from another project, sessions you cannot see, and sessions
already owned by another assignment are not eligible.

Local proposals require the chat to be bound to a governed project on the current
workshop. A mobile Assistant can instead select an exact governed project on a
paired workshop. Medousa shares a bounded committed conversation range with
immutable provenance, not a separate model-authored transcript. Discovery
suggests the most recent 32 entries; one proposal can share at most 256 entries.
Review that selection before starting.

On a supported workshop, a proposal appears above the composer. Review the
agent, instructions, channel, governed work item, execution workshop, exact
shared conversation ranges, expiry, and owner-continuation choice.

Choose **Approve & start** for new work or **Approve & adopt** for discovered
existing work. Approval is durably recorded before startup or observer
attachment. If provider startup
fails, the same card remains approved and offers **Start approved work** for a
safe retry. Choose **Decline** instead to permanently reject it; a revised
request needs a new assignment. Expired proposals cannot be approved or started.
Conversational proposals expire after one hour. Saying “yes” in chat or Live
does not replace these approval controls or start the agent.

If starting fails, the request remains visible unless recorded peer custody
already exists. Retrying does not blindly spawn another process; uncertain
dispatch or owner turns require explicit reconciliation. “Accepted” means the
agent accepted the assignment, not that its work passed review or was deployed.
The card remains in the chat as tracked custody until the daemon records a
terminal receipt; it then yields to Medousa's attributable owner continuation.

Owner continuation, when included, permits one bounded Assistant turn in the
same chat. Medousa can explain the verified result and, only when the existing
conversation already requested a next Codex/Cursor/Hermes step, prepare one
follow-up proposal against the same governed work. The continuation can inspect
peer scope and create that proposal; it cannot approve, launch, steer, cancel,
deploy, or inherit the completed peer's authority. Every follow-up still appears
as a separate approval card before execution.

For work executed on a paired workshop, the originating phone chat also projects
the immutable terminal outcome and result into the delegation card. When owner
continuation was included, the execution workshop commits Medousa's answer while
the phone is away. On return or reconnect, the phone retrieves that committed
answer and adds it to the original chat once. Reopening a previously visited
chat also refreshes its history. A follow-up proposal prepared by that
continuation is projected into the originating chat as a new approval card.

This return requires the original paired workshop to be reachable. Keep the
workshop daemon running while the phone is away; closing Live or the phone app
does not cancel accepted work. The return flow refreshes while the app is open;
it does not send a background notification or start Live automatically. Proposals
created before source-chat return tracking was available retain their existing
card-based result view.

This surface requires operator execution access on the workshop that owns the
proposal. Unsupported workshops do not show it. The phone aggregates proposal
inboxes from Personal and paired workshops while the chat stays on Personal;
approval and launch are still executed by the exact destination workshop.

## Trying the delegation and return flow

1. In Assistant mode on the phone, ask what work is active and select an exact
   project and available agent on a paired workshop.
2. Ask for a small, verifiable task. Review the proposal and its shared context,
   include owner continuation, then choose **Approve & start**. For existing
   running work, use **Approve & adopt** instead.
3. Leave the chat or close Live while the workshop completes the task. Return to
   Personal and open the original chat with the paired workshop reachable.
4. Check that Medousa's answer appears once in that chat and describes the actual
   result. Reopen the chat and reconnect again to check that it stays a single
   answer. Any proposed next step still needs its own approval card.

A terminal agent result is evidence that its prompt finished. Inspect the changes
and relevant checks before treating the project as reviewed or ready to publish.
