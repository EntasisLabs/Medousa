# Getting started

This chapter matches the real first-run paths in Home: welcome wizard, connection health, and the first useful loop. For nouns (workshop vs peer vs phone), see [Architecture and terminology](guide:architecture).

## Before you begin

You need:

1. The **Home** app (desktop) or the **phone companion** pointing at a host.
2. A **workshop** address the app can reach (local engine on this Mac/PC, or a remote host).
3. Optionally a **brain** — BYOK cloud models or the Offline (local Gemma) package.

No account is required. Notes and files stay on the workshop host.

## Desktop first run (wizard)

On a fresh desktop install the wizard runs automatically. Progress (after you pick a mode): **arrive → space → mode → [brain] → ready**, with an optional phone step.

| Step | What you do |
|------|-------------|
| **Arrive** | Confirm Medousa as your permanent workspace on this machine |
| **Space** | Name the workspace, your name, and a theme |
| **Mode** | **Workspace with a brain** (recommended) or **Just the workspace** |
| **Brain** | Only if you chose a brain: **Offline** (local Gemma) or **BYOK** (provider + API key) |
| **Phone** | Optional — pair a companion with QR / invite |
| **Ready** | Land in Home; status should move toward Connected |

**Just the workspace** skips brain setup. You can add models later under Settings → Models / Packages. Skipping Offline download is allowed; the completion screen is non-blocking.

### Migration (“Welcome back”)

If Home detects an existing setup, you may see **Welcome back — Everything is still here** with your current provider/model. Choose **Continue** — you are not forced through a blank first-run.

### Re-run the wizard

**Settings → Workshop → More on this device → Welcome wizard → Re-run** (desktop). Use this to revisit model choice and optional phone pairing without reinstalling.

## Phone / companion first run

Mobile does **not** run the full desktop brain wizard. You connect to a host:

1. Open the companion app.
2. Enter the workshop **address**, or use a **pairing link / QR** from the host (**Settings → Sharing → Phone**).
3. Wait until the shell shows a healthy connection.

Most model and stance changes on phone update the **host** workshop. Host-managed settings may appear read-only on the companion.

## First connection checklist

1. Open **Settings → Workshop** (basement).
2. Confirm an **active workshop** (often Local workshop on this device).
3. Watch the status bar: **Connected**, **Connecting…**, or **Offline**.
4. If Offline: edit **Address** → **Save & test**, or fix the host engine.
5. Open **Chat** and send a short message.

Engine tile (version, tool count, last turn) and **Restart** live on the same Workshop status band. Restart pauses active chats and tools.

```callout
tone: tip
title: Wrong workshop, wrong world
body: Sessions and vault files belong to the active workshop. If chat looks empty or notes are missing, check the workshop switcher in the status bar before rewriting prompts.
```

## The daily loop

1. **Ask** in Chat — set Voice / Stance / Reasoning under the composer only when the turn needs it.
2. **Keep** durable results in the Library (notes, boards, sheets) or send work to the **Work** board for background jobs.
3. **Return** via rails, panes, or pop-outs — you should not relaunch Home for one note.

Suggested first hour:

| Goal | Where |
|------|--------|
| Prove the link | Chat — one short turn |
| Leave a note | Library → Notes |
| See background work | Work (when you use `/ask` or long jobs) |
| Optional phone | Settings → Sharing → Phone |

## Profiles and agents (day one)

- **Profile / You** — who you are; teach facts from the **You** surface.
- **Specialist (Agents)** — optional imported skills under Automations → Agents.
- **Stance / models** — under-composer controls and Settings → Medousa Agent / Models.

Switch profile or attach context from the composer **+** menu without leaving chat. Details: [Chat](guide:chat).

## Common first-day failures

| Symptom | What to check |
|---------|----------------|
| Cannot send chat | Status bar Offline / Connecting; Workshop → Save & test |
| Empty or “wrong” vault | Active workshop in the status bar |
| Wizard offers Offline but download stalls | Completion still works; finish later in Settings → Packages |
| Phone cannot find host | Same Wi‑Fi; host Sharing exposure; fresh QR / invite |
| Shared rooms missing | Shared mode off, or workshop too old |
| Rebuilt machine, old data | Migration screen, or restore workshop storage — see [Workshops and connections](guide:workshops-connections) |

## Where to go next

1. [Navigation and surfaces](guide:navigation-surfaces) — every built-in destination, Library/Automations modes, panes, desktops, mobile More hub.
2. [Chat](guide:chat) — composer, models, turn controls.
3. [Workshops and connections](guide:workshops-connections) — multi-workshop, engine restart, updates.
