# How-to recipes

Short recipes. Assumes you’re Connected — see [Getting started](guide:getting-started) if not.

## Research → a written brief

```plan
title: Research → brief
subtitle: Web + Chat + Library
grouping: day

---
label: Gather
time: Step 1
icon: globe
body: Open **Web**; save useful pages to the Library.
---
label: Hand off
time: Step 2
icon: users
body: Take control for logins or CAPTCHA; hand back when Medousa should continue — [Browser](guide:browser).
---
label: Draft
time: Step 3
icon: message-circle
body: In Chat, ask for a short cited brief based on those notes.
---
label: Keep
time: Step 4
icon: book
body: Keep the result in Library.
```

## Turn a repeated chat into a schedule

```plan
title: Chat → schedule
grouping: day

---
label: Prove the prompt
time: Once
icon: check
body: Get the prompt working once in Chat.
---
label: Build the flow
time: Next
icon: zap
body: Automations → **Flows** — add Ask Medousa and/or Script steps.
---
label: Schedule it
time: Then
icon: clock
body: **New schedule** — when it runs, where results go. Run once by hand; confirm it fired.
```

→ [Automations](guide:grapheme-automations) · [Writing scripts](guide:writing-scripts)

## Pin a live feed on a custom view

```steps
title: Pin a feed

---
label: Produce output
body: Let an automation write into a feed block in a note
status: current
icon: zap
---
label: Create a view
body: Pin that note or widget — [Views](guide:views-environments)
status: pending
icon: layers
---
label: Fix stale badges
body: If the badge says stale, fix or resume the schedule
status: pending
icon: clock
```

## Share with a peer on your network

```steps
title: Peer share

---
label: Connect
body: **Peers** → Connect / trust — [Sharing](guide:sharing-phone)
status: current
icon: users
---
label: Exchange
body: Message or send a backup of views if needed
status: pending
---
label: Revoke
body: **Revoke** when you’re done
status: pending
icon: lock
```

## Try an Agent safely

```steps
title: Safe agent trial

---
label: Import
body: Automations → **Agents**
status: current
icon: sparkles
---
label: Keep it sandboxed
body: Prefer sandboxed skills; keep shell tools off until you need them — [Permissions](guide:permissions-budgets)
status: pending
icon: shield
---
label: Run once
body: **Run** once in chat before scheduling
status: pending
icon: message-circle
```

## Pair a phone for one person

```steps
title: Pair a phone

---
label: Show the QR
body: On the computer — Sharing → Phone → show QR (Shared mode: seat invite)
status: current
icon: home
---
label: Scan
body: Scan on the phone; wait for a healthy connection
status: pending
icon: globe
---
label: Forget later
body: **Forget** in the paired list to revoke
status: pending
icon: lock
```

## Recover a note

```steps
title: Recover a note

---
label: Trash
body: Deleted → Library → **Trash** → **Restore**
status: current
icon: book
---
label: Versions / conflict
body: Overwritten → Versions history (if on), or conflict **Reload** / **Keep mine**
status: pending
icon: pencil
```

→ [Trash and versions](guide:vault-recovery)

## Fix a failed schedule

```steps
title: Fix a schedule

---
label: Is it paused?
body: Check Automations → Schedules
status: current
icon: clock
---
label: Check Runtime
body: Runtime → Jobs / Delivery
status: pending
icon: cpu
---
label: Machine and channel
body: Computer online? Channel connected?
status: pending
icon: alert-triangle
```

→ [Troubleshooting](guide:troubleshooting)

## Set backup models

Settings → Medousa Agent → Models — set Chat / Vision / Dictation, then optional fallbacks.

→ [Chat](guide:chat)

Next: [FAQ and limits](guide:faq-limits) · [Find answers](guide:find-answers).
