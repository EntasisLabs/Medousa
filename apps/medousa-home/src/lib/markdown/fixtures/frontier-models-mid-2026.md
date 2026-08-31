# Frontier models — mid 2026

```decision
title: Which frontier model for coding?
subtitle: One decision, not a catalog
recommendation: Model Alpha

---
label: Model Alpha
score: 9.1
pros: Strong agents | Fast iteration | Good docs
cons: Expensive | Closed weights
---
label: Model Beta
score: 7.8
pros: Cheaper | Open weights
cons: Weaker tooling
```

```callout
tone: note
title: How to read this brief
body: Treat launch benchmarks as provisional until independent evals land.
```

```compare
title: Coding vs general chat
recommendation: Model Alpha

| | Model Alpha | Model Beta |
| --- | --- | --- |
| Coding | Excellent | Good |
| Cost | High | Moderate |
```

```carousel
title: Model landscape

---
title: Alpha
subtitle: Flagship coding
meta: Leader · Proprietary · Early read
summary: Best early signal on agentic coding workloads.
chips: Coding | Agents | Tool use
point: Launch framing | Most early coverage echoes vendor claims. | 📰
point: Independent testing | Third-party evals usually lag launches.
---
title: Beta
subtitle: Open-weight option
meta: Value · Open weights · Community
summary: Strong community momentum with cheaper inference.
chips: Open | Community | Fine-tune
point: Ecosystem | Tooling trails proprietary stacks. | 🛠️
---
title: Gamma
subtitle: Multimodal generalist
meta: Vision · Audio · General chat
summary: Broad capability with uneven coding depth.
chips: Multimodal | General
point: Tradeoff | Better breadth than depth on code. | ⚖️
---
title: Delta
subtitle: Long-context research
meta: Context · Research | Retrieval
summary: Useful for large-doc synthesis, less for tight loops.
chips: Context | Research
point: Fit | Strong when documents dominate the task. | 📄
---
title: Epsilon
subtitle: Edge / local
meta: On-device · Privacy
summary: Viable for local workflows with modest hardware.
chips: Local | Privacy
point: Limits | Expect smaller models and narrower tools. | 💻
```

```report
title: Adoption snapshot
subtitle: North America
columns: 2

Opening prose for the quarter.

```chart
type: bar
title: Weekly active developers

| Week | Alpha | Beta |
| ---- | ----- | ---- |
| W1   | 186   | 142  |
| W2   | 305   | 198  |
```

## Deep dive

More prose after the nested chart.
```

```tabs
title: Setup paths
default: Cloud

---
label: Cloud
body: Use hosted inference with API keys in Settings → Connections.
---
label: Local
body: Install optional packages and run the daemon on the same machine.
```

```accordion
title: FAQ
multiple: true

---
label: What counts as frontier?
body: Models at the top of public leaderboards for coding and agents.
open: true
---
label: When to re-evaluate?
body: Revisit picks after major vendor releases or independent benchmarks.
```

```cite
title: Independent eval note
url: https://example.com/eval
quote: Third-party coding benchmarks usually trail launch marketing by weeks.
source: research memo
```

```brief
title: Recommendation
subtitle: One path forward
tone: research

---
heading: Pick Alpha for coding
body: Start with Model Alpha for agentic coding until independent evals shift.
---
heading: Keep Beta as fallback
body: Use Model Beta when cost or open weights matter more than peak coding score.

===
---
title: Vendor launch blog
url: https://example.com/launch
quote: Launch-day claims should be treated as directional, not final.
---
title: Community benchmark thread
url: https://example.com/thread
```
