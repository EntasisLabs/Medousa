# Reviewing delegated work

On a supported workshop, ask Medousa to have Medousa Coder, Codex, Cursor, or
Hermes work on a project. Medousa can discover local agent availability and
delegate work that your request already authorizes, or suggest a proposal when
another decision is needed. In Assistant mode on mobile, it can also
prepare that proposal on an authorized paired workshop selected from active-work
discovery.

## Tracking work in chat

Delegated agents and workshop background workers appear in an **agent group**
inside the turn that requested their work. The group shows how many agents are
working, ready for review, accepted, or need attention. Each row leads with its
task, status, and current activity. Results update those rows in place; finishing
does not move an agent to the bottom of the conversation.

Groups show four tasks initially. Open **more agents** to see the rest together,
or collapse the whole group. Open a task to see its worker, workshop, sender
responsibility, result, and review. The full assignment and technical provenance
stay behind **Assignment** and **Details**. Workshop workers also offer their
transcript and tool activity. If more requests are available from the workshop,
choose **More requests** to load them into their corresponding groups.
Assignments whose originating entries are outside the loaded history remain
available together before the visible turns.
Workshop worker results include **View agent**, which opens the originating
group and task so you can reconnect the result with its assignment.

Local sender-controlled handoffs can start directly when your request authorizes
the work. Only proposals needing your decision show **Approve & start** (or
**Approve & adopt**). By default the sender retains responsibility, receives
acceptance and completion updates, and reviews the returned result. A finished
worker stays **Awaiting sender review** until the sender records its decision;
**Result accepted** means that review was accepted. If the sender chose worker
completion without review, the line says **Work completed** instead.

Ownership transfer, review, sender wakeups, and user follow-up are separate
choices captured by the sender. Inspect them under **Details**.
Choosing no user follow-up does not suppress the sender's requested callbacks.
An unavailable workshop leaves the last observed work visible. Refresh errors
and delayed progress do not turn a working assignment into a failed one.

On the execution workshop, the expanded task offers **Open project**, and native
Medousa Coder assignments offer **Open chat**. For another connected workshop,
open that workshop first; session and project ids remain scoped to its daemon.

## Background Medousa workers

Assistant mode on mobile can also queue a Medousa background worker on the
workshop bound to the chat. This is separate from the Codex, Cursor, or Hermes
approval proposals described below. Medousa saves the request and its parent
chat context on the phone, then returns a work id while the request is queued.
The phone resolves and authorizes a destination from a fresh authenticated
workshop inventory before dispatch, and records that exact route before the
worker starts. Retries keep using that route. Peer destinations apply their
scoped execution policy; portal destinations use their direct workshop role.
Coder work can use a project already bound to the target runtime. If you
explicitly ask Medousa to create or clone a project on a portal, it can include
that setup in the worker request; the portal creates and binds the Forge project
on its own disk before starting the Coder worker. Peer destinations still need
an already-approved project. You can check queued work with the workshop status
control, and cancel a request while it is still queued to prevent dispatch.

The phone must stay active long enough to deliver a queued request. If it sleeps
or loses its connection first, delivery resumes when it wakes and reconnects.
Once the paired Mac accepts the worker, that worker continues there if the phone
goes away. If the requested target stays unavailable, Medousa reports a failure
after its bounded retry period. This flow does not add push notifications or
indefinite phone background execution. Completed results return to the original
chat after the workshop is reachable again.

## Agent approval proposals

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

On a supported workshop, a proposal appears in its chat turn's agent group. Expand it to review the
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

## Add another daemon as a worker

A Home portal connection and a delegated worker connection are deliberately
separate. **Add workshop** lets Home browse and chat with another workshop. It
does not give the local engine an identity it can use for delegated execution.

To let this workshop delegate work to another daemon:

1. On the destination workshop, generate a full v2 pairing link containing an
   Iroh ticket (`medousa pair qr --full`). The destination daemon can remain on
   loopback; its HTTP port does not need to be reachable from the caller.
2. On the machine that will delegate, open **Settings → Connection → Remote
   workers** and choose **Add worker**.
3. Paste the destination pairing link, confirm its name, then choose
   **Add worker daemon**. The daemon uses the ticket for pairing and subsequent
   worker requests. Legacy links without a ticket require a reachable LAN address.
4. Select the paired runtime from the Workers control when you want to target it.
   The picker offers **Auto**, the current workshop, and each paired worker.
   Rename or remove paired workers in **Remote workers**; the name is stored on
   the local daemon and does not rename the destination workshop.

The local `medousa_daemon` creates and stores its own identity and credentials.
Home never reuses or exports its portal identity for delegated work. Removing a
portal therefore does not silently remove its worker identity, and vice versa.

The same flow is available from the CLI:

```bash
medousa pair join '<full-pairing-link>' --label 'Studio Mac'
medousa pair workers
medousa pair targets
medousa pair target use <runtime-id>
medousa pair target local
medousa pair worker-remove <worker-id>
```

`pair target local` returns delegation to this workshop. Pairing credentials are
kept by the daemon in its private data directory rather than by the CLI process.
