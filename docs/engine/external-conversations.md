# External agent conversations

The workshop daemon owns persistent conversations with provider-hosted agents. Medousa sends Muse messages through its local WhatsApp adapter and Grok Bot requests through a webhook routine. These are separate from local ACP sessions.

| Route | Capability | Purpose |
| --- | --- | --- |
| `GET /v1/external-conversations` | `workshop.read` | List the principal's conversations and events |
| `POST /v1/external-conversations` | `admin.execute` | Create a Muse or Grok Bot binding |
| `POST /v1/external-conversations/muse/discovery` | `admin.execute` | Start a five-minute Muse chat challenge |
| `GET /v1/external-conversations/muse/discovery` | `workshop.read` | Poll for the linked adapter's observed chat ID |
| `GET /v1/external-conversations/{id}` | `workshop.read` | Replay one conversation |
| `DELETE /v1/external-conversations/{id}` | `admin.execute` | Remove a conversation and revoke its stored keys |
| `POST /v1/external-conversations/{id}/callback-key/rotate` | `admin.execute` | Replace the Grok Bot callback key and show the new value once |
| `POST /v1/external-conversations/{id}/messages` | `workshop.interact` | Commit and send a user message |
| `POST /v1/external-conversations/{id}/events` | `workshop.interact` plus callback key | Ingest a Grok Bot VM event |
| `POST /v1/external-conversations/whatsapp/inbound` | `workshop.interact`, loopback | Route an adapter inbound message |

Create body: `provider` (`muse` or `grok_bot`), `label`, `target`, and for Grok Bot `webhook_url` and `webhook_key`. A Muse `target` must match the chat ID observed during a live discovery challenge; clients cannot guess or enter a phone number. Grok Bot creation returns one `callback_key`; store it in the provider VM's secret store. Grok Bot URLs must use HTTPS under `cursor.com` or `cursor.sh`. Credentials are kept in the workshop credential store, outside the conversation journal.

The send body is `{ "request_id": "stable-id", "text": "..." }`. Keep the request ID stable while reconciling an uncertain send. Reusing it returns a conflict. The daemon commits the user message and `transport_pending` attempt before network I/O. A successful response contains the durable conversation and final transport status. `transport_accepted` does not mean the provider completed the task. A pending attempt after daemon restart or `transport_uncertain` means the request may have reached the provider; inspect its native history before submitting another request.

Grok Bot callback body is `{ "event_id": "provider-event-id", "request_id": "original-request-id", "kind": "progress|question|provider_message|completed|failed", "text": "..." }`. Call with a paired workshop bearer and the `x-medousa-bridge-key` header. Every callback requires a known request ID. Repeated event IDs are deduplicated. The Bot VM can use `medousa-cli daemon-external-event` with `MEDOUSA_BRIDGE_BEARER` and `MEDOUSA_BRIDGE_KEY` set privately.

The WhatsApp adapter's local inbound route can claim an exact one-time Muse challenge before normal Medousa channel ingest. The observed individual chat ID is held only for the five-minute setup window. After binding, the route claims only that chat ID; other chats continue through normal ingest. Meta's public Muse material does not specify whether its managed WhatsApp chat appears to linked-device adapters, so a successful live discovery and outbound send/reply are release gates. The provider VM receives no filesystem authority from its WhatsApp identity; it needs separately authenticated Medousa CLI/MCP access and workshop grants.
