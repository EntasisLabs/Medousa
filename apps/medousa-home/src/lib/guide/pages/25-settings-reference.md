# Settings reference

What each Settings area is for. Everyday look-and-feel lives under Preferences; power and safety knobs live under Medousa Agent and Runtime Controls.

Related: [How Medousa fits together](guide:architecture) · [Desktop, web, and phone](guide:platform-matrix) · [Where your data lives](guide:data-lifecycle)

## At a glance

| Section | Who it affects | Where you can change it |
|---------|----------------|-------------------------|
| **Preferences** | Mostly this device (theme also follows layout) | All apps; some items phone-only |
| **Medousa Agent** | This workshop’s answers and models | Editable wherever the selected workshop grants control |
| **Runtime Controls** | Tool safety and advanced engine options | Same as Agent |
| **Sharing** | Phone, peers, Shared seats, channels | QR and Shared host controls on desktop |
| **Connections** | ChatGPT account sign-in for Medousa; Codex/Cursor coding-agent sign-in | Medousa account sign-in works in Personal on phone; coding-agent connections remain desktop-only |
| **Packages** | Optional software on this computer | Desktop app only |
| **MCP** | External tool servers | Desktop app only |
| **Connection** | Which workshop you’re in, restart, updates | Address everywhere; file paths on desktop |

## Preferences

| Band | Controls |
|------|----------|
| **Look** | Light/dark, color theme, what chrome to show |
| **Work cards** | How long finished Work stays on the board |
| **Everyday** | Alerts and guidance; on phone, push / Live Activity |
| **More display** | Model picker, Liquid chat, technical detail in chat |

Motion calming follows your **system** reduced-motion setting — there’s no separate Medousa toggle.

## Medousa Agent

Models (chat, vision, dictation), stance/voices, memory depth, and presentation cleanup. Changes apply to the workshop. Stages and provider keys are under expandable details.

## Runtime Controls

Advanced: which tools are allowed, shell access, network limits, and optional note **Versions**. Empty tool lists can mean everything is allowed — be careful. Day-one safety tips: [Permissions](guide:permissions-budgets).

## Sharing / Connections / Packages / MCP / Connection

- **Sharing** — phone QR, peers, Shared mode, messaging channels.
- **Connections** — connect a **ChatGPT account** directly to the Medousa runtime on desktop or in Personal on phone. Medousa still owns the agent loop and tools. Desktop hosts can separately sign into **Codex** and **Cursor** coding agents; missing CLIs install from the same screen using vendor installers. Signed-out routes show a sign-in prompt in the chat runtime picker.
- **Packages** — optional Offline brain and helpers (desktop).
- **MCP** — connect external tools (desktop).
- **Connection** — switch workshop, restart engine, app updates, **Files & diagnostics** paths on desktop.

```callout
tone: tip
title: Older name
body: If you remember Settings → Rhythm, that content now lives under Preferences.
```

Next: [Desktop, web, and phone](guide:platform-matrix).
