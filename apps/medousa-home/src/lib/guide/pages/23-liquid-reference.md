# Liquid blocks

**Advanced.** Liquid blocks are interactive pieces inside notes — cards, charts, plans, feeds, and more. Insert them from the Live slash menu under **Blocks**, or type the fences in Build.

Related: [Vault and notes](guide:vault-notes) · [Views and environments](guide:views-environments)

Each entry below shows a **live example**, then the **source** you can copy.

## Catalog

### Card

Single-entity summary. Tap to open the detail sheet when the host supports it.

```card
title: Morning notes
subtitle: Library
icon: book
summary: Keep what matters in Library so Medousa can find it later.
```

````markdown
```card
title: Morning notes
subtitle: Library
icon: book
summary: Keep what matters in Library so Medousa can find it later.
```
````

### Carousel

Swipe or step through a few related cards.

```carousel
title: Today’s picks

---
title: Chat
subtitle: Ask something
icon: message-circle
body: Open Chat and send a short hello.
---
title: Library
subtitle: Save a note
icon: book
body: Create a note while the idea is fresh.
```

````markdown
```carousel
title: Today’s picks

---
title: Chat
subtitle: Ask something
icon: message-circle
body: Open Chat and send a short hello.
---
title: Library
subtitle: Save a note
icon: book
body: Create a note while the idea is fresh.
```
````

### Actions

Compact action row (alias: `action_row`).

```actions
Open Chat | open-chat
Open Library | open-library
```

````markdown
```actions
Open Chat | open-chat
Open Library | open-library
```
````

### Callout

Toned tip / note / warn callout for emphasis.

```callout
tone: tip
title: Start simple
body: Most days you only need Chat, Library, and a healthy connection.
```

````markdown
```callout
tone: tip
title: Start simple
body: Most days you only need Chat, Library, and a healthy connection.
```
````

### Section

Labeled section with supporting prose.

```section
title: Under the chat box
subtitle: Voice · Stance · Runtime
---
Leave these alone until a turn needs a different feel or model path.
```

````markdown
```section
title: Under the chat box
subtitle: Voice · Stance · Runtime
---
Leave these alone until a turn needs a different feel or model path.
```
````

### Block

Styled prose block (font, size, spacing).

```block
id: guide-styled
font: serif
size: lg
align: left
spacing: relaxed
---
A quieter reading block inside a note — adjust chrome in Live when you want a different look.
```

````markdown
```block
id: guide-styled
font: serif
size: lg
align: left
spacing: relaxed
---
A quieter reading block inside a note — adjust chrome in Live when you want a different look.
```
````

### Chips

Compact choice or label chips (alias: `chip_group`).

```chips
- Voice | tone: accent | value: voice
Stance | tone: default
Runtime | tone: warn
```

````markdown
```chips
- Voice | tone: accent | value: voice
Stance | tone: default
Runtime | tone: warn
```
````

### Media

Image or media embed. Vault paths and https URLs both work; this demo uses an offline SVG.

```media
src: data:image/svg+xml;utf8,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%22640%22%20height%3D%22360%22%3E%0A%20%20%20%20%20%20%3Cdefs%3E%3ClinearGradient%20id%3D%22g%22%20x1%3D%220%22%20y1%3D%220%22%20x2%3D%221%22%20y2%3D%221%22%3E%0A%20%20%20%20%20%20%20%20%3Cstop%20offset%3D%220%22%20stop-color%3D%22%231e3a5f%22%2F%3E%3Cstop%20offset%3D%221%22%20stop-color%3D%22%230d9488%22%2F%3E%0A%20%20%20%20%20%20%3C%2FlinearGradient%3E%3C%2Fdefs%3E%0A%20%20%20%20%20%20%3Crect%20width%3D%22640%22%20height%3D%22360%22%20fill%3D%22url(%23g)%22%2F%3E%0A%20%20%20%20%20%20%3Ctext%20x%3D%2250%25%22%20y%3D%2250%25%22%20fill%3D%22white%22%20font-family%3D%22system-ui%2Csans-serif%22%20font-size%3D%2228%22%0A%20%20%20%20%20%20%20%20text-anchor%3D%22middle%22%20dominant-baseline%3D%22middle%22%3ELibrary%20preview%3C%2Ftext%3E%0A%20%20%20%20%3C%2Fsvg%3E
alt: Sample preview
caption: Replace with a vault image path or URL in your notes.
ratio: 16/9
```

