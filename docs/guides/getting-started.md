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

On first launch the app starts your **Personal** workshop for you. Desktop
onboarding happens inside Home: choose what you want nearby, shape the first
layout, and pick a Medousa mark and matching color theme. On iPhone and Android,
Personal runs as an embedded engine: name your Home, pick its look, and
optionally connect a model provider. Pairing a computer is not required.

> Prefer a terminal / headless engine? See
> [Install & self-host](../cookbook/install-and-self-host.md).
> Release CDN / R2 layout: [release-to-r2](../cookbook/release-to-r2.md).
---

## 2. Shape Home, then choose how she thinks

On desktop, choose one or more focus areas. Code prepares the coding engine, language
servers, and shell session. Messages installs only the channel adapters you
explicitly select. Notes and planning need no optional download. Pick a focused,
side-by-side, or dashboard layout; it becomes your actual first desktop.

Choose one of the ten approved Medousa marks. Its matching theme previews on the
real app chrome immediately, and the mark and theme stay independently editable
under **Settings → Preferences → Look**.

If you selected Medousa assistance, pick one model path:

Pick one path:

| Path | When to use it |
|------|----------------|
| **Offline / private brain** | Keep model weights on this machine (Gemma). Needs the offline brain package + a model download. |
| **Bring your own key** | OpenAI, Anthropic, and other cloud providers — keys stay on your device. |
| **Ollama** | You already run Ollama locally. |

If offline brain isn’t installed yet, the wizard points you to
**Settings → Packages** to add it. Cloud keys work without Packages.

Enter Home whenever you are ready. Optional package downloads continue in the
background and do not block the workspace.

On iPhone or Android, choose **Workspace with a brain**, then sign in with
ChatGPT to use your subscription or search the providers and models supported
by embedded Personal and enter an API key on-device.
Choose **Just the workspace** to start with Home, Notes, Calendar, and
Automations and add a model later. The wizard does not open the keyboard until
you choose a field, and computer pairing stays optional under **Settings →
Connection**.

---

## 3. Say hello

Send a normal message. Medousa keeps work durable on the engine — closing the
window does not throw away an accepted job.

The mode control beside the composer shows how Medousa will approach this
conversation. **General** is the everyday life, planning, and research mode.
**Coder** can be selected before a project is bound. Chat then offers ready
projects, blank project creation, or a least-authority setup turn where Medousa
can choose or create the project from an explicit request. Once bound, Coder
works inside the governed worktree with a turn-scoped lease and a restricted
coding tool surface. Your selection follows the conversation across restarts.

Medousa can also suggest a mode when another approach would materially help.
The suggestion appears above the composer and expires without changing the
current mode. In **Settings → Medousa Agent → Mode suggestions**, choose the
expiry window and whether task-scoped or all suggestions may auto-accept.

Useful early asks:

- *“Remember that I prefer concise answers.”*
- *“What’s in my vault?”* (after you add a folder)
- *“Build me a simple notes canvas.”*

To include a screenshot or document, choose **+ → Attach** or drag files onto
the composer. On a phone, **+** also offers **Take photo**, **Photo library**,
and **Attach file**. iPhone HEIC/HEIF photos are converted automatically; JPEG,
PNG, GIF, WebP, AVIF, BMP, and TIFF images are accepted across desktop and
mobile. The files appear as removable previews before you send; each message
can include up to five attachments.

To put an unfinished prompt aside intentionally, choose **+ → Stash draft**.
Medousa saves its text and attachments to the connected workshop, then clears
the composer. Open **+** again and choose the saved prompt under **Prompt
stashes** to restore it without sending. The trash control removes a stash;
ordinary per-conversation draft recovery remains private to the app.

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

Mobile uses a compact shell with its own embedded Personal engine. Its menu button
stays on the left. Open the menu and choose **Edit** to show or hide supported
destinations for the active layout. That edits the same workshop-owned layout
preset used by the desktop rail; it does not create a separate phone profile or
disable hidden features. Home, You, Preferences, and Workshop remain available.

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
The download stays cold: model memory is allocated only when a chat turn uses
**Medousa Local**, and can be released from **Settings → Connection → Private
brain**.

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
| Windows console flash / PowerShell windows popping | Update to current packages; workshop sidecars and language servers spawn with no console window. Restart the workshop after upgrading shell-session / coding-engine |
| Terminal or Code returns 503 / “health timed out” | Settings → Packages — reinstall **shell-session** and **coding-engine**, then restart the workshop. Closing flashed console windows kills those hosts |
| C# Problems on every non-`Program.cs` file | Install `csharp-ls` (`dotnet tool install -g csharp-ls`) or OmniSharp on PATH; open a folder with a `.sln`/`.csproj` so the server loads the project instead of single-file mode |

Still stuck? [Doctor & health](../runbooks/doctor-and-health.md) (power users) or
open a GitHub issue (not for security — see [SECURITY.md](../../SECURITY.md)).
