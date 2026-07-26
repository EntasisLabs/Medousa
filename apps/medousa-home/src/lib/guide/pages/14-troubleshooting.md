# Troubleshooting

Start from what you **see**, not from a guess about the daemon. For mental model and connection basics, see [Architecture](guide:architecture) and [Getting started](guide:getting-started).

## Offline / reconnect

| You see | Do this |
|---------|---------|
| Status **Offline** / **Connecting…** | Settings → **Workshop** → address → **Save & test** |
| Chat offline gate | Desktop: **Start** / **Restart Medousa**, **Connection settings**, engine log |
| Phone cannot reach host | Same Wi‑Fi; host **Sharing** exposure; fresh QR / pairing link |
| Browser preview only | Run the Medousa desktop app to chat |

Spotlight **Check daemon health** when available. Deep runbook: connection reliability (bundled under Workshop resources).

## Wrong workshop

Sessions and vault belong to the **active workshop**. If notes or chat look alien: status bar workshop switcher, or Settings → Workshop → switch active workshop. See [Getting started](guide:getting-started#first-connection-checklist).

## Cannot send chat

1. Offline gate? Fix connection first.
2. Empty draft with no attachments?
3. Sending an **image** without a Vision model → Settings → Models.
4. API key rejected → Settings → Medousa Agent → Providers / Models (exact error often says so).

## Stalled or silent turn

1. Connection still Connected?
2. [Work](guide:work-jobs) — card **blocked** or stuck **in flight**?
3. Chat — pending **permission** or **budget** bar?
4. **Browser verification** banner — complete check → **Continue agent**.
5. External runtime (Cursor/Codex) — confirm that agent is still alive outside Home.
6. Last resort: Settings → Workshop → **Restart** engine (pauses active chats).

There may be no single Cancel button for every runtime; Work drag-to-cancel helps for in-flight cards.

## Budget blocked

- Chat: **Needs your approval** → Approve / Deny / Work
- `/budget list` then `/budget approve` or `deny`
- Work inspector on the blocked card

Details: [Permissions, budgets, and tool safety](guide:permissions-budgets).

## Permission denied / waiting

- Chat: **Agent needs permission** → **Allow** or **Deny**
- Note which Runtime is asking
- If tools seem missing after Allow, check Runtime Controls allowlists and MCP

## Browser CAPTCHA / verification

1. **Open in Web** if the challenge is not already focused.
2. Complete the check as a human in the browser.
3. **Continue agent**.

Do not put site passwords into the chat composer.

## Vault conflict

Bar: **This note changed elsewhere…**

| Action | When |
|--------|------|
| **Reload** | Accept the other version |
| **Keep mine** | Keep local buffer |
| **History** | If Git versions are enabled — compare / restore |

Versions and trash get a fuller chapter later; for day-one, pick Reload vs Keep mine deliberately.

## Schedule or delivery failed

1. Automations → **Schedules** / **History** — failed styling, last run.
2. Runtime → **Delivery** — pending deliveries, last delivery.
3. Runtime → **Jobs** — failed / dead letter counts.
4. Confirm the workshop was online at the scheduled time.

## Phone discovery

- Host: Settings → **Sharing** → Phone; prefer **Always reachable on Wi‑Fi** when you need companions often.
- Guest / captive Wi‑Fi: use QR or short code, not discovery alone.
- Re-run pairing if the invite expired.

## Peer trust

- **Peers** surface: Connect nearby untrusted; review trusted list; revoke when done.
- Manual URL fallback if discovery fails.
- Peers are not the same as phone portals — [Architecture](guide:architecture#workshop-vs-peer-vs-phone).

## MCP unavailable

- Settings → **MCP** (desktop app). Companion/web may say to connect MCP from desktop.
- Check gateway/package install errors on that page.
- Missing tools after config → Runtime Controls module allowlist may be excluding them.

## Model / provider errors

| Message theme | Check |
|---------------|--------|
| API key rejected | Settings → Models / Providers |
| Vision required | Assign Vision model before image send |
| Dictation / mic | Desktop app + Dictation model + mic OS permission |
| Offline brain download | Settings → Packages; chat can still use BYOK |

## Shared rooms missing

Enable **Shared mode** in Settings → Sharing before **New shared room**. Older workshops may not support it.

## Custom view / feed looks broken

Layout preset may hide the surface; feed/runtime mismatch is covered under [Views and environments](guide:views-environments). Try Default preset, then reopen the view.

## Still stuck

1. Note the **exact UI sentence** (offline gate, budget bar, conflict bar).
2. Note **workshop name**, **runtime**, and whether Work shows a blocked card.
3. Workshop → Engine version / Restart.
4. Open this guide’s chapter for that surface once D2 docs land — until then, Settings section names above are the map.

Next: [Workshops and connections](guide:workshops-connections) for multi-engine and updates.
