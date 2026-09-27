# Agent conversation bridges: Muse and Grok Bot

Status: prototype implemented on this branch; live provider gates remain open.
Date: 2026-09-27.
Branch: `codex/agent-conversation-bridges`, based on `medousa/low-user-friction` at `4ee79366`.

## Product decision

Medousa presents persistent external-agent conversations in Home. The connected
workshop daemon owns transport, credentials, durable messages, and task state.
Home, including remote and mobile Home, never holds a WhatsApp session or Grok
Bot webhook key. The agents run on provider-owned computers; their computers do
not become Medousa workshops. Workshop context and tools require a separate,
authenticated, scoped Medousa CLI/MCP connection admitted by the relevant
workshop daemon.

| Agent | Medousa-to-agent path | Agent-to-Medousa path |
| --- | --- | --- |
| Muse | Linked WhatsApp session sends to the user's Muse chat | Session receives Muse messages; optional CLI/MCP events from its VM if supported |
| Grok Bot | Daemon POSTs to a webhook routine owned by the selected Bot | Bot reports progress, questions, and terminal result through authenticated Medousa CLI/MCP calls |

The same Home conversation UI presents both. A WhatsApp send acknowledgment or
Grok Bot webhook `200` means transport acceptance, not task completion.
Provider-native chat, run history, approvals, and computer views remain
authoritative for provider-only actions. Home shows only events it actually
receives, with verified Medousa tool receipts distinguished from provider
reports.

## Existing systems to extend

- `adapters/medousa-whatsapp/src/main.rs` already pairs as a linked device,
  sends to a JID, and receives messages. Its current inbound path calls
  `/v1/ingest` as if the sender were a person talking to Medousa. A configured
  Muse chat must be routed before that path; other chats keep current behavior.
- `apps/medousa-home/src/lib/utils/sessionAgentRuntime.ts` and
  `ChatRuntimePicker.svelte` select Medousa or ACP runtimes. The daemon's
  `agents` service and `crates/medousa-acp-client` create local ACP sessions
  for Cursor, Codex, and Hermes. Muse and Grok Bot must not be sent through an
  ACP spawn command merely to appear in that picker.
- `crates/medousa-types/src/coordination.rs` and
  `src/daemon/coordination.rs` provide exact external-peer assignment, context
  grant, and terminal-receipt rules. Reuse those for bridge-backed delegated
  work. Ordinary user-to-agent chat needs its own persistent lifecycle; do
  not create a fake Forge work item for every message.
- Workshop and SDK transport remain location-neutral. Filesystem and vault
  authority stays on the workshop daemon even when Home is remote.

## Daemon-owned contract

Persist an external conversation keyed by workshop authority, owner principal,
provider, and exact provider conversation binding. Expose opaque Medousa IDs to
Home; retain provider binding and credentials on the daemon. A record carries
provider target, visibility scope, optional governed Forge work binding,
capabilities, readiness, and health. One Home chat may bind to one provider
conversation. Changing provider must not relabel its earlier transcript.

Commit an append-only event sequence:

1. User message with stable request ID, before network send.
2. Transport attempt, acceptance, rejection, or uncertain outcome.
3. Provider message or callback with provider event/run ID when known.
4. Progress, question, approval-needed signal, or terminal receipt when
   explicitly reported and correlated.

Home replays committed events after restart or reconnect without duplicate
bubbles. Never infer completion from silence, a webhook `200`, a WhatsApp send
acknowledgment, or an ordinary provider message. An ambiguous external send
stays uncertain for reconciliation; automatically retrying it could launch
duplicate work.

Use a transport-neutral port for discovery, binding, send, inbound events,
health, and capability reporting. Text, attachments, progress, terminal
callback, cancellation, and approvals are independent capabilities. Do not
render a cancel or approval control until the provider-specific action is
verified. A stop-request message, if supported, must be labeled as such.

### Correlation and ordering

- Generate the Medousa request ID at send admission. Include it in Grok Bot
  webhook JSON and callback instructions. Keep WhatsApp provider message IDs
  and reply references where available. An unlinked Muse reply belongs to the
  bound conversation but is shown as uncorrelated if several requests are open.
- Deduplicate with provider event/message ID, direction, and account binding;
  deduplicate callbacks with request/event IDs. Keep provider timestamps for
  display but order by the daemon journal sequence.
- Initially serialize sends per provider conversation. Overlapping work needs
  proven provider-side request correlation.
- Treat all provider text and webhook bodies as untrusted data, never a grant.

## Muse transport

First prove a real send/reply through the existing linked-device adapter with
an opt-in Muse account. Pair on the workshop that owns the session. Bind only
an explicitly selected and verified Muse JID. Inbound routing checks both the
paired account and exact chat JID before recording an external-agent message.
Ignore `is_from_me` echoes as replies while retaining outbound send IDs.

The linked-device client in this repo is unofficial and already warns about
production use. The WhatsApp Business Cloud API sends as a registered business
number; it does not impersonate the user's personal WhatsApp identity. Release
readiness therefore requires live send/reply, reconnect/re-pair, account
isolation, and an explicit product decision about linked-device distribution.
If the route is unavailable, show Muse as unavailable rather than silently
sending from another identity.

