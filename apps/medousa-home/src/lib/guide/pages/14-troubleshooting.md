# Troubleshooting

Start from what you **see** on screen. Basics: [Getting started](guide:getting-started) · [Find answers](guide:find-answers).

```accordion
title: By what you see
multiple: true

---
label: Offline / can’t connect
icon: globe
body: |-
  - Status **Offline** or **Connecting…** → Settings → **Workshop** → **Save & test**
  - Chat blocked by Offline → desktop **Start** / **Restart**; check Connection settings
  - Phone can’t reach the computer → same Wi‑Fi; computer awake; fresh QR under Sharing → Phone
  - Browser-only preview → install and run the desktop app to chat

  Spotlight may offer **Check daemon health** — that checks whether Medousa on this computer is running.

  → [Find answers](guide:find-answers#offline) · [Getting started](guide:getting-started)
open: true
---
label: Wrong workshop
icon: layers
body: |-
  Notes and chats belong to the **active workshop**. If things look alien, switch workshops in the status bar or Settings → Connection.

  → [Getting started](guide:getting-started) · [Workshops and connections](guide:workshops-connections)
---
label: Can’t send a message
icon: message-circle
body: |-
  1. Offline? Fix connection first.
  2. Empty message with no attachments?
  3. Sending a **picture**? Set a Vision model under Settings → Medousa Agent → Models.
  4. API key error? Check Models / Providers on the computer.

  → [Chat](guide:chat)
---
label: Stuck or silent reply
icon: hourglass
body: |-
  1. Still Connected?
  2. Open **Work** — is a card **blocked**?
  3. Chat — waiting for **Allow** or **Approve**?
  4. Browser — verification / CAPTCHA banner? Finish it, then **Continue agent**.
  5. Last resort: Settings → Connection → **Restart** (chats pause briefly).

  → [Work](guide:work-jobs) · [Permissions](guide:permissions-budgets)
---
label: Allow / Approve prompts
icon: shield
body: |-
  - **Allow / Deny** — permission for a tool.
  - **Approve / Deny** — more tool rounds for a long task; `/budget list` lists pending ones.

  → [Permissions](guide:permissions-budgets) · [Find answers](guide:find-answers#allow-or-approve)
---
label: Browser verification
icon: globe
id: browser-captcha--verification
body: |-
  Complete the check in **Web**, then **Continue agent**. Don’t paste website passwords into chat.

  → [Browser](guide:browser)
---
label: Note conflict
icon: pencil
body: |-
  “This note changed elsewhere…”

  - **Reload** — keep the other version
  - **Keep mine** — keep what you’re editing
  - **History** — Versions is on — compare older copies

  → [Trash and versions](guide:vault-recovery)
---
label: Schedule didn’t run / message didn’t send
icon: clock
body: |-
  1. Automations → Schedules / History — is it paused?
  2. **Runtime** → Delivery / Jobs (advanced).
  3. Was the computer awake at the scheduled time?

  → [Runtime](guide:runtime-telemetry) · [Automations](guide:grapheme-automations)
---
label: Phone / peers / channels
icon: users
body: |-
  - Phone: Sharing exposure, same network, fresh invite — [Sharing and phone](guide:sharing-phone)
  - Peers: Connect, trust, or revoke on the Peers surface
  - Messaging: channel status **needs_setup** until Save & connect — [Messaging channels](guide:messaging-channels)
---
label: Packages / MCP / models
icon: cpu
id: mcp-unavailable
body: |-
  - Packages and MCP install on the **desktop app**.
  - Missing tools after setup → Runtime allowlists may be too tight.
  - Dictation needs the desktop app, a dictation model, and mic permission.

  → [Packages and MCP](guide:mcp-packages) · [Platform matrix](guide:platform-matrix)
---
label: Still stuck?
icon: alert-triangle
body: |-
  Write down the **exact sentence** on screen, your **workshop name**, and whether Work shows a blocked card. Then Workshop → Restart, or revisit the chapter for that screen.

  → [Find answers](guide:find-answers) · [Workshops and connections](guide:workshops-connections)
```

Next: [Workshops and connections](guide:workshops-connections) · [Find answers](guide:find-answers).
