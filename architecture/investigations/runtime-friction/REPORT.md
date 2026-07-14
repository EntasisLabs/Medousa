# Medousa Runtime Friction Investigation Report

**Generated:** 2026-06-27  
**Scope:** Investigation only — no runtime behavior changes  
**Evidence source:** Live daemon data dir (operator machine; path redacted)  
**Corpus artifact:** local `daemon-corpus.json` only (gitignored — mined by [`scripts/mine-turn-ledger.py`](../../../scripts/mine-turn-ledger.py); do not commit)

---

## Executive summary

Web search is the dominant friction source on this host. The daemon's turn ledger shows **307 `cognition_web_search` failure digests** against **4,745 successes**, with **77% of failures** (`236/307`) caused by **`web.tavily` binding unavailable** — yet the live `capabilities.toml` overlay ranks tavily at priority **2** (highest).

Secondary friction is **prompt calibration**, not missing tools: vault/memory/identity tools are heavily used (518+ vault searches, 125 memory stores), but discovery spirals and host/worker delegation add rounds when web search fails or when STTP contradicts the one-shot `cognition_web_search` path.

**Top recommended fixes (post-investigation, not implemented):**

| Tier | Fix | Confidence |
|------|-----|------------|
| **S** | Set `preferred_provider = "duckduckgo"` in capabilities overlay; demote or remove unavailable providers (tavily, brave, xaviv) from binding list | High — ledger + overlay agree |
| **S** | Exclude `web.providers` / `web.capabilities` from `cognition_web_search` fallback chain for `mode=search` | High — code audit |
| **A** | Consolidate STTP: one-shot `cognition_web_search` on host; discover-first only in worker lane | High — prompt conflict confirmed |
| **A** | Add `cognition_web_search` to `base-researcher.yaml` allowlist | Medium — worker uses invoke spiral |
| **A** | Rename docs `cognition_finish` → `cognition_turn_finish` | High — static drift |
| **A+** | Turn-start relevance slice (identity + vault, ~800 chars); wire Home context pack selector | Medium — injection audit |

---

## Methodology

1. **Phase 0:** Built read-only ledger miner; indexed **55 sessions** from **75 ledger files** into `daemon-corpus.json`.
2. **Phase 1:** Quantified web search failures; reconstructed timelines for top pain sessions; traced binding path in `bridge_tools.rs` / `capability_catalog.rs`.
3. **Phase 2:** Static injection audit + proactive scoring on ledger sessions (overtook / balanced / too_passive).
4. **Phase 3:** Spot-checked guidance conflicts triggered by daemon traces.
5. **Phase 4:** Synthesized scored friction matrix and tiered roadmap.

**Grapheme ground truth:** `grapheme` CLI not on PATH during investigation. Installed ops inferred from ledger failure messages and live overlay at the operator `capabilities.toml`. Recommended: run `grapheme modules ops web --yaml` on operator machine to validate.

---

## Aggregate statistics (this host)

| Metric | Value |
|--------|-------|
| Ledger files mined | 55 sessions (75 files on disk) |
| Sessions with `cognition_web_search` | 20 |
| Friction scores (web sessions) | score 3 (clean): 7 · score 2: 5 · score 1 (retries): 8 · score 0: 0 |
| `cognition_web_search` digest ok / fail | 4,745 / 307 |
| Top fail reason | `binding_tavily_unavailable` — 236 (77%) |
| Other fail reasons | xaviv 27 · brave 25 · websearch facade 19 |
| Worker escalations after web pain | 3 sessions tagged |
| `work_failed` (max rounds) | 34 events across web-related ledgers |

**Top tool invocations (all sessions):**

| Tool | Count |
|------|------:|
| `cognition_web_search` | 1,247 |
| `cognition_grapheme_run` | 708 |
| `cognition_vault_search` | 518 |
| `cognition_capability_search` | 388 |
| `cognition_capability_invoke` | 388 |
| `cognition_spawn_turn_worker` | 188 |

---

## Phase 1 — Web search forensics (P0)

### Root cause: binding priority vs installed Grapheme ops

Live overlay (operator `capabilities.toml`):

```toml
[[capabilities.bindings.grapheme]]
module_op = "web.tavily"
priority = 2          # ← tried first

[[capabilities.bindings.grapheme]]
module_op = "web.providers"
priority = 5          # ← discovery, not search

[[capabilities.bindings.grapheme]]
module_op = "web.duckduckgo"
priority = 10         # ← actually works (per ok digests)
```

Embedded seed in [`capability_catalog.rs`](../../../src/capability_catalog.rs) also lists `web.providers` (5) and `web.capabilities` (8) before `web.duckduckgo` (10). Overlay adds tavily at 2.

