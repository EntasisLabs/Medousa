# Runtime status

**Advanced.** **Runtime** shows what the workshop is doing right now — useful when a job seems stuck. Open from the dock, phone More menu, or Spotlight.

Related: [Work and background jobs](guide:work-jobs) · [Troubleshooting](guide:troubleshooting)

## Tabs

| Tab | Meaning |
|-----|---------|
| **Now** | In motion / running / queued; active turn phase and tools, or “No active turn”; model · depth · reasoning |
| **Jobs** | Counters: enqueued, running, succeeded, failed, dead letter, outbox, recurring + last tick |
| **Delivery** | **Outbox** (endpoint, target, pending, auth, last delivery) · **Continuations** (pending / resumed / consumed / DLQ) |
| **Routing** | Stage routes — Role / Target / Policy / Fallback (edit in Settings → Models → Stages). Hidden on mobile. |

Use **Refresh** when numbers look stale.

## Jobs at a glance

```dashboard
title: Jobs counters
columns: 2

---
label: Enqueued
value: Waiting
tone: default
---
label: Running
value: In motion
tone: accent
---
label: Failed
value: Errored
tone: warn
---
label: Dead letter
value: Stuck
tone: warn
```

Open **Jobs** for succeeded, outbox, recurring, and last tick as well.

## When to open Runtime

```chips
- Is anything running? → Now | tone: accent
Job failed / dead letter → Jobs | tone: warn
Schedule didn’t notify → Delivery | tone: default
Wrong model stage → Routing | tone: default
```

| Symptom | Tab |
|---------|-----|
| “Is anything running?” | **Now** |
| Job failed / dead letter | **Jobs** |
| Schedule didn’t notify | **Delivery** |
| Wrong model stage | **Routing** + Settings → Models |

Work cards are the human board; Runtime is the engine pulse. Start on Work for “what am I waiting on?”, Runtime for “what did the queue do?”

Next: [Work and background jobs](guide:work-jobs) · [MCP and packages](guide:mcp-packages).
