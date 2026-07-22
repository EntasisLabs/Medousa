# Channels (messaging)

**Audience:** Medousa app users connecting Telegram, Discord, Slack, or WhatsApp.

Talk to the same brain from chat apps. Commands like `/brief` on Telegram can
kick off a morning summary that lands where you configured delivery.

---

## 1. Install the adapter (if needed)

Desktop **Settings → Packages** → install the adapter for your channel
(Telegram, Discord, Slack, WhatsApp). Home finds the binary under your data
`bin` folder.

From a terminal (same CDN packages):

```bash
medousa pull telegram    # or discord / slack / whatsapp
```

Guide: [Packages](packages.md).

---

## 2. Configure in the app

Open Settings messaging / channel surfaces (wording may sit under Reach or a
Messaging section depending on build) and paste bot tokens / webhook secrets.

Keep secrets on your machine — Medousa does not require a Medousa cloud account
for channels.

---

## 3. Smoke test

1. Restart or let Home respawn the adapter after save.
2. Message the bot from your phone.
3. Confirm the turn shows up in Medousa chat / history.

---

## Slash commands & specialties

Channel slash commands and specialty packs (morning brief, etc.) are documented
for operators in [Channels & chat](../cookbook/channels-and-chat.md) and
[Skills & specialties](../cookbook/skills-and-specialties.md).

From the app: ask Medousa to schedule a specialty to a channel, or wire delivery
from Automations.

---

## Safety

- Prefer least privilege for bot tokens.
- Review **Settings → Reach** so tools and outbound actions match what you want.
- Don’t paste tokens into public issues or screenshots.

---

## Related

- [Getting started](getting-started.md)
- [Workshop & Automations](workshop-and-automations.md)
- Operator CLI path: [channels-and-chat.md](../cookbook/channels-and-chat.md)