### Code path (`cognition_web_search`)

When `mode=search` and no explicit `provider` in tool input:

1. [`web_search_settings()`](../../../src/capability_catalog.rs) resolves `preferred_provider` from env → tui_defaults → capabilities.toml `[web_search]` (commented out on this host → **None**).
2. With no preferred provider, [`web_search_binding_reference`](../../../src/bridge_tools.rs) returns **None** — no explicit binding pin.
3. [`select_binding_for_invoke`](../../../src/bridge_tools.rs) picks **lowest priority number** among available bindings → **tavily first**.
4. With `try_fallbacks=true` (default), walks full chain including discovery ops before duckduckgo.

When `preferred_provider` is set (e.g. `"tavily"`), step 2 pins `web.tavily` explicitly — same failure if unavailable.

**Mechanism confirmed:** failures are system configuration + fallback order, not model hallucination.

### Prompt contradiction (amplifies discovery spiral)

| Source | Instruction |
|--------|-------------|
| [`system_prompt.rs`](../../../src/agent_runtime/system_prompt.rs) `one_shot_invoke` | Host: **`cognition_web_search` (preferred)** |
| Same file `step_3_web_preference` | Discover **`web.providers`, `web.capabilities`** first, then `web.<provider>` |
| [`turn_worker/prompts.rs`](../../../src/agent_runtime/turn_worker/prompts.rs) HOST_BUS | Bootstrap includes `cognition_web_search`; delegate heavy work to workers |
| [`turn_worker/prompts.rs`](../../../src/agent_runtime/turn_worker/prompts.rs) `host_route_appendix` | On delegate route: **do not** call `cognition_capability_invoke` on host |

Models oscillate: capability_search spiral → web_search fail → spawn worker → invoke failures on worker (no `cognition_web_search` in `base-researcher.yaml`).

### Manuscript gap

[`base-researcher.yaml`](../../../.medousa/manuscripts/base-researcher.yaml) allowlist:

- Has: `cognition_capability_invoke`, `cognition_capability_search`, grapheme tools
- **Missing:** `cognition_web_search`

Research workers must use invoke/discovery path even when one-shot search would work.

### Session timelines (top pain)

#### 1. `medousa-home-07be7bd3-8eb8-4a43-8b05-0f7d4eb6daf1` — discovery spiral + worker gap

**Goal:** Test tavily MCP for GitHub self-hosted runner research  
**Tags:** `web_search_pain`, `worker_escalation`, `discovery_spiral`, `proactive_overtook`

| Round | Event | Outcome |
|------:|-------|---------|
| 1–5 | `cognition_capability_search` ×4, `cognition_tool_history_summary` | ok — spiral before search |
| 6 | `cognition_web_search` | **fail** — tavily unavailable |
| 7 | `cognition_spawn_turn_worker` (research) | delegated |
| W1 | `cognition_mcp_invoke` | fail |
| W2 | `cognition_capability_invoke` | fail — `unknown capability 'web_search'` |
| W3 | `cognition_capability_invoke` | fail — `no available bindings for 'web_research'` |
| W4–6 | `cognition_web_search` | ok/fail mixed |
| W7 | `cognition_turn_finish` | finalized |

**Root causes:** `binding_unavailable`, `discovery_spiral`, `capability_name_mismatch`, `worker_binding_gap`

#### 2. `medousa-home-b710ea5e-7acc-47e3-b3c8-a9712124b900` — tavily MCP request

**Goal:** Tavily search via MCP after enabling integrations  
**ws_fail:** 28 (all tavily unavailable)

| Pattern | Detail |
|---------|--------|
| Opens with | `cognition_tools_discover` fail + cap_search spiral |
| Middle | Multiple `cognition_web_search` ok interleaved with tavily fails |
| Escalation | spawn research worker after round 9 |

#### 3. `medousa-ask_medousa-daemon-ask-1781373324085` — heavy research

**Goal:** Home cannabis cultivation research (legal jurisdiction)  
**ws_fail:** 60 (all tavily) · **ws_ok:** 746 digest hits

Host succeeds with web_search mostly; still spawns worker at round 4; worker hits invoke failures then `cognition_grapheme_run` ok.

#### 4. `03a8cf9cca7e4b4d8ff3383cbdd103a5` — sustained web search session

**ws_fail:** 69 (tavily 27, xaviv 27, brave 15)  
Long run of consecutive `cognition_web_search` rounds (12+ on host) with intermittent fails — fallback eventually works but burns rounds.

#### 5. `medousa-home-f25bccec-cb77-406d-9a6a-6a2a778f939a` — windows app + research

**ws_fail:** 94 (tavily 84, brave 10)  
Opens with tools_discover fails + vault_search; spawns 3 research workers; heavy web_search on workers.