````markdown
```media
src: data:image/svg+xml;utf8,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%22640%22%20height%3D%22360%22%3E%0A%20%20%20%20%20%20%3Cdefs%3E%3ClinearGradient%20id%3D%22g%22%20x1%3D%220%22%20y1%3D%220%22%20x2%3D%221%22%20y2%3D%221%22%3E%0A%20%20%20%20%20%20%20%20%3Cstop%20offset%3D%220%22%20stop-color%3D%22%231e3a5f%22%2F%3E%3Cstop%20offset%3D%221%22%20stop-color%3D%22%230d9488%22%2F%3E%0A%20%20%20%20%20%20%3C%2FlinearGradient%3E%3C%2Fdefs%3E%0A%20%20%20%20%20%20%3Crect%20width%3D%22640%22%20height%3D%22360%22%20fill%3D%22url(%23g)%22%2F%3E%0A%20%20%20%20%20%20%3Ctext%20x%3D%2250%25%22%20y%3D%2250%25%22%20fill%3D%22white%22%20font-family%3D%22system-ui%2Csans-serif%22%20font-size%3D%2228%22%0A%20%20%20%20%20%20%20%20text-anchor%3D%22middle%22%20dominant-baseline%3D%22middle%22%3ELibrary%20preview%3C%2Ftext%3E%0A%20%20%20%20%3C%2Fsvg%3E
alt: Sample preview
caption: Replace with a vault image path or URL in your notes.
ratio: 16/9
```
````

### Cite

Source citation with optional quote.

```cite
title: Operator’s Guide
quote: Use this manual when you want to know what a control does.
source: guide
```

````markdown
```cite
title: Operator’s Guide
quote: Use this manual when you want to know what a control does.
source: guide
```
````

### Compare

Side-by-side options with a recommendation.

```compare
title: Phone vs peer
subtitle: Two different relationships
recommendation: Phone

| | Phone | Peer |
| --- | --- | --- |
| Notes | Same as your workshop | Separate workshop |
| Setup | Pair QR on Wi‑Fi | Trust on the network |
```

````markdown
```compare
title: Phone vs peer
subtitle: Two different relationships
recommendation: Phone

| | Phone | Peer |
| --- | --- | --- |
| Notes | Same as your workshop | Separate workshop |
| Setup | Pair QR on Wi‑Fi | Trust on the network |
```
````

### Plan

Paced checklist grouped by day (or similar).

```plan
title: First hour
subtitle: Prove it works
grouping: day

---
label: Send a hello
time: Now
icon: message-circle
body: Open Chat and send one short message.
---
label: Save a note
time: Next
icon: book
body: Library → Notes — keep something worth remembering.
```

````markdown
```plan
title: First hour
subtitle: Prove it works
grouping: day

---
label: Send a hello
time: Now
icon: message-circle
body: Open Chat and send one short message.
---
label: Save a note
time: Next
icon: book
body: Library → Notes — keep something worth remembering.
```
````

### Timeline

Chronological events on a vertical rail. See also Timeline layouts below for `snapshot`.

```timeline
title: Operate a Work card
subtitle: Typical loop

---
ts: Open
label: Open inspector
detail: Click a card for timeline, result, and links.
icon: search
---
ts: Act
label: Act or cancel
detail: Drag in-flight to cancel, or finish what’s blocked.
icon: zap
```

````markdown
```timeline
title: Operate a Work card
subtitle: Typical loop

---
ts: Open
label: Open inspector
detail: Click a card for timeline, result, and links.
icon: search
---
ts: Act
label: Act or cancel
detail: Drag in-flight to cancel, or finish what’s blocked.
icon: zap
```
````

### Shortlist

Ranked picks with scores.

```shortlist
title: Where to start
subtitle: Everyday path
criteria: usefulness · simplicity
density: comfortable

---
label: Chat
summary: Ask and iterate
score: 9.5
icon: message-circle
---
label: Library
summary: Keep durable notes
score: 9.0
icon: book
```

````markdown
```shortlist
title: Where to start
subtitle: Everyday path
criteria: usefulness · simplicity
density: comfortable

---
label: Chat
summary: Ask and iterate
score: 9.5
icon: message-circle
---
label: Library
summary: Keep durable notes
score: 9.0
icon: book
```
````

