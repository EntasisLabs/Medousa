# Packages (Settings)

**Audience:** Medousa app users who want optional binaries without opening the
standalone installer.

Home already ships with the **engine** (daemon, CLI, and TUI). Packages is where
you add more later — offline brain, messaging adapters, MCP gateway.

From a terminal you can also use:

```bash
medousa packages status
medousa pull mcp-gateway
medousa pull telegram
medousa update
```

---

## Open Packages

1. Open **Settings** (gear).
2. Choose **Packages** in the left nav (desktop), or find **Packages** under
   Connection → Extras on some layouts.

You should see a short list of optional components with **Install**, **Update**,
or **Installed**.

---

## What you can install

| Package | What you get |
|---------|----------------|
| **Offline brain** | `medousa_local` — on-device inference for Gemma |
| **MCP gateway** | Connect MCP tool servers to Medousa |
| **Telegram / Discord / Slack / WhatsApp** | Channel adapter binaries |

**Not listed here (on purpose):**

- The **desktop app** — you’re already in it
- The **engine** — bundled with Home
- **Model weights** — download from the private-brain / Models UI after Offline
  brain is installed

---

## Install or update

1. Click **Install** (or **Update** when a newer build is available).
2. Wait for the progress line — one install at a time is enough.
3. When it says **Installed**, the binary is under your Medousa data directory
   (`…/medousa/bin`). Home finds it automatically.

Need model weights next? Use the link to **Connection → Extras** (private brain
panel) or **Settings → Models**, then download Gemma.

---

## Remove

Optional packages show **Remove**. That deletes the binary and package marker
from your data directory. Your chats and vault stay put.

---

## Advanced: Medousa Installer

At the bottom of Packages, **Open Medousa Installer…** launches the standalone
installer in modify mode when it’s installed. Use that for repair, full
workloads (Express / Offline workstation / Developer), or headless-oriented
layouts.

Most people never need it after Home-first install.

Operators / CI: [Install & self-host](../cookbook/install-and-self-host.md) ·
[Release to R2](../cookbook/release-to-r2.md).

---

## Tips

- Packages needs a network path to your **release manifest**
  (`MEDOUSA_RELEASE_BASE_URL` or the embedded release defaults).
- Phone / companion apps don’t install desktop binaries — do Packages on the Mac
  or PC that hosts the engine.
- After installing a channel adapter, configure tokens under messaging /
  [Channels](channels.md).
