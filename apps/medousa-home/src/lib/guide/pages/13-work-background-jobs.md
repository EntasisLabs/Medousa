# Work and background jobs

**Work** is a board of jobs that keep going after a normal chat reply — background asks, long tasks, and items waiting on you.

Related: [Chat](guide:chat) · [Permissions, budgets, and tool safety](guide:permissions-budgets) · [Navigation and surfaces](guide:navigation-surfaces)

## Open Work

- Rail / Spotlight → **Work**
- Chat cues after `/ask`: “Working in background”, “see Work”
- Budget bar **Work** button → linked card

Views on the surface:

| View | Role |
|------|------|
| **Hub** | Kanban + trays |
| **Asks** | List focused on ask-lane cards |

## Columns

```dashboard
title: Work board columns
columns: 2

---
label: Backlog
value: Queued
tone: default
---
label: In flight
value: Running
tone: accent
---
label: Wrapping up
value: Finishing
tone: default
---
label: Blocked
value: Needs you
tone: warn
---
label: Done
value: Finished
tone: success
```

**Done** shows when **Show done** is checked. In flight’s header hint: drag to cancel.

## How cards appear

| Source | What you see |
|--------|----------------|
| `/ask …` or Spotlight **Background task** | Background ask card; interactive chat can keep going |
| Long agent / worker turns | Turn-worker cards as the workshop syncs |
| Budget approvals | Often **blocked** until you Approve/Deny |
| Spotlight **Morning brief** (when available) | Queued job that lands you on Work |

If an interactive turn is already live, new `/ask` traffic still goes to background rather than stealing the foreground turn.

## Operate a card

```timeline
title: Operate a card
subtitle: Typical loop
granularity: day

---
ts: 1
label: Open inspector
detail: Click a card for timeline, result, chat/note links, and budget actions.
icon: search
---
ts: 2
label: Cancel if needed
detail: Drag an in-flight card to the cancel drop zone.
icon: x
---
ts: 3
label: Group with swimlanes
detail: none · By intent · By manuscript · By job family · By session.
icon: layers
```

```callout
tone: tip
title: Blocked usually means you
body: Check the card inspector and chat for a budget or permission bar before assuming the engine died.
```

## Retention

**Settings → Preferences → Work cards**

| Setting | Effect |
|---------|--------|
| **Hide from board** | Hours after done before the card leaves the board |
| **Clear archives** | Days before archived work is wiped |

Tune these if the board fills with completed noise.

## Work vs Runtime

| Surface | Audience | Shows |
|---------|----------|--------|
| **Work** | Operators | Cards, asks, cancel, inspect |
| **Runtime** | Diagnostics | **Now**, **Jobs**, **Delivery**, **Routing** — queues, failures, dead letters, delivery endpoints |

When a schedule “didn’t deliver,” check Runtime → **Delivery** as well as Automations history. Work answers “what am I waiting on?”; Runtime answers “what did the engine enqueue?”

## Slash and Spotlight

| Action | How |
|--------|-----|
| Background ask | `/ask …` in chat, or Spotlight **Background task** |
| Budget list / approve / deny | `/budget …` — see [Permissions](guide:permissions-budgets) |

Next: [Troubleshooting](guide:troubleshooting) if a card sits in blocked or in flight too long.
