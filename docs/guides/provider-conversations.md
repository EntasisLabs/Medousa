# Muse and Grok Bot conversations

Medousa can show conversations with agents that run on their own computers. Set up Muse sessions and Grok bots in **Settings → External Agents**. In Chat, choose **Muse** or **Grok Bot** from the runtime picker, then choose the registered session or bot from the model picker. Their transcripts belong to the connected workshop, so the same history appears when you reconnect from another Medousa device.

## Grok Bot

1. Create a webhook-triggered routine for the Bot you want to contact. Copy its POST URL and sender key.
2. In **Settings → External Agents → Grok Bot**, choose **Add bot** and enter a label, Bot name, URL, and key. Medousa stores the key in the workshop credential store.
3. Save the callback key shown once. The Bot VM needs a separately paired Medousa workshop credential and must report events to the conversation's `/events` route with `x-medousa-bridge-key`. The webhook key and callback key serve different directions. The VM can use `medousa-cli daemon-external-event` with `MEDOUSA_BRIDGE_BEARER` and `MEDOUSA_BRIDGE_KEY` set in its private environment.
4. In Chat, choose **Grok Bot** and the registered bot, then send a message. An accepted webhook means the routine started; the conversation shows a result only when a callback arrives.

The routine receives JSON with `schema_version`, `request_id`, `conversation_id`, and `message`. Keep the IDs from that request when reporting progress or the result. For example, after configuring `MEDOUSA_BRIDGE_BEARER` and `MEDOUSA_BRIDGE_KEY` privately on the VM:

```sh
medousa-cli daemon-external-event "$CONVERSATION_ID" "$EVENT_ID" completed "$RESULT" \
  --request-id "$REQUEST_ID" --daemon-url "$WORKSHOP_URL"
```

Use a new event ID for each report. The workshop URL must be reachable from the VM through an authenticated Medousa connection.

If the callback key is lost or exposed, use **Rotate callback key** in External Agents and update the VM's private configuration. **Remove** deletes its transcript from this workshop and revokes its stored keys.

## Muse

1. Install and pair the WhatsApp adapter on the **connected workshop** through **Settings → Packages** and the [Channels guide](channels.md).
2. In **Settings → External Agents → Muse**, choose **Add session → Find Muse chat**. In the normal Muse chat in WhatsApp, ask Muse to reply with the exact one-time code Medousa shows. The linked adapter uses that reply to discover the chat's internal ID; you do not need a phone number.
3. When Medousa reports **Chat observed**, connect the session. In Chat, choose **Muse** and that session, then send a small test message. Replies from the bound chat appear in the same chat view if the linked adapter can access this Meta-managed chat.

Muse's WhatsApp chat may be managed by Meta and may not be visible to linked-device adapters. Discovery expiring without observing a reply means the transport is unavailable; it is not evidence that Muse ignored the message. The linked-device WhatsApp adapter is experimental. If a send is uncertain, check the native chat before sending the same request again.

## What the status means

- **Sending** is a durable attempt. If it remains after a workshop restart, check the provider's native history before resending.
- **Transport accepted** means WhatsApp accepted a send or the Grok Bot webhook accepted a request. It does not mean the agent finished.
- **Transport uncertain** means the workshop could not prove whether the provider received the request. Check the provider's native chat or routine history before resending.
- **Provider message, progress, question, completed, failed** are events the workshop actually received. An ordinary Muse reply does not claim a task is complete.

Provider VM access to Medousa tools is a separate pairing step. The workshop still decides which files, vault content, and tools that VM may access. Never paste a workshop credential into a chat message.
