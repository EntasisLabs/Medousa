# Calendar

**Calendar** is the workshop daybook — Day, Week, and Month — with `.ics` import/export for interoperability.

Related: [Work and background jobs](guide:work-jobs) · [Grapheme and automations](guide:grapheme-automations) (scheduled automations are separate)

## Views

| Control | Behavior |
|---------|----------|
| **Day** / **Week** / **Month** | Segmented view modes |
| **Today** · Previous / Next | Move the anchor date |
| Create | Double-click (desktop) or double-tap (mobile) a day → **New event** |

On phone, the first open often switches **Month → Day** so the grid is usable.

## Events

**New event** / **Edit event** fields:

- Title (placeholder **New Event**)
- **All day** toggle
- Start / end date and time
- **Add Location**, **Add Notes**
- Submit with ⌘Enter; delete when editing

## Import / export

| Action | Result |
|--------|--------|
| **Import .ics** | Accepts `.ics` / `text/calendar` |
| **Export .ics** | Downloads `personal.ics` |

This is file interchange with other calendars — not a live two-way sync with Apple/Google Calendar.

```callout
tone: note
title: Calendar vs Schedules
body: Calendar holds human events. Automations → Schedules run scripts, prompts, and deliveries on cron. Use both; do not confuse them.
```

Next: [Navigation and surfaces](guide:navigation-surfaces).
