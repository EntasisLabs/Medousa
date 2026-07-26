# Grapheme and automations

**Automations** is the workbench for scripts, flows, schedules, and history. Open it from the rail (or Library explorer modes: Scripts / Agents / Flows / Schedules / History).

Related: [Specialist agents](guide:specialist-agents) · [Work and background jobs](guide:work-jobs) · [Troubleshooting](guide:troubleshooting#schedule-or-delivery-failed)

## Scripts workbench

Header: “Scripts workbench · write, run, add to flow.”

| Control | Shortcut / notes |
|---------|------------------|
| **Save** | ⌘/Ctrl+S |
| **Run** | ⌘/Ctrl+Enter |
| **Compile** | ⌘/Ctrl+B |
| **Optimize (AOT)** | When available |
| **Add to flow** | Graduate into a Flow step |
| Output / chat panes | Toggle show/hide |

Left rail tools: **Scripts**, **Templates**, **WASM**.

### Templates (recipes)

Starter templates (UI label **Templates**): **Say hello**, **Search the web**, **Chain steps together**, **Run a sandboxed command**. They are opinionated starters — rewrite freely.

Prefer documented host modules over raw shell when a first-class op exists. Read run output before blaming the editor.

## Flows

**Add step** → choose:

| Step type | UI label | What you configure |
|-----------|----------|--------------------|
| Grapheme | **Script** | From library or source |
| Prompt | **Ask Medousa** | Instructions |
| MCP | **External tool** | Server, tool, JSON arguments |

Build a flow once by hand, then schedule it.

## Schedules

**+ New schedule** defaults to cron `0 9 * * *` in the browser timezone.

| Field | Options |
|-------|---------|
| **When** | Every day / Weekdays / Weekends / Weekly / Custom (cron) / Run manually only |
| Delivery | **Stay in Medousa** (run history) or **Telegram** (+ chat id) |
| Execution | **Quick prompt** vs **Agent turn** |
| Lifecycle | Pause / Resume — paused schedules will not fire |

Meta shows raw cron + timezone. Confirm the workshop is online at fire time — offline engines miss ticks.

## History

**History** can rebuild from conversation beats (“moments she already lived”) — select beats, name a flow, **Open**. Also use it to audit past runs alongside Runtime → Delivery.

## Feeds (last-good)

Automations do not own a “Feeds” tab. Liquid **`feed`** blocks and custom-view badges (**Live feed** / **Stale feed**) show last-good automation output. See [Liquid reference](guide:liquid-reference) and [Views and environments](guide:views-environments).

```callout
tone: warn
title: Automations still need a healthy workshop
body: Scheduled work fails quietly if the engine is offline. Check Workshops and Runtime → Delivery when a cron never fires.
```

Operator tips: keep scripts small and named after the job; run by hand once before scheduling; comment intent for future-you and agents.

Next: [Specialist agents](guide:specialist-agents).