### Control group (clean sessions)

| Session | Score | Pattern |
|---------|------:|---------|
| `medousa-home-986ddc8e-31f8-4c72-9680-1e5549c67941` | 3 | ws_ok only, no cap_search spiral |
| `medousa-home-3e2f0f51-e3f0-42de-93e7-4c6c7965bf44` | 3 | Indoor bonsai research — clean |
| `medousa-ask_medousa-daemon-ask-1781364073415` | 3 | vault_search + web_search + memory_store — **balanced proactive** |

Clean sessions correlate with: duckduckgo fallback succeeding early, fewer cap_search rounds, no worker escalation.

### Web search failure classification

| Root cause tag | Digest count | System fix lever |
|----------------|-------------:|------------------|
| `binding_tavily_unavailable` | 236 | Overlay priority / preferred_provider |
| `binding_xaviv_unavailable` | 27 | Remove from overlay |
| `binding_brave_unavailable` | 25 | Remove from overlay |
| `binding_websearch_unavailable` | 19 | Facade fallback ordering |
| `discovery_spiral` | qualitative (≥3 sessions) | STTP consolidation |
| `capability_name_mismatch` | ledger traces | Prompt + catalog id hygiene |
| `worker_binding_gap` | 3 escalation sessions | Manuscript + worker allowlist |
| `max_rounds_exhausted` | 34 work_failed | Symptom of above |

---

## Phase 2 — Proactive prompting audit (P1)

### Injection audit (static)

| Context | Proactive today? | Gap |
|---------|------------------|-----|
| Identity digest | Yes, via `[MEDOUSA_IDENTITY_CONTEXT]` | Truncated to **260 chars** ([`prompt_prep.rs:401`](../../../src/agent_runtime/prompt_prep.rs)) |
| Relational memory | Ranked slice at turn start | Full graph tool-gated |
| Vault notes | **`runtime-learning` tag only** auto-injected ([`learning_artifacts.rs`](../../../src/learning_artifacts.rs)) | No general pinned-note recall |
| Context pack | When selector set | **Home sets `selected_context_pack_query: None`** ([`daemon_interactive_turn.rs:923`](../../../src/agent_runtime/daemon_interactive_turn.rs)) |
| Memory/vault unlock | Host auto-unlocks | Prompt says "call directly" — good |
| Runtime boundary | [`TURN_RUNTIME_BOUNDARY_APPENDIX`](../../../src/agent_runtime/turn_ledger.rs) | Pushes tool use; no explicit "don't over-tool" except `early_exit` in STTP |

### Daemon proactive scoring (17 tagged sessions)

| Score | Count | Session IDs (sample) |
|-------|------:|----------------------|
| **balanced** | 11 | `1781364073415` (vault+web+memory), `1781670544243`, `1781373324085` |
| **overtook** | 4 | `07be7bd3`, `b710ea5e`, `f25bccec`, `1781052803304` |
| **too_passive** | 2 | `b39340c6`, `b643ad94` |

**Over-took pattern:** High `cognition_capability_search` / `cognition_tools_discover` before substantive answer; discover fails on host ("tool not on session") then retries.

**Balanced pattern:** `cognition_vault_search` before web research; `cognition_memory_store` after decisions (see GitHub runners session `1781670544243`).

**Too-passive pattern:** Research-heavy goals with zero vault/memory/identity tools in session — agent answered from web only without persisting learnings.

### Prompt consolidation targets (report only)

1. **Web search playbook (STTP):** Host → `cognition_web_search` first; no `capability_search` for `web_research` unless first call returns `decision=deny`. Worker → same, or add to manuscript allowlist.
2. **Digest-before-recall:** "Check `[MEDOUSA_IDENTITY_CONTEXT]` and `[MEDOUSA_RUNTIME_LEARNINGS]` before `cognition_identity_recall` or vault grep."
3. **Vault write gate:** "Propose `cognition_vault_write` with `[runtime-learning]` only on explicit decisions, preferences, or operator-requested persistence — not every research turn."
4. **Early exit reinforcement:** STTP `early_exit` exists but competes with discover-first workflow nodes — dedupe `workflow(.98)` and `workflow(.99)` blocks.

---

## Phase 3 — Guidance spot-check (P2)

