# Medousa Home — Mobile Plan (M8)

> **Status:** In progress — M8a shell shipped in repo  
> **Date:** 2026-05-30  
> **Related:** [medousa-home-tauri-design.md](medousa-home-tauri-design.md), [medousa-home-main-workspace-plan.md](medousa-home-main-workspace-plan.md)

## Thesis

**Medousa on your phone isn’t the whole workshop — it’s the heartbeat.**

Glance at what’s moving. Say one thing. Tap one decision. Leave.

Desktop is where you *live* with the agent. Mobile is where you *stay in touch* with it. Same daemon, same stores, different choreography.

**We refuse:** a shrunk desktop — icon rail with nine destinations, split panes, kanban postage stamps, drag-to-cancel zones.

---

## Four tabs

| Tab | Label | Question it answers |
|-----|-------|---------------------|
| **Pulse** | Pulse | What’s happening right now? |
| **Work** | Work | What needs me? |
| **Chat** | Chat | What do I say? |
| **You** | You | Where’s everything else? |

Default landing: **Pulse** (glance first, compose second).

Cron, Messaging, Runtime, Skills, Library, Settings → **You** hub (purposeful lists, not equal tab citizens). Condense to three tabs later only if usage proves four is too many.

---

## Layout model

```text
┌─────────────────────────────┐
│ Connection pill · Activity  │  ← safe-area top
├─────────────────────────────┤
│                             │
│   One tab surface (stack)   │
│                             │
├─────────────────────────────┤
│ Pulse · Work · Chat · You   │  ← safe-area bottom
└─────────────────────────────┘
```

- **Stack navigation** inside tabs — no `SplitPane` on mobile.
- **Bottom sheets** for ask composer, completion, activity, sessions.
- **Full-screen story** for card detail (not side inspector).

---

## Work — vertical timeline (not kanban)

```text
Needs you        ← blocked / failed (top)
In motion        ← backlog · in flight · wrapping up
Done today       ← collapsed section
```

| Gesture | Action |
|---------|--------|
| Tap card | Open work story |
| Swipe left (in motion) | Cancel (future: haptic + undo) |
| Pull down | Refresh lanes |
| FAB | New ask → bottom sheet |

---

## Chat — thumb zone

- Composer fixed above home indicator.
- Phase strip: one line under header during stream.
- Sessions: sheet from header (not permanent drawer).
- Identity: sheet from avatar.

---

## Pulse — glance screen

- Connection state + one hero action (continue / review / chat).
- Blocked count tappable → Work tab, filtered to needs-you.
- Column counts as three compact tiles (not full kanban).

---

## You — curated back room

| Destination | Mobile v1 |
|-------------|-----------|
| Notes | Library list + reader |
| Skills | Searchable list → run in chat |
| Schedule | Cron list |
| Channels | Messaging (status-first) |
| Workshop | Settings (connection, theme, notifications) |
| Advanced | Workshop defaults (simplified tabs) |
| Workshop health | Runtime read-only |

Copy honesty: full stage routing and token forms remain desktop-first where needed.

---

## Component map

| Desktop | Mobile |
|---------|--------|
| `WorkshopShell` | `MobileShell` |
| `NavSidebar` | `MobileTabBar` |
| `HomeOverview` | `PulsePanel` |
| `KanbanBoard` | `WorkTimeline` |
| `CardInspector` + split | `WorkStory` (full screen) |
| `NewWorkAsk` | `AskSheet` (M8b) |
| `ActivityPanel` | `ActivitySheet` |
| `StatusBar` | `ConnectionPill` |
| `WorkRail` | *(removed — Work tab is the rail)* |

**Rule:** `workspace`, `chat`, `runtime`, etc. stores unchanged. Layout forks on `layout.isMobile`.

Detection: `max-width: 768px` media query (+ Tauri mobile builds). `AppShell` picks shell.

---

## Milestones

| Phase | Deliverable | Feel test |
|-------|-------------|-----------|
| **M8a** | Tab bar, Pulse, Connection pill, Activity sheet, You hub shell, `AppShell` | “Alive in 2 seconds” |
| **M8b** | Work timeline, work story, ask sheet, pull-to-refresh | “Retried without pinching” |
| **M8c** | Mobile chat frame, session sheet, phase strip | “One-handed message” |
| **M8d** | You drill-downs polished, simplified settings | “Found it without nine icons” |
| **M8e** | Push, haptics, share, `medousa://work/{id}` deep links | “Felt native” |

**Not M8:** swimlanes, split inspector, full defaults matrix, vault prose editor, stage routing editor.

---

## Packaging

- Tauri `mobile_entry_point` already in `lib.rs`.
- Safe-area insets (`env(safe-area-inset-*)`).
- No tray on mobile — badge on app icon (M8e).
- OS share sheet for job results (M8e).

---

## README line (when we ship)

> Medousa on your phone isn’t the whole workshop — it’s the heartbeat. See what’s running. Answer when it asks. Retry when it stalls. The rest waits for your desk.

---

## Document history

| Date | Change |
|------|--------|
| 2026-05-30 | Initial mobile plan — 4 tabs, M8a–M8e milestones |
| 2026-05-30 | M8a implementation started — `AppShell`, `MobileShell`, Pulse, You hub |
| 2026-05-30 | M8b shipped — `AskSheet`, pull-to-refresh, swipe-cancel, session sheet |
| 2026-05-30 | M8c shipped — mobile chat frame, phase strip, identity sheet |
| 2026-05-30 | M8d shipped — You hub polish, stack drill-downs, simplified settings |
