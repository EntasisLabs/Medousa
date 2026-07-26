# Work and background jobs

**Work** is the operator-facing board for jobs that outlive a single interactive reply — background asks, long agent turns, and items waiting on you.

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

| Column | Meaning |
|--------|---------|
| **backlog** | Queued, not running yet |
| **in flight** | Running — header hint: drag to cancel |
| **wrapping up** | Finishing / consolidating |
| **blocked** | Waiting on you (often budget or similar) |
| **done** | Finished — shown when **Show done** is checked |

## How cards appear

| Source | What you see |
|--------|----------------|
| `/ask …` or Spotlight **Background task** | Background ask card; interactive chat can keep going |
| Long agent / worker turns | Turn-worker cards as the workshop syncs |
| Budget approvals | Often **blocked** until you Approve/Deny |
| Spotlight **Morning brief** (when available) | Queued job that lands you on Work |

If an interactive turn is already live, new `/ask` traffic still goes to background rather than stealing the foreground turn.

## Operate a card

1. **Click** a card → inspector (timeline, result, links to chat/note, budget actions when relevant).
2. **Drag in-flight** to the cancel drop zone → “Release to cancel this card”.
3. Use **Swimlanes** to group: none, By intent, By manuscript, By job family, By session.

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
