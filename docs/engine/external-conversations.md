# External agent conversations

The workshop daemon owns persistent conversations with provider-hosted agents. Medousa sends Muse messages through its local WhatsApp adapter and Grok Bot requests through a webhook routine. These are separate from local ACP sessions.

| Route | Capability | Purpose |
| --- | --- | --- |
| `GET /v1/external-conversations` | `workshop.read` | List the principal's conversations and events |
| `POST /v1/external-conversations` | `admin.execute` | Create a Muse or Grok Bot binding |
| `POST /v1/external-conversations/muse/discovery` | `admin.execute` | Start a five-minute Muse chat challenge |
| `GET /v1/external-conversations/muse/discovery` | `workshop.read` | Poll for the linked adapter's observed chat ID |
| `GET /v1/external-conversations/whatsapp/pairing` | `admin.execute` | Read the current short-lived WhatsApp pairing QR for display in Medousa |
| `POST /v1/external-conversations/whatsapp/pairing` | `workshop.interact`, loopback | Let the local adapter publish QR, connected, or logged-out state |
| `GET /v1/external-conversations/{id}` | `workshop.read` | Replay one conversation |
| `DELETE /v1/external-conversations/{id}` | `admin.execute` | Remove a conversation and revoke its stored keys |
| `POST /v1/external-conversations/{id}/callback-key/rotate` | `admin.execute` | Replace the Grok Bot callback key and show the new value once |
| `POST /v1/external-conversations/{id}/messages` | `workshop.interact` | Commit and send a user message |
| `POST /v1/external-conversations/{id}/events` | `workshop.interact` plus callback key | Ingest a Grok Bot VM event |
| `POST /v1/external-conversations/whatsapp/inbound` | `workshop.interact`, loopback | Route an inbound message or claim an outgoing Muse discovery code |

Create body: `provider` (`muse` or `grok_bot`), `label`, `target`, and for Grok Bot `webhook_url` and `webhook_key`. A Muse `target` must match the chat ID observed during a live discovery challenge; clients cannot guess or enter a phone number. Grok Bot creation returns one `callback_key`; store it in the provider VM's secret store. Grok Bot URLs must use HTTPS under `cursor.com` or `cursor.sh`. Credentials are kept in the workshop credential store, outside the conversation journal.

The send body is `{ "request_id": "stable-id", "text": "..." }`. Keep the request ID stable while reconciling an uncertain send. Reusing it returns a conflict. The daemon commits the user message and `transport_pending` attempt before network I/O. A successful response contains the durable conversation and final transport status. `transport_accepted` does not mean the provider completed the task. A pending attempt after daemon restart or `transport_uncertain` means the request may have reached the provider; inspect its native history before submitting another request.

Grok Bot callback body is `{ "event_id": "provider-event-id", "request_id": "original-request-id", "kind": "progress|question|provider_message|completed|failed", "text": "..." }`. Call with a paired workshop bearer and the `x-medousa-bridge-key` header. Every callback requires a known request ID. Repeated event IDs are deduplicated. The Bot VM can use `medousa-cli daemon-external-event` with `MEDOUSA_BRIDGE_BEARER` and `MEDOUSA_BRIDGE_KEY` set privately.

The WhatsApp adapter routes an exact one-time Muse code sent by the user from the phone before ignoring other outgoing messages. The same route can claim the code from an incoming message before normal Medousa channel ingest. The observed individual chat ID is held only for the five-minute setup window. After binding, the route claims only inbound messages from that chat; other chats continue through normal ingest. Discovery proves the linked adapter saw the chat ID, not that Muse receives linked-device messages or forwards replies to linked devices. In a live test, a WhatsApp-accepted send appeared on the phone but not in Muse's app, and Muse's replies to phone messages did not reach the adapter. Treat this transport as unverified until a send appears in Muse's app and a reply reaches Medousa. The provider VM receives no filesystem authority from its WhatsApp identity; it needs separately authenticated Medousa CLI/MCP access and workshop grants.

The adapter posts a pairing QR payload to the daemon over loopback with its workshop credential. The daemon renders SVG in memory, returns it only to an `admin.execute` client, and stops returning it when its WhatsApp expiry passes or the adapter reports a new state. The QR is never written to the conversation journal. Home polls this status while Muse setup is open, so remote Home clients can scan the workshop's QR from the desktop app.
