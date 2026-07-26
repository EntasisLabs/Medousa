# Settings reference

Operator map of Settings sections: what they control, where state lives, and who can edit them. Day-one safety detail for Runtime lives in [Permissions, budgets, and tool safety](guide:permissions-budgets).

Related: [Architecture and terminology](guide:architecture) · [Platform matrix](guide:platform-matrix) · [Data and recovery](guide:data-lifecycle)

## Sections

| Section | Scope | Platform | Restart? | Risk |
|---------|-------|----------|----------|------|
| **Preferences** | Mostly **this device**; theme/chrome also **layout preset**; Work card retention writes **workshop** on desktop | All (some bands mobile-only) | No | Low |
| **Medousa Agent** | **Workshop** charter (`tui_defaults.json`) | Editable on desktop host; **read-only** on phone companion | Save hot-applies | Medium — models/keys/stance affect all turns |
| **Runtime Controls** | **Workshop** charter + Versions | Same read-only rule on phone | Save hot-applies; Versions toggle immediate; engine restart is under Workshop | **High** — shell, allowlists, tool posture |
| **Sharing** | Workshop + seats | QR mint / Shared host toggle: **desktop**; companion mostly read-only | Reachability may reconnect | Trust / seats / LAN exposure |
| **Packages** | **This machine** binaries | Desktop Tauri only | Install-driven | Disk / optional brain |
| **MCP** | Machine gateway + servers | Desktop for install/config | Gateway install | External tool trust |
| **Workshop** | Active workshop + device paths/autostart | Paths UI desktop; address all platforms | **Engine Restart** pauses chats | Wrong workshop / restart mid-turn |

## Preferences

| Band | Controls | Notes |
|------|----------|--------|
| **Look** | Light/dark, named color theme (per layout), shell chrome (rail, vault chat FAB, sidebar, mobile Home, layouts) | Theme follows active layout preset |
| **Work cards** | Hide-from-board (hours), clear archives (days) | Host-persisted on desktop; defaults 24h / 7d |
| **Everyday** | Work-done alerts, workshop guidance, open Web on browse; mobile: remote push, Live Activity | Device-local |
| **More display** | Technical activity, engine details in chat, model picker, Liquid chat | Device-local |

Reduced motion follows the **OS** preference (`prefers-reduced-motion`) — no Settings toggle.

## Medousa Agent

| Band | Controls |
|------|----------|
| **Answers** | Stance (builtin + up to 8 custom voices), depth |
| **Models** | Chat / Vision / Dictation; favorites; fallbacks — apply immediately |
| **Stages & providers** | Stage routes, API keys (expandable) |
| **Memory** | Hot/cold turns, prompt budgets |
| **Presentations cleanup** | Age / max per session (desktop engine) |

Copy that says “Runtime → Routing” for stages means the **Runtime surface** diagnostics tab, not Settings → Runtime Controls.

## Runtime Controls

| Band | Controls |
|------|----------|
| **Reach** | Tool posture, specialists, web search, tool rounds, module allowlist (empty = full catalog) |
| **Shell** | Agent shell tools, network ceiling, timeouts, output caps, binary / writable-root allowlists |
| **Engine** | Thinking traces, OTel, store backend, retries / quality |
| **Versions** | Optional Git vault versioning — [Vault trash and versions](guide:vault-recovery) |

## Sharing

| Band | Controls |
|------|----------|
| **Shared** | Shared mode / seats |
| **Phone** | Pairing QR / Forget |
| **Nearby** | Wi‑Fi reachability, peers, canvas backup conflict policy |
| **Channels** | Messaging adapters — [Messaging channels](guide:messaging-channels) |

## Packages / MCP / Workshop

See [MCP and packages](guide:mcp-packages) and [Workshops and connections](guide:workshops-connections). Workshop → **Files & diagnostics** lists resolved Engine data, Vault, and config paths on desktop.

```callout
tone: tip
title: Stale labels
body: Older copy may say Settings → Rhythm — that band merged into Preferences. Work card retention lives under Preferences → Work cards.
```

Next: [Platform matrix](guide:platform-matrix).
