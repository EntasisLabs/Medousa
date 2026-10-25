# Bots

**Audience:** Medousa app users who want a named teammate that remembers an
ongoing working relationship.

A **Bot** is a named teammate built from an **archetype**: reusable expertise and
an approach to work. It keeps a name, purpose, avatar, its own memory, and a
primary conversation on the active workshop.

Bots, modes, and archetypes do different jobs:

| Control | What it changes |
|---------|-----------------|
| **Bot** | Who you are working with and which durable memory continues |
| **Mode** | How this turn is handled: General, Assistant, Teacher, Instant, or Coder |
| **Archetype** | Reusable expertise and tool boundaries |

Changing a mode does not change the Bot. A Bot can mentor in Teacher mode,
collaborate in General, or work in Coder while keeping the same relationship.

## Create and open a Bot

1. Open **Sessions** from Chat.
2. Under **Bots**, choose **+** or **Create a Bot**.
3. Enter a name and purpose. Tap the avatar to choose from six expressions for each companion—Medousa, Seahorse, and Starfish—or a colored Medousa mark.
4. Choose an **Archetype**, or select **Create archetype…** to define reusable expertise and an optional approach. Your Bot draft is preserved while you do this.
5. Choose **Runs with**: Medousa, Codex, Cursor, or Hermes. For an external runtime, choose its workshop and a provisioned project.
6. Choose **Create Bot**.

Medousa opens the Bot's primary conversation. Opening that Bot later—from the
same app, your phone, or another Medousa client connected to the workshop—opens
the same conversation and Bot memory.

The conversation header shows your Bot’s companion and name. Select it to view
the Bot’s purpose and archetype, or choose **Edit Bot**. The composer addresses
your Bot by name. Its runtime stays attached to the Bot; the model, mode, and
behavior controls remain available where supported.

Bot creation opens in a centered dialog on desktop and a bottom sheet on mobile.
**More options** contains browser continuity when a persistent browser is available.

Archetypes use the same reusable definitions called Specialists elsewhere in Medousa.
Creating one here makes it available to other Bots and the existing specialist editor.
Existing emoji avatars are preserved until you choose another avatar.

## Find and create from Spotlight

Open Spotlight and search a Bot’s name, purpose, or runtime. Select
it to open its primary conversation. Connected Muse, Grok Bot, Instinct, and
Dots conversations appear under **Connected agents** and open their attached
sessions.

In Spotlight’s **Create** view, choose **New Bot** or **Connect agent**. You can
also type `+ bot` or `+ agent`. These open the same setup dialogs as Sessions.

## Runtime Bots

Codex, Cursor, and Hermes Bots keep their runtime, workshop, and project in
their profile. Their composer accepts text and has no runtime, model, or mode
picker. The runtime must be installed and signed in on the selected workshop.
The project must already have a provisioned workspace; create one in Code if
none are available.

Each request uses a fresh runtime process with the Bot’s purpose and bounded
conversation context. Accepted requests keep running when you leave the chat.
Long coordinator chats share a recent slice that fits the handoff budget.
Remote coding assignments can also carry existing Locus summaries from the
coordinator's memory scope. Include the task, constraints, and acceptance
criteria in the assignment so the worker has a clear brief even when older
history is omitted.
The status line lets you cancel a job or retry an uncertain submission using
the original request ID. Responses are saved to the Bot conversation on the
workshop. Execution administration is required to configure these bindings.

For native coding handoffs across paired workshops with owner continuation,
the desktop daemon retrieves the remote owner's final answer into the original
coordinator chat. Retrieval continues while Medousa's window is closed and
resumes after daemon restarts. An open window refreshes the answer automatically;
offline workshops are retried without submitting the assignment again.

## Message a Bot or another agent chat

Ask your assistant, for example: “Message Taco and tell it the API review is
ready,” or “Ask the backend coder which tests are still failing.” Native Bots
with an open primary conversation and existing native agent chats on the current
or a paired workshop can receive these messages. A Bot's name or a chat's title identifies
it; if names collide, the assistant needs the exact Bot or chat ID.

Messages identify the sending chat and carry only the supplied text. They do
not copy the sender's entire conversation. A busy recipient can read its inbox
while working; otherwise the message waits and wakes an Assistant turn when the
chat is free. Native Bots retain their profile, archetype, and memory. The
recipient can reply through the saved return route, waking the originating chat
when it is free. Replies are explicit: delivery alone does not mean a reply has
been sent.

Messages and delivery states survive daemon restarts. Retrying the same message
with the same request key does not send it twice. If a restart interrupts an
already admitted delivery and its outcome cannot be established, the assistant
reports an uncertain delivery instead of silently running it again.

A message is another agent's communication, not a human instruction or an
execution approval. Sending one does not create a Forge assignment or grant new
permissions. Message wakes use the normal Assistant loop to continue authorized
work; continuing Coder work still needs its normal scoped admission.