| ID | Claim | Verdict | Evidence |
|----|-------|---------|----------|
| **G1** | Host bus `auto` always slim registry; route ignored for bus activation | **Confirmed** | [`routing.rs:158-162`](../../../src/agent_runtime/turn_worker/routing.rs) — `Auto \| Force => true`; `_route` ignored |
| **G2** | Scheduled manuscript allowlist may be overridden by host bus | **Partial** | Home passes manuscript allowlist but host bus wraps registry ([`run.rs:830`](../../../src/agent_runtime/turn_worker/run.rs)); delegate route blocks host invoke |
| **G3** | STTP discover-first vs one-shot web search | **Confirmed conflict** | `step_3_web_preference` vs `one_shot_invoke` in same STTP tree |
| **G4** | Docs say `cognition_finish`, code uses `cognition_turn_finish` | **Confirmed drift** | [`docs/engine/agent-tools.md:42`](../../../docs/engine/agent-tools.md) |
| **G5** | Worker research lane missing `cognition_web_search` in base manuscript | **Confirmed** | [`base-researcher.yaml`](../../../.medousa/manuscripts/base-researcher.yaml) |
| **G6** | Host delegate appendix forbids invoke on host | **Confirmed** | [`host_route_appendix`](../../../src/agent_runtime/turn_worker/prompts.rs) — conflicts when web_search fails and model tries invoke before spawn |

---

## Friction matrix (scored)

Scoring: impact 40% · confidence 25% · scope 20% · S++ leverage 15%

| # | Finding | Impact | Confidence | Scope | S++ | **Weighted** |
|---|---------|:------:|:----------:|:-----:|:---:|:------------:|
| 1 | Tavily-first binding unavailable | 10 | 10 | 9 | 2 | **8.7** |
| 2 | Discovery ops in search fallback chain | 8 | 9 | 8 | 3 | **7.6** |
| 3 | STTP discover-first vs one-shot web search | 8 | 9 | 7 | 4 | **7.5** |
| 4 | Worker manuscript lacks `cognition_web_search` | 6 | 8 | 9 | 3 | **6.5** |
| 5 | Identity digest 260-char truncation | 5 | 7 | 6 | 8 | **6.0** |
| 6 | Home skips context pack selector | 4 | 8 | 7 | 9 | **6.2** |
| 7 | Docs `cognition_finish` drift | 3 | 10 | 10 | 1 | **5.5** |
| 8 | No turn-end reflection loop | 4 | 6 | 3 | 10 | **5.2** |
| 9 | Host bus route ignored in `auto` | 5 | 9 | 5 | 4 | **5.8** |
| 10 | Over-tooling on discover failures | 6 | 7 | 6 | 5 | **6.1** |

---

## Tiered fix roadmap (no implementation — awaiting approval)

### Tier S — quick wins (config / small code)

1. **capabilities.toml:** Uncomment and set `[web_search] preferred_provider = "duckduckgo"`; set `try_fallbacks = true`.
2. **Binding list:** Remove or deprioritize unavailable providers (tavily p2, brave, xaviv) until Grapheme modules installed.
3. **`bridge_tools.rs`:** For `cognition_web_search` with `mode=search`, filter fallback candidates to **search ops only** — exclude `web.providers`, `web.capabilities`.
4. **Docs:** Rename `cognition_finish` → `cognition_turn_finish` in agent-tools.md.

### Tier A — prompt consolidation

1. Single web retrieval playbook in STTP: host uses `cognition_web_search` directly; discovery steps moved to worker troubleshooting appendix only.
2. Add `cognition_web_search` to `base-researcher.yaml` tools.allow.
3. Resolve duplicate `workflow(.98)` / `workflow(.99)` STTP nodes.
4. Clarify capability id: always `web_research`, never `web_search`.

### Tier A+ — runtime awareness

1. Unified turn-start relevance ranker: identity digest + top vault notes, budget ~800 chars.
2. Wire Home app to pass context pack selector when available.
3. Expand auto-injected vault tags beyond `runtime-learning` for pinned/session notes.

### Tier S++ — deferred

1. Turn-end reflection → Locus + optional vault `runtime-learning` proposal with user consent.
2. Post-tool self-check before `cognition_turn_finish`.

---

## Artifacts produced

| File | Description |
|------|-------------|
| [`scripts/mine-turn-ledger.py`](../../../scripts/mine-turn-ledger.py) | Read-only ledger miner |
| [`daemon-corpus.json`](daemon-corpus.json) | 55 sessions, selection buckets, timelines |
| This report | Synthesis + roadmap |

---

## Success criteria checklist

- [x] Daemon corpus indexed: **55 sessions** (≥15 required)
- [x] Web search failures classified with quantified breakdown
- [x] **5+** full turn timelines documented
- [x] Proactive calibration: **17** sessions scored (≥5 required)
- [x] Tiered fix roadmap — **no code changes** applied

---

## Recommended next step

Review Tier **S** items first — they address 77% of web search failures with minimal risk. Re-run miner after config change:

```bash
python3 scripts/mine-turn-ledger.py
```

Compare `ws_fail_reason_totals.binding_tavily_unavailable` before/after on new sessions.