### Decision

Weighted options with pros and cons.

```decision
title: With a brain or later?
subtitle: Welcome wizard
factors: answers · setup time
recommendation: With a brain

---
label: With a brain
score: 9.0
pros: Can answer in Chat | Models ready
cons: Needs a key or Offline download
---
label: Workspace only
score: 7.0
pros: Faster first open | Add models later
cons: Chat won’t answer until models are set
```

````markdown
```decision
title: With a brain or later?
subtitle: Welcome wizard
factors: answers · setup time
recommendation: With a brain

---
label: With a brain
score: 9.0
pros: Can answer in Chat | Models ready
cons: Needs a key or Offline download
---
label: Workspace only
score: 7.0
pros: Faster first open | Add models later
cons: Chat won’t answer until models are set
```
````

### Brief

One-page structured takeaway.

```brief
title: Connection check
subtitle: Before you troubleshoot
tone: research

---
heading: Look first
body: Status bar should say Connected — not Offline.
---
heading: Then try
body: Settings → Workshop → Save & test, or Start / Restart on desktop.
```

````markdown
```brief
title: Connection check
subtitle: Before you troubleshoot
tone: research

---
heading: Look first
body: Status bar should say Connected — not Offline.
---
heading: Then try
body: Settings → Workshop → Save & test, or Start / Restart on desktop.
```
````

### Dashboard

Metric tiles at a glance.

```dashboard
title: Work columns
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
label: Blocked
value: Needs you
tone: warn
---
label: Done
value: Finished
tone: success
```

````markdown
```dashboard
title: Work columns
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
label: Blocked
value: Needs you
tone: warn
---
label: Done
value: Finished
tone: success
```
````

### Chart

Data chart. This demo is `type: bar` — see Chart types for the full list.

```chart
type: bar
title: Notes this week
legend: bottom

| Day | Count |
| --- | --- |
| Mon | 4 |
| Tue | 6 |
| Wed | 3 |
```

````markdown
```chart
type: bar
title: Notes this week
legend: bottom

| Day | Count |
| --- | --- |
| Mon | 4 |
| Tue | 6 |
| Wed | 3 |
```
````

### Report

Narrative layout that can nest charts.

```report
title: Weekly pulse
subtitle: One workshop
columns: 1

A short narrative above the figure.

```chart
type: line
title: Chats started
legend: bottom

| Day | Chats |
| --- | --- |
| Mon | 2 |
| Tue | 5 |
| Wed | 4 |
```
```

````markdown
```report
title: Weekly pulse
subtitle: One workshop
columns: 1

A short narrative above the figure.

```chart
type: line
title: Chats started
legend: bottom

| Day | Chats |
| --- | --- |
| Mon | 2 |
| Tue | 5 |
| Wed | 4 |
```
```
````

### Slides

Lightweight deck inside a note.

```slides
title: Quick tour
theme: dusk
columns: 1

---
label: Welcome
layout: hero

# Medousa
Chat, notes, and the tools around them.

---
label: Next
layout: stack

Open **Chat**, then keep what matters in **Library**.
```

````markdown
```slides
title: Quick tour
theme: dusk
columns: 1

---
label: Welcome
layout: hero

# Medousa
Chat, notes, and the tools around them.

---
label: Next
layout: stack

Open **Chat**, then keep what matters in **Library**.
```
````

### Tabs

Switch between labeled panels.

```tabs
title: Workshop relationships
default: Your workshop

---
label: Your workshop
body: Your notes and chats on this computer (or the one you connected to).
---
label: Phone
body: Another screen into the same workshop — not a second brain.
```

````markdown
```tabs
title: Workshop relationships
default: Your workshop

---
label: Your workshop
body: Your notes and chats on this computer (or the one you connected to).
---
label: Phone
body: Another screen into the same workshop — not a second brain.
```
````

### Steps

Ordered procedure with optional status.

```steps
title: Pair a phone

---
label: Show the QR
body: Settings → Sharing → Phone on the computer
status: done
icon: home
---
label: Scan
body: Same Wi‑Fi; wait until the connection looks healthy
status: current
icon: globe
---
label: Forget later
body: Revoke from the paired list when you’re done
status: pending
icon: lock
```

