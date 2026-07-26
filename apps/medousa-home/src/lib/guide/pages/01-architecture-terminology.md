# Architecture and terminology

Medousa has a few nouns that look interchangeable until you switch workshops or pair a phone. This chapter is the shared mental model for the rest of the guide.

## The three layers

| Layer | What it is | What you control |
|-------|------------|------------------|
| **Home** | The app shell — rails, panes, pop-outs, status bar, this guide | Layout, themes on this device, which workshop you point at |
| **Workshop** | The host machine running your Medousa engine (often *This device*) | Models, vault files, sessions, tools, pairing, Shared mode |
| **Engine** | The daemon Home talks to — models, tools, turns, vault IO | Restart, address, health; pauses active chats when restarted |

Home without a healthy workshop connection is a shell. The workshop without Home is still the source of truth for notes and sessions.

```callout
tone: note
title: Engine, daemon, workshop
body: Settings and status copy use “engine,” “daemon,” and “workshop” for the same running host. Prefer “workshop” when talking to people; “engine” when you mean the process you restart.
```

## Workshop vs peer vs phone

These are **not** the same relationship:

| Relationship | Operator labels | Meaning |
|--------------|-----------------|---------|
| **Your workshop** | Workshop, Local workshop, Remote | The brain and vault *you* are working in. Home connects to its address. |
| **Phone companion** | Phone, Pair a phone | A second **portal** into *your* workshop (same Wi‑Fi / invite). Turns and most settings still live on the host. |
| **Peer** | Peers, Nearby | Another workshop on the LAN you **trust** — inbox, messages, optional canvas bundles. Separate identity and vault. |

- Pairing a phone does **not** create a peer.
- Trusting a peer does **not** make their vault yours.
- Switching the active workshop changes vault, sessions, and tools for Home.

Deep procedures: [Workshops and connections](guide:workshops-connections), [Sharing and phone](guide:sharing-phone).

## Identity: profile, You, specialist, stance, models

| Term | Where you see it | Meaning |
|------|------------------|---------|
| **Profile** | **You**, seat switches, Shared mode seats | Who *you* are on this workshop — display name, identity memory, preferences. Default is often *Personal*. |
| **You** | Surface / dock door | The identity field — teach and correct what the workshop knows about people and preferences. |
| **Specialist / Agent** | Automations → **Agents**, import wizards | An importable skill/manuscript with its own tools and schedule options — not the same as your user profile. |
| **Medousa Agent** | Settings → **Medousa Agent** | Orchestrator defaults: stance, memory, depth, charter on the workshop. |
| **Stance (voice)** | Under the composer; Agent settings | How answers should feel (Default, Direct, or a custom preset). Stored on the workshop. |
| **Depth** | Agent settings | Concise / Standard / Deep for answer length. |
| **Models / Stages** | Settings → Models | Main, vision, and dictation profiles; optional stage routes (Lead, Reader, …). |

On a phone companion, changing models or stance usually **updates the host workshop**, not a separate phone-only brain.

## Shared mode

Settings → **Sharing** → **Shared**:

- **Off** — Personal hats as usual; one operator identity flow.
- **On** — Team **seats** on one shared brain and vault; Phone invites can bind to a seat. Requires a workshop that supports Shared mode (desktop host). Older engines show that it is unavailable.

Shared chat rooms need Shared mode enabled. Full seat/invite detail: [Sharing and phone](guide:sharing-phone).

## What stays where (state scopes)

| Scope | Examples | Operator hint |
|-------|----------|---------------|
| **This device** | Everyday notification prefs, workshop guidance, preferred mode, wizard flags | Preferences bands that say they are saved on this device |
| **Workshop (host)** | Models, stance/charter, vault files, Shared mode, pairing, runtime tool policy | Changes follow you when you switch Home machines that connect to the same workshop |
| **Profile** | Custom views / environment canvas for that profile | Views you build are tied to a profile id (often *personal*) |
| **Layout preset** | Which destinations appear on the rail, order, shell chrome, theme for that preset | Status bar layout switcher; **Edit destinations** |
| **Virtual desktop** | Pane splits and tab sets for that desk only | Up to four desks — **layout only**; vault, chat, and workshop stay shared |
| **Session** | Chat thread, drafts, which specialist chip is active | Switching sessions does not switch workshops |

When something “disappears,” ask: did I switch **workshop**, **profile**, **layout preset**, or **desktop**?

## Shape of a running day

```
Home shell
  ├─ Layout preset → rail destinations + chrome
  ├─ Virtual desktops (≤4) → pane/tab layouts
  └─ Active workshop connection
        ├─ Vault + Library / Automations
        ├─ Chat sessions + Work cards
        ├─ Profiles (You) + optional Shared seats
        ├─ Runtime (jobs, delivery) + tool policy
        └─ Peers / Phone / Channels (trust & portals)
```

## Glossary (seed)

| Term | Short definition |
|------|------------------|
| **Surface** | A full workspace mode (Chat, Library, Web, …) or a custom view |
| **Library** | Notes, Local Files, and Presentations explorer (rail may say Workspace in older copy) |
| **Automations** | Scripts, Agents, Flows, Schedules, History |
| **Work** | Kanban of background / agent jobs |
| **Runtime** | Live diagnostics — Now, Jobs, Delivery, Routing |
| **Map** | Session link map (Locus moments and links) |
| **Liquid** | Interactive document fences in notes and chat |
| **Grapheme** | Script language for automations |
| **Portal** | Phone (or similar) connected into your workshop |
| **Peer** | Trusted other workshop on the network |
| **Seat** | Shared-mode profile slot on one brain |
| **Layout preset** | Named set of surfaces, order, theme, and shell chrome |
| **Offline brain** | Optional local Gemma package — separate from cloud chat models |

Next: [Getting started](guide:getting-started) for first-run and the daily loop, then [Navigation and surfaces](guide:navigation-surfaces) for every built-in destination.