## Grok Bot transport

The user creates or selects a webhook-triggered routine for the exact Bot and
enters its POST URL and sender key through daemon-backed setup. Store the key
in the workshop credential store; never put it in Home local storage, chat,
logs, or the webhook body. POST bounded JSON with a schema version, request
ID, conversation ID, task text, and an expiring reference to separately
authorized context. Do not put a vault dump or permanent Medousa credential
in the payload.

The routine treats the body as untrusted task data and reports progress and
terminal state to Medousa CLI/MCP using the request ID. Its Medousa credential
is distinct from the webhook sender key, scoped to agent/conversation/tools,
and revocable. The daemon authenticates each callback, checks its binding,
and deduplicates events. A `200` starts a Grok Bot run; it does not finish it.
Without a callback, status remains accepted/running/unknown until reconciled.

Prove that Grok Bot's cloud computer can reach the workshop through an
existing authenticated remote path or deliberately configured private route.
Do not expose an unauthenticated daemon endpoint to make callbacks work.
The setup test must prove both trigger and return paths.

## Workshop access from provider VMs

Offer the smallest CLI/MCP surface needed for the bound task. Grant context by
explicit manifest or selected Forge work item under existing visibility,
execution, and approval checks. Provider account or WhatsApp identity alone
does not grant workshop access. The daemon owns credential issuance,
revocation, expiry, replay checks, and filesystem/vault decisions. A provider
VM reads or edits workshop data only through admitted daemon tools.

## Home behavior

List Muse and Grok Bot with daemon-reported `ready`, `needs setup`,
`disconnected`, or `unavailable` states. Setup belongs under Connection /
external agents and Settings → Packages where an optional adapter is needed.
Medousa's home-first chat works before either is configured. The transcript
shows pending, accepted, reply/progress, needs input, and terminal states from
durable events. For provider-only approvals, link to the provider's approval
surface until a verified submit API exists. Remote Home neither pairs
WhatsApp locally nor reveals the session DB or webhook secret. Media is a
later capability unless identity, size limits, storage, and display are proven
end to end.

## Delivery slices and exit evidence

| Slice | Implementation | Exit evidence |
| --- | --- | --- |
| 0. Contract | Persistent conversation/events, transport port, ownership, API/SDK schema | Restart/replay, duplicate-event, owner isolation, and uncertain-send tests |
| 1. Grok Bot prototype | Credential setup, webhook POST, scoped CLI/MCP callback, exact Bot binding | Real routine starts; callback progress/result appears in Home after reopen; `200` alone never completes it |
| 2. Muse prototype | WhatsApp outbound binding and inbound demultiplexing | Real Muse send/reply, wrong-chat rejection, echo handling, re-pair, and restart replay |
| 3. Home integration | Readiness/setup, selection, transcript, status, provider handoff | Local and remote Home show the same conversation; unavailable targets cannot appear ready |
| 4. Delegation integration | Bridge-backed assignments under existing context/grant/receipt rules | Exact owner, context, target, approval, dedupe, and terminal receipt verified; no fake ACP process or Forge work |
| 5. Release gate | Supported setup and limits in user/integrator docs | Live end-to-end acceptance per enabled provider, CI parity, `docs/guides/` and `docs/README.md` updated |

Keep each provider behind a capability/readiness flag until its live gate
passes. An outbound transport proof does not establish VM access or a complete
provider transcript.

### Prototype checkpoint (2026-09-27)

- Implemented: daemon-owned conversation journal, owner-scoped API and SDK
  contract, Grok Bot webhook send and authenticated event callback, Muse
  WhatsApp inbound routing, and a Medousa conversation panel. Credentials are
  stored on the workshop; the UI receives a callback key only at setup or
  rotation.
- Still required for release: live Muse send/reply and reconnect on the linked
  account; real Grok Bot routine start and VM callback; exact webhook host
  validation; workshop reachability from each provider VM; provider readiness
  and account checks; scoped VM credentials and governed delegation receipts.
  The current panel is an opt-in prototype until those checks pass.

## Provider facts to verify during prototypes

1. Muse: exact chat identity, linked-device reliability, and VM access to
   Medousa CLI/MCP through an approved network path.
2. Grok Bot: routine URL/key setup, duplicate webhook behavior, callback
   reachability, and whether routine output appears in native Bot chat.
3. Both: provider approval/cancel mechanisms, message/media limits, and
   authoritative terminal-outcome signals.

## Provider references checked for this plan

- Meta, [Introducing Muse](https://about.fb.com/news/2026/09/introducing-muse-personal-ai-agent/) (2026-09-08): WhatsApp access and Muse Secure VM.
- Cursor, [Grok Bot routines](https://prod.cursor.com/help/grok-bot/routines): webhook URL/key, JSON POST, and `200` as run-start acknowledgment.
- SpaceXAI, [Grok Bot overview](https://docs.x.ai/grok-bot/overview): persistent cloud computer and conversation model.
- Meta, [WhatsApp Business Platform messages](https://www.postman.com/meta/whatsapp-business-platform/folder/o48mro7/messages): registered business-number send API and message-status webhooks.
