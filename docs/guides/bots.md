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
3. Enter a name and purpose. Tap the avatar to choose Medousa, Seahorse, Starfish, or a colored Medousa mark.
4. Choose an **Archetype**, or select **Create archetype…** to define reusable expertise and an optional approach. Your Bot draft is preserved while you do this.
5. Choose **Create Bot**.

Medousa opens the Bot's primary conversation. Opening that Bot later—from the
same app, your phone, or another Medousa client connected to the workshop—opens
the same conversation and Bot memory.

The Bot chip under the composer tells you when the conversation belongs to a
Bot. The mode control remains separate.

Bot creation opens in a centered dialog on desktop and a bottom sheet on mobile.
**More options** contains browser continuity when a persistent browser is available.

Archetypes use the same reusable definitions called Specialists elsewhere in Medousa.
Creating one here makes it available to other Bots and the existing specialist editor.
Existing emoji avatars are preserved until you choose another avatar.

## Edit, duplicate, and archive

Use the quiet actions on a Bot row in Sessions:

- **Edit** changes its name, purpose, avatar, or archetype. Existing transcript
  history is not rewritten.
- **Duplicate** copies the setup into a new Bot with a fresh conversation and
  fresh memory. It does not copy learned memory or transcript history.
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
operator CLI. The executor pins `runtime: "codex"`, `home_workshop_id`, a provisioned
`forge_work_id`, `forge_repo_id`, and `session_contract: "fresh_per_job"`.
Use the workshop ID shown by `medousa pair targets`; the pinned Forge work and
repository must exist on that destination. Editing this binding requires
execution administration. The Bot's name and purpose are still managed with
the normal Bot profile. ACP manages Codex's own tool permissions.

```sh
medousa bot list
medousa bot enroll codex-mini --workshop-id MINI_WORKSHOP_ID --work-id FORGE_WORK_ID --repo-id FORGE_REPO_ID
medousa ask "fix the failing test" --bot codex-mini
medousa ask "fix the failing test" --agent codex --workshop "mac mini"
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
the selected Forge project, and Codex CLI must be installed and signed in on
the destination. The destination checks the pinned repository and governed
workdir before starting ACP. Portal and HTTPS bridges are separate paths.

## Related

- [Models and agent sources](models-and-agent-sources.md)
- [Memory & identity](memory-and-identity.md)
- [Workshop & Automations](workshop-and-automations.md)
