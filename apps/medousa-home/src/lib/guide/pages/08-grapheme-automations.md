# Automations and scripts

**Automations** is where you write scripts, build flows, and schedule work. Open it from the rail (modes: Scripts, Agents, Flows, Schedules, History). Skip this until you need repeating jobs.

To learn the script language (Grapheme), pipes, and copy-paste starters, see [Writing scripts](guide:writing-scripts).

Related: [Writing scripts](guide:writing-scripts) · [Agents](guide:specialist-agents) · [Work](guide:work-jobs) · [Troubleshooting](guide:troubleshooting)

## Scripts workbench

Header: “Scripts workbench · write, run, add to flow.”

| Control | Shortcut / notes |
|---------|------------------|
| **Save** | ⌘/Ctrl+S |
| **Run** | ⌘/Ctrl+Enter |
| **Compile** | ⌘/Ctrl+B |
| **Optimize (AOT)** | Speeds some scripts when the button is available |
| **Add to flow** | Use this script inside a Flow |
| Output / chat panes | Toggle show/hide |

Left rail: **Scripts**, **Templates**, and **WASM** (advanced modules).

### Templates

Starters such as **Say hello**, **Search the web**, **Chain steps together**, and **Run a sandboxed command**. They’re starting points — rewrite freely.

Full source for each starter lives in [Writing scripts](guide:writing-scripts#starter-examples).

Read the run output if something fails before assuming the editor is broken.

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

Next: [Writing scripts](guide:writing-scripts) · [Specialist agents](guide:specialist-agents).
