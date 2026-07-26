# Troubleshooting

Start from what you **see** on screen. Basics: [Getting started](guide:getting-started) · [Find answers](guide:find-answers).

## Offline / can’t connect

| You see | Try |
|---------|-----|
| Status **Offline** or **Connecting…** | Settings → **Workshop** → **Save & test** |
| Chat blocked by Offline | Desktop: **Start** / **Restart**; check Connection settings |
| Phone can’t reach the computer | Same Wi‑Fi; computer awake; fresh QR under Sharing → Phone |
| Browser-only preview | Install and run the desktop app to chat |

Spotlight may offer **Check daemon health** — that checks whether Medousa on this computer is running.

## Wrong workshop

Notes and chats belong to the **active workshop**. If things look alien, switch workshops in the status bar or Settings → Workshop.

## Can’t send a message

1. Offline? Fix connection first.
2. Empty message with no attachments?
3. Sending a **picture**? Set a Vision model under Settings → Medousa Agent → Models.
4. API key error? Check Models / Providers on the computer.

## Stuck or silent reply

1. Still Connected?
2. Open **Work** — is a card **blocked**?
3. Chat — waiting for **Allow** or **Approve**?
4. Browser — verification / CAPTCHA banner? Finish it, then **Continue agent**.
5. Last resort: Settings → Workshop → **Restart** (chats pause briefly).

## Allow / Approve prompts

- **Allow / Deny** — permission for a tool.
- **Approve / Deny** — more tool rounds for a long task; `/budget list` lists pending ones.

→ [Permissions](guide:permissions-budgets)

## Browser verification

Complete the check in **Web**, then **Continue agent**. Don’t paste website passwords into chat.

## Note conflict

“This note changed elsewhere…”

| Choice | When |
|--------|------|
| **Reload** | Keep the other version |
| **Keep mine** | Keep what you’re editing |
| **History** | Versions is on — compare older copies |

→ [Trash and versions](guide:vault-recovery)

## Schedule didn’t run / message didn’t send

1. Automations → Schedules / History — is it paused?
2. **Runtime** → Delivery / Jobs (advanced).
3. Was the computer awake at the scheduled time?

## Phone / peers / channels

- Phone: Sharing exposure, same network, fresh invite — [Sharing and phone](guide:sharing-phone).
- Peers: Connect, trust, or revoke on the Peers surface.
- Messaging: channel status **needs_setup** until Save & connect — [Messaging channels](guide:messaging-channels).

## Packages / MCP / models

- Packages and MCP install on the **desktop app**.
- Missing tools after setup → Runtime allowlists may be too tight.
- Dictation needs the desktop app, a dictation model, and mic permission.

## Still stuck?

Write down the **exact sentence** on screen, your **workshop name**, and whether Work shows a blocked card. Then Workshop → Restart, or revisit the chapter for that screen.

Next: [Workshops and connections](guide:workshops-connections) · [Find answers](guide:find-answers).
