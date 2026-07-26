# Messaging channels

Telegram, Discord, Slack, and WhatsApp adapters let the workshop deliver and (where configured) accept messages outside Home. Configure under Settings → **Sharing** / Network → messaging channels, or the **Channels** surface.

Related: [Sharing and phone](guide:sharing-phone) · [Permissions, budgets, and tool safety](guide:permissions-budgets) · [Troubleshooting](guide:troubleshooting)

## Before you connect

1. Prefer a workshop you control — tokens are workshop credentials.
2. Use **allowlists** / user IDs wherever the channel offers them.
3. Heartbeat destinations are optional; leave blank until you need nudges.
4. Status shows connected / ready / **needs_setup** — use **Save & connect**.

## Per channel

| Channel | Credentials | Who may call | Notes |
|---------|-------------|--------------|--------|
| **Telegram** | **Bot token** | **Your Telegram user ID** (`/whoami`) | Optional heartbeat chat IDs |
| **Discord** | **Bot token** | **Command prefix** (default `!`) | Heartbeat channel IDs |
| **Slack** | **Bot token** (xoxb) + **App token** (xapp) | **Allowed Slack user IDs** | Heartbeat channel IDs |
| **WhatsApp** | **Deliver bind** (default `127.0.0.1:7422`); JIDs in Connect | **Deliver URL**, optional **Session DB path** | Heartbeat chat JIDs |

Tokens are cleared/replaced on save according to the field UX — do not paste tokens into chat or vault notes.

```callout
tone: warning
title: Allowlists matter
body: An empty or overly broad allowlist can let strangers drive the workshop. Set user IDs before sharing a bot into a busy server.
```

## Delivery from automations

Schedules can deliver to **Stay in Medousa** or **Telegram** (when that adapter is ready). Failed deliveries show under Runtime → **Delivery** and Automations **History**.

Next: [Grapheme and automations](guide:grapheme-automations) · [Runtime telemetry](guide:runtime-telemetry).
