# Getting started with Medousa

**Audience:** anyone who just downloaded Medousa. No terminal required.

Medousa is a permanent AI workspace on your devices. This walkthrough gets you
from download to first chat in about a minute, then points you at the next
useful surfaces.

---

## 1. Download the app

1. Get **Medousa** for your desktop (Mac, Windows, or Linux):

```bash
curl -fsSL https://raw.githubusercontent.com/EntasisLabs/Medousa/main/scripts/install-app.sh | bash
```

   Or open
   **[releases.entasislabs.com](https://releases.entasislabs.com/medousa/stable/installer-bootstrap.json)**
   — the bootstrap picks the right build for your platform.
2. Install and open **Medousa**.

On first launch the app starts a local **engine** for you. You should land in
the welcome flow, then chat.

> Prefer a terminal / headless engine? See
> [Install & self-host](../cookbook/install-and-self-host.md).
> Release CDN / R2 layout: [release-to-r2](../cookbook/release-to-r2.md).
---

## 2. Welcome flow — choose how she thinks

Pick one path:

| Path | When to use it |
|------|----------------|
| **Offline / private brain** | Keep model weights on this machine (Gemma). Needs the offline brain package + a model download. |
| **Bring your own key** | OpenAI, Anthropic, and other cloud providers — keys stay on your device. |
| **Ollama** | You already run Ollama locally. |

If offline brain isn’t installed yet, the wizard points you to
**Settings → Packages** to add it. Cloud keys work without Packages.

Finish the wizard. You should be in **Chat**.

---

## 3. Say hello

Send a normal message. Medousa keeps work durable on the engine — closing the
window does not throw away an accepted job.

Useful early asks:

- *“Remember that I prefer concise answers.”*
- *“What’s in my vault?”* (after you add a folder)
- *“Build me a simple notes canvas.”*

---

## 4. Orient yourself (desktop)

| Surface | What it’s for |
|---------|----------------|
| **Chat** | Talk, think, attach context |
| **Work** | Background jobs and the work board |
| **Library / Vault** | Notes and finished artifacts |
| **Web** | Browse and save pages |
| **Peers** | Other workshops / people on your network |
| **Workshop / Automations** | Scripts, flows, schedules |
| **Settings** | Room, models, memory, phone, packages, connection |

Mobile uses a compact shell with the same engine behind it.

---

## 5. Add more when you need it

Home already includes the engine. Optional pieces live in
**Settings → Packages**:

- Offline brain binary
- Channel adapters (Telegram, Discord, Slack, WhatsApp)
- CLI tools
- MCP gateway

Guide: [Packages](packages.md).

Model **weights** for offline Gemma download from the private-brain / models UI
after the binary is installed — not from Packages itself.

---

## 6. Optional next steps

- [Phone pairing](phone-pairing.md) — use your phone as a portal
- [Peers & Nearby](peers-and-nearby.md) — LAN / tunnel workshops
- [Memory & identity](memory-and-identity.md) — teach who you are
- [Channels](channels.md) — messaging from Settings
- [Workshop & Automations](workshop-and-automations.md) — flows and scripts
- [Custom views & canvas](../cookbook/custom-views-and-canvas.md) — pin your own pages

---

## Troubleshooting (quick)

| Symptom | Try |
|---------|-----|
| Can’t chat / engine down | **Settings → Connection** — restart engine; wait until health looks good |
| Offline path blocked | **Settings → Packages** — install Offline brain, then download a model |
| macOS blocks the app | Right-click → Open the first time, or allow in Privacy & Security |
| Windows console flash | Rebuild with current Home — daemon should spawn without a console window |

Still stuck? [Doctor & health](../runbooks/doctor-and-health.md) (power users) or
open a GitHub issue (not for security — see [SECURITY.md](../../SECURITY.md)).
