# External agent conversations

The workshop daemon owns persistent conversations with provider-hosted agents. Medousa sends Muse and Instinct messages through its local WhatsApp adapter, Dots messages through Slack, and Grok Bot requests through a webhook routine. These are separate from local ACP sessions.

| Route | Capability | Purpose |
| --- | --- | --- |
| `GET /v1/external-conversations` | `workshop.read` | List the principal's conversations and events |
| `POST /v1/external-conversations` | `admin.execute` | Create a Muse, Grok Bot, Instinct, or Dots binding |
| `POST /v1/external-conversations/muse/discovery` | `admin.execute` | Start a five-minute Muse chat challenge |
| `GET /v1/external-conversations/muse/discovery` | `workshop.read` | Poll for the linked adapter's observed chat ID |
| `GET /v1/external-conversations/whatsapp/pairing` | `admin.execute` | Read the current short-lived WhatsApp pairing QR for display in Medousa |
| `POST /v1/external-conversations/whatsapp/pairing` | `workshop.interact`, loopback | Let the local adapter publish QR, connected, or logged-out state |
| `GET /v1/external-conversations/{id}` | `workshop.read` | Replay one conversation |
| `DELETE /v1/external-conversations/{id}` | `admin.execute` | Remove a conversation and revoke its stored keys |
| `POST /v1/external-conversations/{id}/callback-key/rotate` | `admin.execute` | Replace the Grok Bot callback key and show the new value once |
| `POST /v1/external-conversations/{id}/api-token` | `admin.execute` | Issue or replace a provider API token and show it once |
| `DELETE /v1/external-conversations/{id}/api-token` | `admin.execute` | Revoke a provider API token |
| `POST /v1/external-conversations/{id}/messages` | `workshop.interact` | Commit and send a user message |
| `POST /v1/external-conversations/{id}/events` | `workshop.interact` plus callback key | Ingest a Grok Bot VM event |
| `POST /v1/external-conversations/whatsapp/inbound` | `workshop.interact`, loopback | Route an inbound message or claim an outgoing Muse discovery code |
| `POST /v1/external-conversations/slack/inbound` | `workshop.interact`, loopback | Reserve a dot channel and record replies from its bound dot |

Create body: `provider` (`muse`, `grok_bot`, `instinct`, or `dots`), `label`, `target`, and provider-specific fields. A Muse `target` must match the chat ID observed during a live discovery challenge; clients cannot guess or enter a phone number. An Instinct `target` is its WhatsApp phone number, including `+` and the country code. Dots requires a Slack channel ID as `target`, the dot's Slack member ID as `dot_user_id`, and its owner's `xoxp-` Slack user token with `chat:write` as `slack_user_token`. One channel can bind one dot. The token is kept in the workshop credential store, outside the conversation journal. Grok Bot requires `webhook_url` and `webhook_key` and returns one `callback_key`; store it in the provider VM's secret store. Grok Bot URLs must use HTTPS under `cursor.com` or `cursor.sh`.

The send body is `{ "request_id": "stable-id", "text": "..." }`. Keep the request ID stable while reconciling an uncertain send. Reusing it returns a conflict. The daemon commits the user message and `transport_pending` attempt before network I/O. A successful response contains the durable conversation and final transport status. `transport_accepted` does not mean the provider completed the task. A pending attempt after daemon restart or `transport_uncertain` means the request may have reached the provider; inspect its native history before submitting another request.

Grok Bot callback body is `{ "event_id": "provider-event-id", "request_id": "original-request-id", "kind": "progress|question|provider_message|completed|failed", "text": "..." }`. A provider reaction can accompany the event with `"reaction": { "effect_id": "provider-effect-id", "target": "current_user_message", "emoji": "❤️" }`; reaction-only events may use an empty `text`. Call with a paired workshop bearer and the `x-medousa-bridge-key` header. Every callback requires a known request ID. Repeated event IDs are deduplicated. The Bot VM can use `medousa-cli daemon-external-event` with `MEDOUSA_BRIDGE_BEARER` and `MEDOUSA_BRIDGE_KEY` set privately.

For callbacks over Iroh, pass `--iroh-ticket <ticket>` or set `MEDOUSA_BRIDGE_IROH_TICKET`. Alternatively, `--worker <id-or-unique-label>` reads both the ticket and bearer from the matching `medousa pair join` record in `delegation/workers.json`; `MEDOUSA_BRIDGE_KEY` remains required. Worker selection ignores environment ticket/bearer overrides and rejects records without a ticket. These routes use the `medousa-http/1` tunnel with the same authenticated HTTP request and never probe or fall back to the saved LAN URL. Iroh support requires the `iroh-transport` build feature. Without a ticket or worker, the CLI retains HTTP delivery through `--daemon-url`. There are no automatic retries, redirects, or bearer renewals. Reuse the event ID when retrying an uncertain delivery. See the [provider conversation guide](../guides/provider-conversations.md) for commands.

The WhatsApp adapter routes an exact one-time Muse code sent by the user from the phone before ignoring other outgoing messages. The same route can claim the code from an incoming message before normal Medousa channel ingest. The observed individual chat ID is held only for the five-minute setup window. After binding, the route claims only inbound messages from that chat; other chats continue through normal ingest. Discovery proves the linked adapter saw the chat ID, not that Muse receives linked-device messages or forwards replies to linked devices. In a live test, a WhatsApp-accepted send appeared on the phone but not in Muse's app, and Muse's replies to phone messages did not reach the adapter. Treat this transport as unverified until a send appears in Muse's app and a reply reaches Medousa. The provider VM receives no filesystem authority from its WhatsApp identity; it needs separately authenticated Medousa CLI/MCP access and workshop grants.

