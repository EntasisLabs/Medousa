# Navigation and surfaces

Medousa is a set of **places** you switch between — Chat, Library, Web, and more — not a pile of separate apps. Here’s where things live on computer and phone.

## The rail and the dock

**Desktop**

- **Primary rail** — destinations from the active **layout preset**, in order. Use it for big context changes.
- **Dock** — usually **Settings**, **Runtime**, **You**, and **Channels** live here rather than in the main strip.
- **Spotlight** (`⌘K` / `Ctrl+K`) — jump to a surface by name when you already know the destination.
- **Edit destinations** — status bar layout menu; show/hide and reorder surfaces for the current preset (Settings and Runtime stay available as safety surfaces).

**Library** and **Automations** are explorers with **modes** (see below). Opening “Automations” or legacy “Workshop” deep links usually lands you in Library/Automations with the right mode selected.

## Built-in surfaces

Labels are what operators see. Availability depends on the active layout preset (Default includes nearly everything; **Focus** trims to a smaller set).

| Surface | Role |
|---------|------|
| **Home** | Landing / presence; often not a primary rail icon |
| **Chat** | Sessions, composer, turns, artifacts |
| **Work** | Kanban of background and agent jobs |
| **Library** | Notes, Local Files, Presentations |
| **Calendar** | Day / week / month; phones often default to Day |
| **Web** | Human browser (tabs, history, bookmarks) |
| **Map** | Picture of recent chats and linked notes |
| **You** | Profiles / identity field (dock door) |
| **Automations** | Scripts, Agents, Flows, Schedules, History |
| **Peers** | Nearby workshops, trust, inbox |
| **Channels** | Messaging adapters (under Settings / More — not the life rail) |
| **Runtime** | Now, Jobs, Delivery, Routing diagnostics |
| **Settings** | Preferences, Agent, Runtime Controls, Sharing, Packages, Workshop |

**Custom views** you create also appear as surfaces on the rail when the layout includes them.

```callout
tone: note
title: Workspace vs Library
body: Some Spotlight copy still says “Workspace” for notes and files. The rail and explorer label is Library.
```

## Library modes

Open **Library**, then switch mode in the explorer strip:

| Mode | For |
|------|-----|
| **Notes** | Vault markdown, boards, sheets, Liquid |
| **Local Files** | Loose / pinned folders on the desktop host |
| **Presentations** | Presentation artifacts |

## Automations modes

Open **Automations** (or deep-link into scripts/agents):

| Mode | For |
|------|-----|
| **Scripts** | Grapheme workbench |
| **Agents** | Specialist import and tuning |
| **Flows** | Multi-step workflows |
| **Schedules** | Cron / recurring runs |
| **History** | Past runs (secondary strip — chevron) |

## Layout presets

Built-in examples:

| Preset | Typical surfaces |
|--------|------------------|
| **Default** | Full set (chat, work, library, calendar, web, map, peers, automations, messaging, runtime, settings, …) |
| **Focus** | Smaller set — chat, peers, work, library, map, settings, runtime |

Switching preset can also apply that preset’s **theme** and **shell chrome**. Custom presets come from your environment / canvas setup — see [Views and environments](guide:views-environments).

## Shell tabs and panes

Inside a desktop:

- Split **right** / **down**; focus panes from the keyboard (see [Keyboard and flow](guide:keyboard-flow)).
- Up to **four panes** per desktop.
- Tabs inside a pane for sessions, notes, browser pages, etc.
- Some surfaces are **singletons** (one tab max): Library, Peers, Channels, Map, Work, Calendar, Settings, Runtime, You.

Treat panes like a light tiling manager for one workshop — not twelve separate Home windows.

## Virtual desktops

- Up to **four** named desktops (status marks / hotkeys 1–4).
- Each desktop snapshots **pane layout and tabs only**.
- **Shared across desktops:** vault, chat sessions, and the active workshop.

Rename a desk when it has a job (“Writing”, “Debug”). Switching desks should feel like walking to another bench, not reloading the app.

## Pop-outs and the desktop toolbar

| Window | Role |
|--------|------|
| Chat pop-out | Full chat away from the shell |
| Vault sticky | Floating note |
| Web | Dedicated browser window |
| View pop-out | A custom view as its own window |
| Desktop toolbar | Slim always-on strip to summon the above |
| Operator's Guide | This manual |

Spotlight: **Toggle desktop toolbar**. Closing a pop-out usually **hides** it so state stays warm — summon again to restore.

## Mobile navigation

**Tab bar:** Home · Chat · Notes · Web · **More**

**More hub**

| Section | Destinations (examples) |
|---------|-------------------------|
| **Stay in touch** | You, Map, Agents, Calendar, Channels, Peers |
| **Preferences** | Preferences (Settings), Workshop (Runtime pulse / jobs) |
| **My views** | Custom surfaces from the active layout preset |

Automations may open from deep links or More rather than a permanent tab. Companion shells talk to a **remote workshop**; see [Getting started](guide:getting-started#phone--companion-first-run).

## Summon toolbars

Library, Automations, and similar explorers can grow a contextual toolbar. Mouse-shake may summon it if enabled. Prefer keyboard when you know the binding — [Keyboard and flow](guide:keyboard-flow).

## Quick “where is X?”

| I need… | Go to |
|---------|--------|
| Sessions / ask | Chat |
| Background job board | Work |
| Notes | Library → Notes |
| Scripts / flows | Automations |
| Identity / teach | You |
| LAN trust / inbox | Peers |
| Telegram etc. | Channels (Settings / More) |
| Job failures / delivery | Runtime |
| Pair phone / Shared mode | Settings → Sharing |
| Engine address / restart | Settings → Workshop |

Next: [Chat](guide:chat) for the surface most operators live in.
