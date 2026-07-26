# Calendar

**Calendar** is the workshop daybook — Day, Week, and Month — with `.ics` import/export for interoperability. Rich notes and reminders live in the **vault**; timed events stay in `.ics`.

Related: [Work and background jobs](guide:work-jobs) · [Grapheme and automations](guide:grapheme-automations) (scheduled automations are separate) · [Vault notes](guide:vault-notes)

## Views

| Control | Behavior |
|---------|----------|
| **Day** / **Week** / **Month** | Segmented view modes |
| **Today** · Previous / Next | Move the anchor date |
| Create (**+**) | **New Event** or **New Reminder** |
| Side rail | Mini-month navigator + upcoming agenda |

On phone, the first open often switches **Month → Day** so the grid is usable.

## Events

**New event** / **Edit event** fields:

- Title (placeholder **New Event**)
- **All day** toggle
- Start / end date and time
- **Repeats** (daily / weekly / monthly / yearly)
- **Alerts** (minutes/days before start → local notification)
- **Add Location**, short summary notes
- **Vault note** — create/open a linked markdown note for attachments and depth
- Submit with ⌘Enter; delete when editing

## Reminders

Reminders are vault checkboxes in `calendar/reminders.md` with `@due(YYYY-MM-DD)`. They overlay on Day / Week / Month (distinct from timed events). Tap a reminder chip to mark it complete.

## Import / export

| Action | Result |
|--------|--------|
| **Import .ics** | Accepts `.ics` / `text/calendar` (VEVENT; VTODO skipped) |
| **Export .ics** | Downloads `personal.ics` |

This is file interchange with other calendars — not a live two-way sync with Apple/Google Calendar.

```callout
tone: note
title: Calendar vs Schedules
body: Calendar holds human events and due reminders. Automations → Schedules run scripts, prompts, and deliveries on cron. Use both; do not confuse them.
```

Next: [Navigation and surfaces](guide:navigation-surfaces).
