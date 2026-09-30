# External agent conversations

Medousa can show conversations with agents that run on their own computers. Set up Grok bots in **Settings → External Agents**. Muse WhatsApp support is experimental: a registered chat ID does not establish that Muse receives Medousa messages. Conversation transcripts belong to the connected workshop, so the same history appears when you reconnect from another Medousa device.

For a WhatsApp phone-number agent that uses HTTPS API calls, see [Instinct Agent](instinct-agent.md). For a Slack-connected OpenAI dot, see [Dots](dots.md).

## Grok Bot

1. Create a webhook-triggered routine for the Bot you want to contact. Copy its POST URL and sender key.
2. In **Settings → External Agents → Grok Bot**, choose **Add bot** and enter a label, Bot name, URL, and key. Medousa stores the key in the workshop credential store.
3. Save the callback key shown once. The Bot VM needs a separately paired Medousa workshop credential and must report events to the conversation's `/events` route with `x-medousa-bridge-key`. The webhook key and callback key serve different directions. The VM can use `medousa-cli daemon-external-event` with `MEDOUSA_BRIDGE_BEARER` and `MEDOUSA_BRIDGE_KEY` set in its private environment.
4. In Chat, choose **Grok Bot** and the registered bot, then send a message. An accepted webhook means the routine started; the conversation shows a result only when a callback arrives.

The routine receives JSON with `schema_version`, `request_id`, `conversation_id`, and `message`. Keep the IDs from that request when reporting progress or the result. For a VM paired using `medousa pair join`, list its saved workshop connections with `medousa pair workers`, then select the connection's ID:

```sh
medousa-cli daemon-external-event "$CONVERSATION_ID" "$EVENT_ID" completed "$RESULT" \
  --request-id "$REQUEST_ID" --worker "$WORKER_ID"
```

Set `MEDOUSA_BRIDGE_KEY` privately on the VM. `--worker` reads the Iroh ticket and workshop bearer together from that VM's `delegation/workers.json`; it does not use the record's LAN URL or require a named portal connection. It also accepts a unique saved connection label, which you can check with `pair workers`.

If your routine already stores the ticket and bearer separately, keep `MEDOUSA_BRIDGE_BEARER` and `MEDOUSA_BRIDGE_KEY` in its private environment and use:

```sh
medousa-cli daemon-external-event "$CONVERSATION_ID" "$EVENT_ID" completed "$RESULT" \
  --request-id "$REQUEST_ID" --iroh-ticket "$IROH_TICKET"
```

Alternatively, set `MEDOUSA_BRIDGE_IROH_TICKET` privately and omit `--iroh-ticket`. An explicit ticket takes precedence over this environment variable. `--worker` always uses its own saved ticket and bearer. Both Iroh forms skip direct HTTP, even when a LAN URL is available, and fail without HTTP fallback. A CLI built without Iroh support reports an error; source builds need `cargo build --release --features iroh-transport --bin medousa_cli` (the resulting binary is named `medousa_cli`).

Without a ticket or `--worker`, the command uses HTTP and accepts `--daemon-url "$WORKSHOP_URL"`. Use a new event ID for each new report, but reuse the same ID when retrying an uncertain delivery so the workshop can deduplicate it. A 401 response means the paired workshop bearer needs renewal; callback delivery does not renew it automatically. A 403 means to check the pairing's portal permissions and the callback key.

If the callback key is lost or exposed, use **Rotate callback key** in External Agents and update the VM's private configuration. **Remove** deletes its transcript from this workshop and revokes its stored keys.

## Muse

1. Install the WhatsApp adapter on the **connected workshop** through **Settings → Packages**. In **Settings → Sharing → Channels**, configure WhatsApp with **Deliver bind** `127.0.0.1:7423`; older builds used the port reserved by Medousa desktop. On a local workshop, **Settings → External Agents → Muse → Add session** starts the adapter and shows its live QR. For a remote workshop, start the adapter on that workshop with `medousa whatsapp`. On your phone, open **WhatsApp → Settings → Linked Devices → Link a Device** and scan the QR from Medousa. Keep the adapter running while you use Muse. Pairing does not require Muse's phone number or JID.
2. In **Settings → External Agents → Muse**, choose **Add session → Find Muse chat**. Send the one-time code Medousa shows in the normal Muse chat on your phone. Use **Check for code** if the page has not updated. The linked adapter uses your outgoing code to discover the chat's internal ID; you do not need a phone number or a reply containing the code. **WhatsApp linked** only confirms that the adapter paired with your account; it does not confirm that the Muse chat reached the adapter.
3. When Medousa reports that it observed the chat ID, you can register the session. Registration records the WhatsApp chat ID only. A test message can appear in WhatsApp while failing to reach Muse's own integration, and Muse's reply can remain visible only on the phone. Do not rely on this route for tasks until a message sent from Medousa appears in Muse's app and its reply appears back in Medousa.

Muse uses an additional encrypted envelope beyond ordinary WhatsApp messages. The adapter obtains Muse's pairing secret from authenticated WhatsApp device sync, wraps outgoing text for Muse, and decrypts and extracts text from Muse's rich replies. It supports text conversations; images and other rich content are not imported. If the adapter reports that the Muse pairing secret is unavailable, keep your phone connected so linked-device sync can finish, and verify that Muse is connected in WhatsApp before retrying.

For streamed replies, Medousa waits for Muse's explicit final update and then
shows the complete text. Intermediate previews are not saved as completed
messages. A pause in generation does not finish the reply, and a final update
can still be received after the adapter reconnects. The phone does not need an
app update when the workshop's WhatsApp adapter is updated.

WhatsApp delivery or **Read** status still does not verify that Muse received a request: older adapter builds could produce a visible, read message without reaching Muse. Verify a round trip with a harmless test in your own session. Discovery expiring without observing your outgoing code means the adapter has not seen the chat. The linked-device WhatsApp adapter remains experimental. If a send is uncertain, check Muse's app before sending the same request again.

If the adapter connects in a terminal but Medousa keeps showing **Waiting for the workshop’s WhatsApp pairing QR**, check which `medousa_whatsapp` binary the launcher used. Older adapter builds can pair in the terminal but do not publish QR or connected status to the workshop. Update the adapter package and restart it; an already-paired session reconnects from its session database.

## On iPhone and Android

Open **Chat context** above the composer, then **Runtime** (above Mode). Choose
**Muse**, **Dots**, or **Grok Bot**, then choose the registered session or bot in that same
sheet. To change it later, open Chat context and tap the provider session name.
Sending and the transcript follow that selection; native Mode, Project,
and Workers controls return when you choose Medousa.

The list belongs to the currently connected workshop. The embedded Personal
workshop on your phone has its own list. To use a Muse session already running
on your Mac, select that Mac's workshop on the phone first. The Mac keeps its
WhatsApp adapter running; the installed phone app uses its bundled interface
and does not need a Vite server.

## What the status means

- **Sending** is a durable attempt. If it remains after a workshop restart, check the provider's native history before resending.
- **Transport accepted** means WhatsApp accepted a send or the Grok Bot webhook accepted a request. For Muse, WhatsApp acceptance does not confirm delivery to Muse. It does not mean the agent finished.
- **Transport uncertain** means the workshop could not prove whether the provider received the request. Check the provider's native chat or routine history before resending.
- **Provider message, progress, question, completed, failed** are events the workshop actually received. An ordinary Muse reply does not claim a task is complete.

Provider VM access to Medousa tools is a separate pairing step. The workshop still decides which files, vault content, and tools that VM may access. Never paste a workshop credential into a chat message.