The adapter posts a pairing QR payload to the daemon over loopback with its workshop credential. The daemon renders SVG in memory, returns it only to an `admin.execute` client, and stops returning it when its WhatsApp expiry passes or the adapter reports a new state. The QR is never written to the conversation journal. Home polls this status while Muse setup is open, so remote Home clients can scan the workshop's QR from the desktop app. Agent reactions observed on the linked WhatsApp account are recorded as typed provider events and appear in Home without being converted into assistant prose.

## Provider API credentials

Instinct binds an international `+` phone number, normalized to a WhatsApp PN
JID. It uses the WhatsApp adapter for sends and replies. A phone chat can belong
to only one external conversation, including across owners/providers. Dots uses
a dedicated Slack channel. Medousa posts as the dot's owner with the saved user
token and mentions the selected dot. The Slack adapter reserves that channel
from normal channel ingest and records messages only from the selected dot
member or bot ID.

`POST /v1/external-conversations/{id}/api-token` requires `admin.execute` and
conversation ownership for Muse, Instinct, Dots, and Grok Bot. Body: `{ "scopes": ["read", "work"], "expires_in_days": 30 }`.
It returns `{ "token": "...", "access": { "scopes": [...], "expires_at": "..." } }`
with `Cache-Control: no-store`. Expiry must be 1–90 days. Issuing a replacement
invalidates the old token. `DELETE` on the same route revokes access and returns
the conversation. Lists contain access metadata only; the daemon persists a
SHA-256 verifier of a random 256-bit credential, never its plaintext.

Bearer authentication produces an external-agent principal bound to the
conversation owner's profile. Before route capability checks, the access
boundary restricts these credentials to exact method/registered-route pairs:

| Scope | Methods and routes |
| --- | --- |
| `read` | `GET /v1/capabilities`, `GET /v1/capabilities/{capability_id}` |
| `read` | `GET /v1/vault/notes`, `GET /v1/vault/notes/{*note_path}`, `GET /v1/vault/search`, `GET /v1/vault/tags` |
| `read` | `GET /v1/calendar/events` |
| `work` | `POST /v1/jobs/ask`, `GET /v1/jobs/{job_id}/result`, `GET /v1/jobs/{job_id}/report` |
| `read` or `work` | `POST /v1/work/query` |
| `work` | `POST /v1/work/mutate` |

All other routes fail closed, including on loopback. Scopes authorize workshop
resources, not individual notes or projects. Background work receives member
capabilities and remains governed by existing workshop admission. Token
revocation blocks new HTTP requests but does not cancel admitted jobs.
See the [Instinct guide](../guides/instinct-agent.md) and [Dots guide](../guides/dots.md) for HTTPS requirements and command examples.

## Work participant adapters

`POST /v1/work/query` and `POST /v1/work/mutate` expose the same owner-domain
intent and native execution contracts as the runtime tools. They require a
credential with a bound profile; local operator credentials without a profile
cannot select a domain through these bodies. Requests freeze the authenticated
principal before admission and never construct a synthetic chat turn.

| Endpoint | Action | Fields |
| --- | --- | --- |
| query | `work.graph` | `query`: existing bounded `WorkGraphQuery` |
| query | `work.get` | `work_unit_id` |
| query | `work.events` | `query`: exact actor-owned subscription and bounded limit |
| query | `work.coordination` | `query`: exact `channel` and `coordination_id` |
| query | `peer.discover` | `session_id`: owned source chat; requires `work` scope |
| mutate | `work.record` | `command`: existing idempotent `WorkGraphCommand` |
| mutate | `peer.propose` | `session_id` and `intent`: existing native proposal fields |
| mutate | `work.coordinate` | `input`: existing bounded native coordination fields |

Bodies reject unknown fields, including caller-supplied owner, grant, or
provenance fields. Responses wrap the bounded native result as `{ "result": ... }`.
For example, `{ "action": "work.graph", "query": { "collection": "work_units" } }`
reads accepted work without an active Medousa chat. Native coordination inspection
still checks channel ownership, Forge ownership, and source visibility.

`work.record` uses optimistic graph revision and exact command replay, and
records the authenticated credential ID as model-inferred provenance. It cannot
publish native availability/revisions, reserve or settle execution budgets, or
satisfy registered executor/reviewer work through a model state claim. Source
conversation references still require native visibility.

`peer.propose` requires an exact visible source chat, owned Forge work on this
workshop, bounded committed transcript ranges, and `continue_owner: false`.
An explicit `forge_work_id` can select that work from an unrelated source chat;
omitting it uses the existing chat's project binding.
Discovery does not grant execution authority. Native proposals remain immutable
and require the existing operator approval; provider credentials cannot approve,
dispatch, mint grants, send messages through another provider, or run arbitrary
runtime mutations. After both proposals are registered, the durable controller
advances approved stages without requiring the source chat to remain active.

API access is independent of chat transport verification. WhatsApp/Slack messages
without an exact admitted work association remain conversation events. These
adapters do not make Muse/Instinct/Dots/Grok Bot messages native review receipts,
automatically deliver follow-ups, or establish work federation. Muse's linked-
device transport remains unverified as described above. Revocation blocks new
participant requests; it does not cancel already admitted work or its native grants.
