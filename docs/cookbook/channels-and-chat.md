# Channels & chat commands

Medousa Engine can answer on chat platforms using the same memory and rules as the app.

Configure tokens in `medousa setup` or `product_config.json`. Start adapters:

```bash
medousa start discord
medousa start telegram
medousa start slack
medousa start whatsapp
```

Engine must be healthy (`medousa doctor`).

---

## Slash commands (Discord, Telegram, Slack, WhatsApp)

| Command | Effect |
|---------|--------|
| `/new` | Fresh session |
| `/brief` | Morning brief (optional note after) |
| `/skills` | List imported specialties |
| `/skill <id> [script] [extra]` | Run skill in sandbox |
| `/ask …` | Direct question |
| `/regen` | Regenerate last answer |
| `/stop` | Cancel running work |
| `/history` | Resume older conversation |
| `/model`, `/depth`, `/name` | Tune response |
| `/health`, `/heartbeat` | Runtime status |

Plain text works too — no slash required.

---

## Telegram example

Text `/brief` on Telegram → engine enqueues morning summary → delivery back to chat.

Allowlists and heartbeat chat IDs: **Settings** in setup wizard or `product_config.json` → `telegram.allowed_user_ids`.

---

## WhatsApp

Local deliver endpoint; session DB default:  
`~/.local/share/medousa/whatsapp/session.db`

First pairing shows QR in logs: `~/.local/share/medousa/logs/whatsapp.log`

---

## LAN mobile (iPhone)

Phone app talks to **Medousa Engine** on your Mac over Wi‑Fi. Desktop **Medousa** app handles QR pairing.

For mobile development, prefer a full Iroh invite. `medousa start daemon
--public` is an unsafe temporary escape hatch that exposes the complete daemon
API without mandatory authentication on every route; use it only on an
isolated LAN and stop it immediately after pairing. See
[Mobile & LAN](mobile-and-lan.md).

Protocol: [first-run-and-lan-pairing-plan.md](../../architecture/archive/first-run-and-lan-pairing-plan.md)
