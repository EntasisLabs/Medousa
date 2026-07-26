# Runtime telemetry

**Runtime** is live workshop diagnostics — “what is the engine doing?” Open from the dock / More / Spotlight, or the status connection control when wired.

Related: [Work and background jobs](guide:work-jobs) · [Troubleshooting](guide:troubleshooting)

## Tabs

| Tab | Meaning |
|-----|---------|
| **Now** | In motion / running / queued; active turn phase and tools, or “No active turn”; model · depth · reasoning |
| **Jobs** | Counters: enqueued, running, succeeded, failed, dead letter, outbox, recurring + last tick |
| **Delivery** | **Outbox** (endpoint, target, pending, auth, last delivery) · **Continuations** (pending / resumed / consumed / DLQ) |
| **Routing** | Stage routes — Role / Target / Policy / Fallback (edit in Settings → Models → Stages). Hidden on mobile. |

Use **Refresh** when numbers look stale.

## When to open Runtime

| Symptom | Tab |
|---------|-----|
| “Is anything running?” | **Now** |
| Job failed / dead letter | **Jobs** |
| Schedule didn’t notify | **Delivery** |
| Wrong model stage | **Routing** + Settings → Models |

Work cards are the human board; Runtime is the engine pulse. Start on Work for “what am I waiting on?”, Runtime for “what did the queue do?”

Next: [Work and background jobs](guide:work-jobs) · [MCP and packages](guide:mcp-packages).