````markdown
```steps
title: Pair a phone

---
label: Show the QR
body: Settings → Sharing → Phone on the computer
status: done
icon: home
---
label: Scan
body: Same Wi‑Fi; wait until the connection looks healthy
status: current
icon: globe
---
label: Forget later
body: Revoke from the paired list when you’re done
status: pending
icon: lock
```
````

### Accordion

Collapsible FAQ-style panels.

```accordion
title: Quick answers
multiple: true

---
label: Where do notes live?
body: In the active **workshop** — check the status bar if things look empty.
icon: book
open: true
---
label: What does Offline mean?
body: Home can’t reach the workshop yet. Try Workshop → Save & test.
icon: alert-triangle
```

````markdown
```accordion
title: Quick answers
multiple: true

---
label: Where do notes live?
body: In the active **workshop** — check the status bar if things look empty.
icon: book
open: true
---
label: What does Offline mean?
body: Home can’t reach the workshop yet. Try Workshop → Save & test.
icon: alert-triangle
```
````

### Code

Syntax-highlighted snippet with a language badge.

```code
lang: markdown
title: note.md
---
# Meeting notes

- Decision: ship the phone pair flow
- Next: write the Operator’s Guide tip
```

````markdown
```code
lang: markdown
title: note.md
---
# Meeting notes

- Decision: ship the phone pair flow
- Next: write the Operator’s Guide tip
```
````

### Tree

File or folder tree.

```tree
title: Library sketch
---
Notes/
  Projects/
    Brief.md
  Inbox/
Attachments/
```

````markdown
```tree
title: Library sketch
---
Notes/
  Projects/
    Brief.md
  Inbox/
Attachments/
```
````

### Kanban

Simple column board from markdown headings and tasks.

```kanban
## Backlog
- [ ] Draft the brief

## Doing
- [ ] Gather sources in Web

## Done
- [x] Pair phone
```

````markdown
```kanban
## Backlog
- [ ] Draft the brief

## Doing
- [ ] Gather sources in Web

## Done
- [x] Pair phone
```
````

### Feed

Last-good automation output. This demo id won’t resolve in the guide — you’ll see the empty state until a real schedule writes to the feed.

```feed
id: guide-demo-digest
datatype: md
title: Demo digest
empty: No feed output yet — wire this id to an automation schedule.
refresh: load
```

````markdown
```feed
id: guide-demo-digest
datatype: md
title: Demo digest
empty: No feed output yet — wire this id to an automation schedule.
refresh: load
```
````

## Chart types

Set `type:` on a `chart` fence. The catalog demo above uses **bar**. Other values:

`bar` · `line` · `area` · `pie` · `donut` · `radar` · `radial` · `scatter` · `combo` · `heatmap`

Start from slash **Blocks → Chart** so the table skeleton matches the type.

## Timeline layouts

Default timeline is a vertical rail with a time gutter (see **Timeline** in the catalog). Use `layout: snapshot` for a horizontal track with peek cards:

```timeline
title: Research day
subtitle: Snapshot layout
layout: snapshot

---
ts: Morning
title: Browse
meta: web
body: Save useful pages to Library.
icon: globe
---
ts: Afternoon
title: Draft
meta: notes
body: Ask Chat for a short cited brief.
icon: pencil
---
ts: Evening
title: Keep
meta: library
body: File the result in Notes.
icon: book
```

````markdown
```timeline
title: Research day
subtitle: Snapshot layout
layout: snapshot

---
ts: Morning
title: Browse
meta: web
body: Save useful pages to Library.
icon: globe
---
ts: Afternoon
title: Draft
meta: notes
body: Ask Chat for a short cited brief.
icon: pencil
---
ts: Evening
title: Keep
meta: library
body: File the result in Notes.
icon: book
```
````

## Authoring tips

1. Start from slash **Blocks** so the fence skeleton is valid.
2. Nest charts inside `report` when you need a narrative + visuals layout.
3. Keep `feed` ids stable so badges and last-good resolve.
4. Aliases: `action_row` → `actions`, `chip_group` → `chips`.
5. Export PDF/Word may flatten some interactivity — check [Vault and notes](guide:vault-notes#export-and-chat-bridges).

Next: [Vault and notes](guide:vault-notes).