When your assistant first sends a message during a user turn, the runtime
retains permission for those two chats to communicate in either direction.
That permission survives daemon restarts, and existing admitted exchanges remain
usable after updating. A peer message cannot authorize contact with an unrelated
chat. Visibility, Bot archival, and workshop pairing rules still apply.

There is no per-turn send allowance or lifetime exchange limit. Recipients reply
when they have a useful answer, question, result, or update, and finish silently
when there is nothing to add. A requested confirmation is still a useful reply;
ordinary delivery receipts do not require another conversational acknowledgment.
Payload and queue capacity protections still apply. For cross-workshop
conversations, update both engines to remove the older hop limit.

For example, ask “Message Taco on Studio and ask how the review is going.” The
assistant can discover connected workshops, then find native Bots and chats on
the selected workshop. Repeated workshop names need an exact workshop ID.
Starting a remote exchange requires the source workshop owner's profile and an
active pairing. On the destination, enable Assistant work, agent targeting, and
host shell in the paired workshop's execution policy, or use a full Portal pairing.

Replies retain both workshop and chat identities. A one-way workshop connection
works too: the originating workshop checks a durable reply mailbox, so the
recipient does not need a connection back. Both daemons must be running and the
pairing must remain valid for delivery. Offline connections keep messages queued;
revoked or denied connections report delivery errors.

Codex, Cursor, and Hermes Bots use their existing job/handoff route. Chat messages
coordinate native agents; scoped Coder assignments still use the handoff flow.

## Edit, duplicate, and archive

Select the Bot’s name in the conversation header to view its profile and edit it.
You can also use the quiet actions on a Bot row in Sessions:

- **Edit** changes its name, purpose, avatar, or archetype. Existing transcript
  history is not rewritten.
- **Duplicate** copies the setup into a new Bot with a fresh conversation and
  fresh memory. It does not copy learned memory or transcript history. External executor bindings are cleared; configure the duplicate’s runtime and project explicitly.
- **Archive** removes the Bot from the active list without deleting its
  conversation or memory. Expand **Archived Bots** to restore it.

Ordinary chats stay ordinary. Selecting a Specialist for a one-off chat still
works as before and does not create a Bot.

## Trust boundary

A Bot is context, not authority. Its profile and memory cannot grant shell,
tools, secrets, or workshop permissions. Every turn still passes through the
selected mode, Specialist policy, and workshop admission rules.

## Ask an enrolled external-agent Bot from the CLI

Prepare a Forge project on the Bot's home workshop. These commands talk to that
workshop's local daemon, so run them there. A blank project gets a new Git
repository; `--repo-path` uses an existing repository path on that daemon.
The returned `work_id` and raw `repo_id` are the IDs used for Bot enrollment
and project-scoped mesh policy.

```sh
medousa project create --title "App fixes" --brief "Fix and test the app"
medousa project create --title "Existing app" --brief "Maintain the app" --repo-path /path/on/workshop/to/repo
medousa project list
medousa project inspect FORGE_WORK_ID
```

Create the named Bot in Medousa, then enroll its external executor from the
operator CLI. The executor pins `runtime` (`codex`, `cursor`, or `hermes`), `home_workshop_id`, a provisioned
`forge_work_id`, `forge_repo_id`, and `session_contract: "fresh_per_job"`.
Use the workshop ID shown by `medousa pair targets`; the pinned Forge work and
repository must exist on that destination. Editing this binding requires
execution administration. The Bot's name and purpose are still managed with
the normal Bot profile. ACP manages the selected runtime’s own tool permissions.

```sh
medousa bot list
medousa bot enroll codex-mini --runtime codex --workshop-id MINI_WORKSHOP_ID --work-id FORGE_WORK_ID --repo-id FORGE_REPO_ID
medousa ask "fix the failing test" --bot codex-mini
medousa ask "fix the failing test" --agent codex --workshop "mac mini"
# --runtime and --agent also accept cursor or hermes.
medousa ask "fix the failing test" --bot codex-mini --request-id incident-42 --detach
medousa ask --resume delegation-job-EXAMPLE
medousa ask --cancel delegation-job-EXAMPLE
```

`--bot` resolves exactly one active Bot in the authenticated profile. The
`--agent` form searches that same registry and reports candidates when more
than one Bot matches. A workshop name must resolve to one eligible destination;
it cannot move a Bot away from its pinned home. `ask` prints a durable request
ID before submission and a job handle after admission. Retry an uncertain
submission with the same `--request-id`; it will observe the original job.
Closing the CLI leaves accepted work running. `--resume` reconnects to status
and the terminal result. `--cancel` explicitly stops that job.

For a remote Bot, the source and destination daemons must be paired through
Medousa mesh, the destination policy must allow Assistant and Coder work for
the selected Forge project, and the selected runtime CLI must be installed and signed in on
the destination. The destination checks the pinned repository and governed
workdir before starting ACP. Portal and HTTPS bridges are separate paths.

## Related

- [Models and agent sources](models-and-agent-sources.md)
- [Memory & identity](memory-and-identity.md)
- [Workshop & Automations](workshop-and-automations.md)
